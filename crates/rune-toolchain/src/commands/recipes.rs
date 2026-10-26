//! `rune recipes`: evidence memory under `.rune/memory` (ADR 004). Search
//! ranks; it never approves. Evidence enters only through validation.

use super::common::{compiler_id, descriptor, policy_digest, short, ReportFile};
use super::{common::load_contract, Failure};
use clap::{Args as ClapArgs, Subcommand};
use rune_core::Store as PlanStore;
use rune_memory::{
    EditTemplate, Evidence, FailureScope, Fingerprint, HashedFeatureRanker, Ranker, Recipe,
    Retention, Snapshot, Status, Store, Visibility,
};
use std::fs;
use std::path::{Path, PathBuf};

const RANK_LABEL: &str = "ranking only \u{2014} not approval";

#[derive(Subcommand)]
pub enum Cmd {
    /// Add a proposed recipe (approval is never written here)
    Add(Box<AddArgs>),
    /// Rank recipes against a target fingerprint
    Search(Box<SearchArgs>),
    /// Show one recipe with its folded evidence and failures
    Show {
        /// Recipe id (a unique prefix is enough)
        id: String,
        #[arg(long, default_value = "local")]
        tenant: String,
    },
    /// Validate an evaluator report and add it as evidence for a recipe
    Ingest(Box<IngestArgs>),
    /// Freeze an immutable snapshot of the memory for one campaign
    Snapshot {
        #[arg(long)]
        campaign: String,
        #[arg(long, default_value = "local")]
        tenant: String,
    },
}

#[derive(ClapArgs, Default)]
pub struct FpArgs {
    /// Target triple (default: the host)
    #[arg(long)]
    target: Option<String>,
    #[arg(long = "cpu-feature")]
    cpu_features: Vec<String>,
    #[arg(long = "source-tag")]
    source_tags: Vec<String>,
    #[arg(long = "workload-tag")]
    workload_tags: Vec<String>,
    /// Compiler id (default: digest of `rustc -vV`)
    #[arg(long)]
    compiler_id: Option<String>,
    #[arg(long)]
    abi: Option<String>,
    #[arg(long, default_value = "")]
    policy_digest: String,
    /// Read the whole fingerprint from a JSON file instead
    #[arg(long)]
    fingerprint: Option<PathBuf>,
}

#[derive(ClapArgs)]
pub struct AddArgs {
    #[command(flatten)]
    fp: FpArgs,
    #[arg(long)]
    descriptor: String,
    #[arg(long)]
    baseline_hash: String,
    #[arg(long)]
    candidate_hash: String,
    #[arg(long, default_value = "manual")]
    proposer: String,
    #[arg(long, default_value = "local")]
    tenant: String,
    #[arg(long)]
    parent: Option<String>,
    #[arg(long, default_value = "")]
    find_pattern: String,
    #[arg(long, default_value = "")]
    replace_pattern: String,
    #[arg(long, default_value = "")]
    description: String,
    #[arg(long, default_value = "rune-eval")]
    evaluation_version: String,
    /// Share across tenants (default: private)
    #[arg(long)]
    shared: bool,
}

#[derive(ClapArgs)]
pub struct SearchArgs {
    #[command(flatten)]
    fp: FpArgs,
    #[arg(short = 'k', long, default_value_t = 5)]
    limit: usize,
    #[arg(long, default_value = "local")]
    tenant: String,
}

#[derive(ClapArgs)]
pub struct IngestArgs {
    #[arg(long)]
    recipe: String,
    #[arg(long)]
    report: PathBuf,
    /// The contract the report was produced under
    #[arg(long)]
    contract: PathBuf,
    /// Target the evidence applies to
    #[arg(long, default_value = "x86_64")]
    target: String,
    /// Workspace-relative path of the raw samples (default: the report, if inside the workspace)
    #[arg(long)]
    sample_path: Option<String>,
    #[arg(long, default_value_t = 0)]
    cost_micro_usd: u64,
    /// Policy digest (default: that of the current plan)
    #[arg(long)]
    policy_digest: Option<String>,
}

fn open(ws: &Path) -> Result<Store, Failure> {
    Ok(Store::open(ws)?)
}

fn fingerprint(a: &FpArgs) -> Result<Fingerprint, Failure> {
    if let Some(p) = &a.fingerprint {
        let b = fs::read(p).map_err(|e| Failure::Other(format!("{}: {e}", p.display())))?;
        return Ok(serde_json::from_slice::<Fingerprint>(&b)?);
    }
    let d = descriptor(a.target.as_deref().unwrap_or("x86_64"), &a.cpu_features)?;
    Ok(Fingerprint {
        source_tags: a.source_tags.clone(),
        target_triple: d.triple,
        cpu_features: a.cpu_features.clone(),
        compiler_id: match &a.compiler_id {
            Some(c) => c.clone(),
            None => compiler_id()?,
        },
        abi: a.abi.clone().unwrap_or(d.abi),
        workload_tags: a.workload_tags.clone(),
        policy_digest: a.policy_digest.clone(),
    })
}

fn find(snap: &Snapshot, id: &str) -> Result<Recipe, Failure> {
    let hits: Vec<&Recipe> = snap
        .recipes()
        .iter()
        .filter(|r| r.id.starts_with(id))
        .collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(Failure::Other(format!("no recipe matches `{id}`"))),
        _ => Err(Failure::Other(format!("`{id}` is ambiguous"))),
    }
}

