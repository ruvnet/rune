//! ADR 004 equal-budget random control study driver.
//!
//! `gen` writes the frozen workloads and candidates; `verify` checks that every
//! baseline and candidate passes its own visible and holdout tests (correctness
//! of the generator only; no timing); `run` drives the real `rune-eval` over
//! every applicable (recipe, workload) pair; `analyze` builds the outcome
//! matrix and the `ControlStudy`.

mod analyze;
mod code;
mod gen;
mod harness;
mod recipes;
mod run;
mod spec;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: rune-study <command> [--key value ...]\n\
  gen     --study DIR --candidates DIR           write workloads/, manifest.json, candidates\n\
  verify  --study DIR --candidates DIR --scratch DIR [--jobs N]\n\
  run     --study DIR --candidates DIR --scratch DIR --eval-bin PATH [--jobs N] [--cores 2,3,4,5]\n\
  analyze --study DIR\n\
  describe                                       print the workload / split / tag table";

fn flags(args: &[String]) -> Result<std::collections::HashMap<String, String>, String> {
    let mut m = std::collections::HashMap::new();
    let mut it = args.iter();
    while let Some(k) = it.next() {
        let key = k
            .strip_prefix("--")
            .ok_or(format!("unexpected argument {k}"))?;
        m.insert(
            key.to_string(),
            it.next().ok_or(format!("{k} needs a value"))?.clone(),
        );
    }
    Ok(m)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let res = flags(&args[1..]).and_then(|f| {
        let path = |k: &str| -> Result<PathBuf, String> {
            f.get(k).map(PathBuf::from).ok_or(format!("missing --{k}"))
        };
        let jobs: usize = f.get("jobs").and_then(|j| j.parse().ok()).unwrap_or(4);
        match cmd.as_str() {
            "gen" => {
                let s = path("study")?;
                let m = gen::generate(
                    &s.join("workloads"),
                    &path("candidates")?,
                    &s.join("manifest.json"),
                )?;
                let n: usize = m.workloads.iter().map(|w| w.candidates.len()).sum();
                println!("{} workloads, {n} candidates", m.workloads.len());
                Ok(())
            }
            "verify" => run::verify(
                &path("study")?,
                &path("candidates")?,
                &path("scratch")?,
                jobs,
            ),
            "run" => {
                let cores: Vec<usize> = f
                    .get("cores")
                    .map(|c| c.split(',').filter_map(|x| x.parse().ok()).collect())
                    .unwrap_or_else(|| vec![2, 3, 4, 5]);
                run::sweep(&run::SweepCfg {
                    study: path("study")?,
                    candidates: path("candidates")?,
                    scratch: path("scratch")?,
                    eval_bin: path("eval-bin")?,
                    jobs,
                    cores,
                })
            }
            "analyze" => analyze::analyze(&path("study")?),
            "describe" => {
                spec::describe();
                Ok(())
            }
            other => Err(format!("unknown command {other}")),
        }
    });
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rune-study: {e}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
