use rune_core::contract::Verdict;
use rune_eval::{EvalRequest, Evaluator, RuneEvaluator};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str =
    "usage: rune-eval --contract c.json --baseline <dir> --candidate <dir> --workdir <dir> \
[--out report.json] [--expect-digest <hex>] [--cargo-home <dir>] [--allow-degraded] [--no-pin]\n\
exit: 0 accept, 1 reject, 2 usage or internal error";

fn parse() -> Result<(EvalRequest, Option<PathBuf>), String> {
    let mut a = std::env::args().skip(1);
    let (mut contract, mut baseline, mut candidate, mut workdir, mut out) =
        (None, None, None, None, None);
    let (mut digest, mut cargo_home, mut degraded, mut pin) = (None, None, false, true);
    while let Some(flag) = a.next() {
        let mut val = |name: &str| a.next().ok_or(format!("{name} needs a value"));
        match flag.as_str() {
            "--contract" => contract = Some(PathBuf::from(val("--contract")?)),
            "--baseline" => baseline = Some(PathBuf::from(val("--baseline")?)),
            "--candidate" => candidate = Some(PathBuf::from(val("--candidate")?)),
            "--workdir" => workdir = Some(PathBuf::from(val("--workdir")?)),
            "--out" => out = Some(PathBuf::from(val("--out")?)),
            "--expect-digest" => digest = Some(val("--expect-digest")?),
            "--cargo-home" => cargo_home = Some(PathBuf::from(val("--cargo-home")?)),
            "--allow-degraded" => degraded = true,
            "--no-pin" => pin = false,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let need = |o: Option<PathBuf>, n: &str| o.ok_or(format!("missing {n}"));
    let mut req = EvalRequest::new(
        &need(contract, "--contract")?,
        &need(baseline, "--baseline")?,
        &need(candidate, "--candidate")?,
        &need(workdir, "--workdir")?,
    );
    req.expect_contract_digest = digest;
    req.cargo_home = cargo_home;
    req.allow_degraded = degraded;
    req.pin = pin;
    Ok((req, out))
}

fn main() -> ExitCode {
    let (req, out) = match parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("rune-eval: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let report = match RuneEvaluator.evaluate(&req) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("rune-eval: {e}");
            return ExitCode::from(2);
        }
    };
    let json = serde_json::to_string_pretty(&report).expect("report serializes");
    match out {
        Some(p) => {
            if let Err(e) = std::fs::write(&p, &json) {
                eprintln!("rune-eval: cannot write {}: {e}", p.display());
                return ExitCode::from(2);
            }
        }
        None => println!("{json}"),
    }
    match report.report.verdict {
        Verdict::Accept => ExitCode::from(0),
        Verdict::Reject(_) => ExitCode::from(1),
    }
}
