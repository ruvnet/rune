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
