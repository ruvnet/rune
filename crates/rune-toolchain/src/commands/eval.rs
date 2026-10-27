//! `rune eval`: the independent evaluator (ADR 005), in process. Same
//! behavior as the `rune-eval` binary; additionally records the report.

use super::common::{load_contract, short};
use super::Failure;
use clap::Args as ClapArgs;
use rune_core::contract::Verdict;
use rune_core::Store;
use rune_eval::{EvalRequest, Evaluator, FullReport, RuneEvaluator};
use rune_gc::{put_object, Kind};
use rune_memory::{Evidence, FailureScope};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(ClapArgs)]
pub struct Args {
    /// Frozen evaluation contract (JSON)
    #[arg(long)]
    contract: PathBuf,
    /// Baseline source tree
    #[arg(long)]
    baseline: PathBuf,
    /// Candidate source tree
    #[arg(long)]
    candidate: PathBuf,
    /// Write the full report here instead of printing it
    #[arg(long)]
    out: Option<PathBuf>,
    /// Continue when network-namespace or rlimit isolation is unavailable (still reported)
    #[arg(long)]
    allow_degraded: bool,
    /// Scratch directory (default: a fresh directory under the system temp dir, removed afterwards)
    #[arg(long)]
    workdir: Option<PathBuf>,
    /// CARGO_HOME for the sandboxed runs (default: inside the scratch directory)
    #[arg(long)]
    cargo_home: Option<PathBuf>,
    /// Trusted runner signing key, outside the workspace and both source trees
    #[arg(long, requires_all = ["receipt_out", "out"])]
    signing_key: Option<PathBuf>,
    /// Write a signed provenance receipt; never creates or configures a trust anchor
    #[arg(long, requires = "signing_key")]
    receipt_out: Option<PathBuf>,
}

fn scratch() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("rune-eval-{}-{nanos}", std::process::id()))
}