pub(super) fn run(ws: &Path, cmd: Cmd) -> Result<(), Failure> {
    match cmd {
        Cmd::Add(a) => add(ws, &a),
        Cmd::Search(a) => search(ws, &a),
        Cmd::Show { id, tenant } => {
            let snap = open(ws)?.freeze(&tenant)?;
            let r = find(&snap, &id)?;
            println!("{}", serde_json::to_string_pretty(&r)?);
            println!("status {:?} is a label, not an approval", r.status);
            Ok(())
        }
        Cmd::Ingest(a) => ingest(ws, &a),
        Cmd::Snapshot { campaign, tenant } => snapshot(ws, &campaign, &tenant),
    }
}

fn add(ws: &Path, a: &AddArgs) -> Result<(), Failure> {
    let recipe = Recipe {
        id: String::new(),
        parent: a.parent.clone(),
        proposer: a.proposer.clone(),
        descriptor: a.descriptor.clone(),
        fingerprint: fingerprint(&a.fp)?,
        template: EditTemplate {
            find_pattern: a.find_pattern.clone(),
            replace_pattern: a.replace_pattern.clone(),
            description: a.description.clone(),
        },
        baseline_hash: a.baseline_hash.clone(),
        candidate_hash: a.candidate_hash.clone(),
        evaluation_version: a.evaluation_version.clone(),
        evidence: vec![],
        failures: vec![],
        retention: Retention::Standard,
        status: Status::Proposed,
        tenant: a.tenant.clone(),
        visibility: if a.shared {
            Visibility::Shared
        } else {
            Visibility::Private
        },
    }
    .sealed();
    open(ws)?.add_recipe(&recipe)?;
    println!("recipe {} added (proposed; carries no approval)", recipe.id);
    Ok(())
}

fn search(ws: &Path, a: &SearchArgs) -> Result<(), Failure> {
    let q = fingerprint(&a.fp)?;
    let snap = open(ws)?.freeze(&a.tenant)?;
    println!(
        "{RANK_LABEL}: similarity orders candidates for evaluation, it grants nothing (snapshot {})",
        short(snap.id())
    );
    for (i, h) in HashedFeatureRanker
        .rank(&q, &snap, a.limit)
        .iter()
        .enumerate()
    {
        println!(
            "{:>2}. {}  score {:.4}  status {:?}  [{RANK_LABEL}]",
            i + 1,
            h.id,
            h.score,
            h.status
        );
        for f in &h.failures {
            println!("      failed: {} ({:?})", f.reason, f.scope);
        }
    }
    Ok(())
}

fn ingest(ws: &Path, a: &IngestArgs) -> Result<(), Failure> {
    let contract = load_contract(&a.contract)?;
    let rf = ReportFile::load(&a.report)?;
    let desc = descriptor(&a.target, &[])?;
    let policy = match &a.policy_digest {
        Some(p) => p.clone(),
        None => PlanStore::open(ws)
            .current()
            .map(|p| policy_digest(&p.policy))
            .unwrap_or_default(),
    };
    let sample_path = match &a.sample_path {
        Some(s) => s.clone(),
        None => {
            let abs = fs::canonicalize(&a.report)?;
            let root = fs::canonicalize(ws)?;
            abs.strip_prefix(&root)
                .map_err(|_| {
                    Failure::Other("report is outside the workspace; pass --sample-path".into())
                })?
                .to_string_lossy()
                .into_owned()
        }
    };
    let ev = Evidence {
        recipe_id: a.recipe.clone(),
        evaluator_version: rf.evaluator_version(),
        report: rf.report,
        target_digest: desc.digest(),
        compiler_id: compiler_id()?,
        policy_digest: policy,
        sample_path,
        cost_micro_usd: a.cost_micro_usd,
        scope: FailureScope {
            target_triple: Some(desc.triple),
            ..FailureScope::default()
        },
    };
    open(ws)?.ingest(&ev, &contract)?;
    println!(
        "evidence validated and stored for recipe {}",
        short(&a.recipe)
    );
    Ok(())
}

fn snapshot(ws: &Path, campaign: &str, tenant: &str) -> Result<(), Failure> {
    let ok = !campaign.is_empty()
        && campaign.len() <= 64
        && campaign
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !ok {
        return Err(Failure::Other(
            "campaign id must be 1-64 chars of [A-Za-z0-9_-]".into(),
        ));
    }
    let snap = open(ws)?.freeze(tenant)?;
    let dir = ws.join(".rune/memory/snapshots");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{campaign}.json"));
    let body = serde_json::to_vec_pretty(&serde_json::json!({
        "campaign": campaign,
        "id": snap.id(),
        "tenant": snap.tenant(),
        "recipes": snap.recipes(),
    }))?;
    // create_new: a campaign's snapshot is written once and never replaced.
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => Failure::Other(format!(
                "snapshot for campaign `{campaign}` already exists and is immutable"
            )),
            _ => Failure::Other(format!("{}: {e}", path.display())),
        })?;
    std::io::Write::write_all(&mut f, &body)?;
    println!(
        "snapshot {} frozen for campaign {campaign} ({} recipes)",
        snap.id(),
        snap.len()
    );
    Ok(())
}
