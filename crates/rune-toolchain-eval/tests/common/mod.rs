#![allow(dead_code)]
use rune_core::contract::{Reject, Verdict};
use rune_eval::{EvalRequest, Evaluator, FullReport, RuneEvaluator};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static SERIAL: Mutex<()> = Mutex::new(());

/// Timing-sensitive evaluations must not overlap each other.
pub fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn kernels() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/kernels")
}

pub fn contract() -> PathBuf {
    kernels().join("sqdist-contract.json")
}

pub fn baseline() -> PathBuf {
    kernels().join("sqdist")
}

pub fn candidate(name: &str) -> PathBuf {
    kernels().join("sqdist-candidates").join(name)
}

pub fn eval(contract: &Path, cand: &Path, work: &Path) -> FullReport {
    RuneEvaluator
        .evaluate(&EvalRequest::new(contract, &baseline(), cand, work))
        .expect("evaluator ran")
}

pub fn eval_fixture(name: &str) -> FullReport {
    let work = tempfile::tempdir().unwrap();
    eval(&contract(), &candidate(name), work.path())
}

pub fn rejection(r: &FullReport) -> &Reject {
    match &r.report.verdict {
        Verdict::Reject(x) => x,
        Verdict::Accept => panic!("expected reject, got accept: {:?}", r.report),
    }
}

/// Copy contract + holdout + a candidate into a scratch area so a test can
/// mutate them. Returns (contract path, candidate dir).
pub fn scratch_copy(root: &Path, cand: &str) -> (PathBuf, PathBuf) {
    let cdir = root.join("frozen");
    std::fs::create_dir_all(&cdir).unwrap();
    std::fs::copy(contract(), cdir.join("contract.json")).unwrap();
    let mut c: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cdir.join("contract.json")).unwrap()).unwrap();
    c["holdout_dir"] = "holdout".into();
    std::fs::write(
        cdir.join("contract.json"),
        serde_json::to_vec_pretty(&c).unwrap(),
    )
    .unwrap();
    copy_tree(&kernels().join("sqdist-holdout"), &cdir.join("holdout"));
    let cand_dir = root.join("cand");
    copy_tree(&candidate(cand), &cand_dir);
    (cdir.join("contract.json"), cand_dir)
}

/// Plain recursive copy (no manifest), for building scratch trees.
pub fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type().unwrap().is_dir() {
            if e.file_name() != "target" {
                copy_tree(&src, &dst);
            }
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}