pub(super) fn run(ws: &Path, store: &Store, a: &Args) -> Result<(), Failure> {
    let contract = load_contract(&a.contract)?;
    let identity = if let Some(key) = &a.signing_key {
        let key = fs::canonicalize(key)?;
        for dir in [ws, a.baseline.as_path(), a.candidate.as_path()] {
            if key.starts_with(fs::canonicalize(dir)?) {
                return Err(Failure::Other(
                    "signing key must be outside workspace and source trees".into(),
                ));
            }
        }
        for tree in [&a.baseline, &a.candidate] {
            if tree.join(".cargo/config").exists() || tree.join(".cargo/config.toml").exists() {
                return Err(Failure::Other(
                    "signed native evaluation does not support Cargo config overrides".into(),
                ));
            }
        }
        let base = store.current()?;
        let baseline = rune_core::tree_digest(&a.baseline)?;
        if baseline != base.source_digest {
            return Err(Failure::Other(
                "signed evaluation baseline must match approved plan".into(),
            ));
        }
        let compiler = rune_cargo::toolchain_id()?;
        let host = compiler
            .lines()
            .find_map(|l| l.strip_prefix("host: "))
            .ok_or_else(|| Failure::Other("compiler host identity missing".into()))?;
        let target = super::common::descriptor(host, &[])?;
        Some(rune_target::receipt::EvaluationIdentity {
            schema: rune_target::receipt::SCHEMA.into(),
            report_sha256: String::new(),
            baseline_digest: baseline,
            compiler_id: rune_core::hex_sha256(compiler.as_bytes()),
            target_digest: target.digest(),
            policy_digest: super::common::policy_digest(&base.policy),
            evaluator_version: String::new(),
            degraded: false,
        })
    } else {
        None
    };
    let (workdir, temp) = match &a.workdir {
        Some(w) => (w.clone(), false),
        None => (scratch(), true),
    };
    let mut req = EvalRequest::new(&a.contract, &a.baseline, &a.candidate, &workdir);
    req.allow_degraded = a.allow_degraded;
    req.cargo_home = a.cargo_home.clone();
    let result = RuneEvaluator.evaluate(&req);
    if temp {
        let _ = fs::remove_dir_all(&workdir);
    }
    let full = result?;
    let json = serde_json::to_string_pretty(&full)?;
    if let Some(mut identity) = identity {
        if full.report.baseline_digest != identity.baseline_digest
            || rune_core::tree_digest(&a.baseline)? != full.report.baseline_digest
            || rune_core::tree_digest(&a.candidate)? != full.report.candidate_digest
            || super::common::compiler_id()? != identity.compiler_id
        {
            return Err(Failure::Other(
                "evaluation inputs changed; refusing to sign".into(),
            ));
        }
        identity.report_sha256 = rune_core::hex_sha256(json.as_bytes());
        identity.evaluator_version = full.evidence.evaluator.clone();
        identity.degraded = !full.evidence.isolation_unavailable.is_empty();
        let receipt = rune_target::receipt::EvaluationReceipt::sign(
            identity,
            a.signing_key.as_ref().expect("checked above"),
        )
        .map_err(|e| Failure::Other(format!("sign evaluation: {e}")))?;
        fs::write(
            a.receipt_out
                .as_ref()
                .expect("clap requires receipt output"),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
    }
    match &a.out {
        Some(p) => {
            fs::write(p, &json).map_err(|e| Failure::Other(format!("{}: {e}", p.display())))?
        }
        None => println!("{json}"),
    }
    let r = &full.report;
    eprintln!(
        "rune eval: candidate {} speedup {:.3}x (95% CI {:.3}..{:.3}, {} samples)",
        short(&r.candidate_digest),
        r.speedup_mean,
        r.speedup_ci95_low,
        r.speedup_ci95_high,
        r.samples
    );
    record(ws, store, &full, &contract, &json)?;
    match &r.verdict {
        Verdict::Accept => {
            eprintln!("rune eval: ACCEPT (recorded as validated evidence)");
            Ok(())
        }
        Verdict::Reject(why) => Err(Failure::Other(format!("REJECT: {why:?}"))),
    }
}

/// Every report is kept under `.rune/eval/reports`. Only an Accept that also
/// passes the memory crate's consistency checks becomes validated evidence.
fn record(
    ws: &Path,
    store: &Store,
    full: &FullReport,
    contract: &rune_core::contract::EvalContract,
    json: &str,
) -> Result<(), Failure> {
    let r = &full.report;
    let name = format!("{}-{}.json", r.candidate_digest, short(&r.contract_digest));
    let dir = ws.join(".rune/eval");
    let io = |p: &Path, e: std::io::Error| Failure::Other(format!("{}: {e}", p.display()));
    for sub in ["reports", "evidence"] {
        fs::create_dir_all(dir.join(sub)).map_err(|e| io(&dir, e))?;
    }
    let path = dir.join("reports").join(&name);
    fs::write(&path, json).map_err(|e| io(&path, e))?;
    fs::write(dir.join("latest"), &name).map_err(|e| io(&dir, e))?;
    if r.verdict != Verdict::Accept {
        return Ok(());
    }
    let ev = Evidence {
        recipe_id: String::new(),
        report: r.clone(),
        target_digest: String::new(),
        compiler_id: String::new(),
        policy_digest: String::new(),
        evaluator_version: full.evidence.evaluator.clone(),
        sample_path: format!(".rune/eval/reports/{name}"),
        cost_micro_usd: 0,
        scope: FailureScope::default(),
    };
    ev.validate(contract).map_err(|e| {
        Failure::Other(format!(
            "accept report failed validation, not recorded: {e}"
        ))
    })?;
    let epath = dir.join("evidence").join(&name);
    fs::write(&epath, json).map_err(|e| io(&epath, e))?;
    if let Some(plan_id) = store.current_id().ok().flatten() {
        put_object(ws, Kind::Evidence, &plan_id, None, json.as_bytes())?;
    }
    Ok(())
}
