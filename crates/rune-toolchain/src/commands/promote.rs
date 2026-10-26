//! `rune promote`: approve a new plan bound to the exact tested artifact.
//!
//! The approval key covers the candidate digest, target digest, compiler id,
//! policy digest, contract digest and evaluator version (ADR 004). It is
//! derived here and stored beside the plan; nothing inferred from similarity
//! can produce it.

use super::common::{compiler_id, descriptor, load_contract, policy_digest, short, ReportFile};
use super::Failure;
use clap::Args as ClapArgs;
use rune_cargo as cargo;
use rune_core::contract::Verdict;
use rune_core::{hex_sha256, tree_digest, Plan, Store};
use rune_memory::{Evidence, FailureScope, KeyInputs};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(ClapArgs)]
pub struct Args {
    /// Plan whose policy is carried forward: a plan id or `current`
    #[arg(long, default_value = "current")]
    plan: String,
    /// Evaluator report (JSON) for the artifact being promoted. Keep it outside the workspace or under .rune/: any other file in the tree changes the artifact digest
    #[arg(long)]
    report: PathBuf,
    /// The frozen contract the report must have been produced under
    #[arg(long)]
    contract: PathBuf,
    /// Target the approval applies to: x86_64, aarch64, wasm32 or a full triple
    #[arg(long)]
    target: String,
    /// Required CPU features of the target descriptor (part of the target digest)
    #[arg(long = "cpu-feature")]
    cpu_features: Vec<String>,
}

fn refuse(msg: impl Into<String>) -> Failure {
    Failure::Other(format!("promotion refused: {}", msg.into()))
}

pub(super) fn run(ws: &Path, store: &Store, a: &Args) -> Result<(), Failure> {
    let base = if a.plan == "current" {
        store.current()?
    } else {
        store.load(&a.plan)?
    };
    let contract = load_contract(&a.contract)?;
    let rf = ReportFile::load(&a.report)?;
    let report = &rf.report;

    if report.verdict != Verdict::Accept {
        return Err(refuse(format!(
            "the report verdict is {:?}, not Accept",
            report.verdict
        )));
    }
    if report.contract_digest != contract.digest() {
        return Err(refuse(
            "the report's contract digest does not match the contract",
        ));
    }
    let artifact = tree_digest(ws)?;
    if report.candidate_digest != artifact {
        return Err(refuse(format!(
            "the report evaluated candidate {} but this workspace is {}",
            short(&report.candidate_digest),
            short(&artifact)
        )));
    }
    let evidence_check = Evidence {
        recipe_id: String::new(),
        report: report.clone(),
        target_digest: String::new(),
        compiler_id: String::new(),
        policy_digest: String::new(),
        evaluator_version: rf.evaluator_version(),
        sample_path: "report".into(),
        cost_micro_usd: 0,
        scope: FailureScope::default(),
    };
    evidence_check
        .validate(&contract)
        .map_err(|e| refuse(format!("the report is inconsistent: {e}")))?;

    let target = descriptor(&a.target, &a.cpu_features)?;
    if target.runtime == "unavailable" {
        return Err(refuse(format!(
            "target {} cannot be approved: specification required",
            target.triple
        )));
    }
    let inputs = KeyInputs {
        target_digest: target.digest(),
        compiler_id: compiler_id()?,
        policy_digest: policy_digest(&base.policy),
        contract_digest: contract.digest(),
        evaluator_version: rf.evaluator_version(),
    };
    let key = inputs.key_for(&report.candidate_digest);

    let parent = store.current_id()?;
    let toolchain = cargo::toolchain_id()?;
    let plan = Plan::new(
        artifact.clone(),
        cargo::lock_digest(ws)?,
        toolchain,
        target.triple.clone(),
        base.policy.clone(),
        parent.clone(),
    );
    let id = plan.id();
    let dir = ws.join(".rune/promotions");
    fs::create_dir_all(&dir)?;
    let record = serde_json::json!({
        "plan_id": id,
        "parent": parent,
        "approval_key": key,
        "inputs": inputs,
        "candidate_digest": report.candidate_digest,
        "baseline_digest": report.baseline_digest,
        "report_sha256": hex_sha256(&fs::read(&a.report)?),
        "speedup_mean": report.speedup_mean,
        "speedup_ci95_low": report.speedup_ci95_low,
    });
    fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_vec_pretty(&record)?,
    )?;
    store.approve(&plan)?;
    println!("promoted: approved plan {id}");
    println!("  parent        {}", parent.as_deref().unwrap_or("none"));
    println!("  approval key  {key}");
    println!(
        "  bound to      candidate {} / target {} ({}) / compiler {} / policy {} / contract {} / evaluator {}",
        short(&report.candidate_digest),
        target.triple,
        short(&inputs.target_digest),
        short(&inputs.compiler_id),
        short(&inputs.policy_digest),
        short(&inputs.contract_digest),
        inputs.evaluator_version
    );
    println!("  `rune rollback` restores the previous plan");
    Ok(())
}
