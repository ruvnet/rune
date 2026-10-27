#![allow(dead_code)]
//! Shared helpers for the CLI integration tests.

use rune_core::contract::{EvalContract, EvalReport, Reject, Verdict};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

pub fn contract_path() -> PathBuf {
    fixtures().join("kernels/sqdist-contract.json")
}

pub fn contract() -> EvalContract {
    serde_json::from_slice(&fs::read(contract_path()).unwrap()).unwrap()
}

pub fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            if e.file_name() != "target" {
                copy(&e.path(), &dest);
            }
        } else {
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

/// A scratch copy of `tests/fixtures/<name>` (for example `hello`).
pub fn fixture(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy(&fixtures().join(name), dir.path());
    dir
}

pub fn rune(ws: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rune"))
        .arg("-C")
        .arg(ws)
        .args(args)
        .env_remove("OPENROUTER_API_KEY")
        .env("https_proxy", "http://127.0.0.1:9")
        .env("http_proxy", "http://127.0.0.1:9")
        .output()
        .unwrap()
}

pub fn ok(o: &Output) -> String {
    assert!(o.status.success(), "{}\n{}", out(o), err(o));
    out(o)
}

pub fn fails(o: &Output) -> String {
    assert_eq!(o.status.code(), Some(1), "{}\n{}", out(o), err(o));
    err(o)
}

pub fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

pub fn history(ws: &Path) -> Vec<String> {
    fs::read_to_string(ws.join(".rune/history"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn accept_report(candidate: &str) -> EvalReport {
    EvalReport {
        contract_digest: contract().digest(),
        baseline_digest: "b".repeat(64),
        candidate_digest: candidate.into(),
        samples: 40,
        speedup_mean: 1.5,
        speedup_ci95_low: 1.3,
        speedup_ci95_high: 1.7,
        verdict: Verdict::Accept,
        isolation: vec!["cow-copy".into()],
    }
}

pub fn reject_report(candidate: &str) -> EvalReport {
    EvalReport {
        samples: 0,
        speedup_mean: 0.0,
        speedup_ci95_low: 0.0,
        speedup_ci95_high: 0.0,
        verdict: Verdict::Reject(Reject::HoldoutFailed("tail dropped".into())),
        ..accept_report(candidate)
    }
}

pub fn write_report(path: &Path, r: &EvalReport) {
    fs::write(path, serde_json::to_vec_pretty(r).unwrap()).unwrap();
}

/// Test-only trusted signer. Fixed seed is public fixture material, never a deployment key.
pub fn sign_test_report(ws: &Path, report: &Path) -> (PathBuf, PathBuf) {
    let dir = report.parent().unwrap();
    let key = dir.join("test-only.key");
    let public = dir.join("test-only.pub");
    fs::write(&key, [7u8; 32]).unwrap();
    let signing = rune_target::sign::load_signing_key(&key).unwrap();
    fs::write(&public, signing.verifying_key().as_bytes()).unwrap();
    let h = history(ws);
    let plan: rune_core::Plan = serde_json::from_slice(
        &fs::read(ws.join(format!(".rune/plans/{}.json", h.last().unwrap()))).unwrap(),
    )
    .unwrap();
    let compiler = rune_cargo::toolchain_id().unwrap();
    let report_bytes = fs::read(report).unwrap();
    let r: EvalReport = serde_json::from_slice(&report_bytes).unwrap();
    let identity = rune_target::receipt::EvaluationIdentity {
        schema: rune_target::receipt::SCHEMA.into(),
        report_sha256: rune_core::hex_sha256(&report_bytes),
        baseline_digest: r.baseline_digest,
        compiler_id: rune_core::hex_sha256(compiler.as_bytes()),
        target_digest: rune_target::descriptors::x86_64_linux_gnu(&[]).digest(),
        policy_digest: rune_core::hex_sha256(&serde_json::to_vec(&plan.policy).unwrap()),
        evaluator_version: "unversioned".into(),
        degraded: false,
    };
    let receipt = rune_target::receipt::EvaluationReceipt::sign(identity, &key).unwrap();
    let path = dir.join("receipt.json");
    fs::write(&path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    (path, public)
}

/// Every file under `dir` as (relative path, size), sorted.
pub fn listing(dir: &Path) -> Vec<(String, u64)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if e.file_type().unwrap().is_dir() {
                    walk(base, &p, out);
                } else {
                    let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                    out.push((rel, e.metadata().unwrap().len()));
                }
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v);
    v.sort();
    v
}

pub fn tool(name: &str) -> bool {
    Command::new(name).arg("--version").output().is_ok()
}

pub fn rust_target_installed(triple: &str) -> bool {
    Command::new("rustc")
        .args(["--print", "target-libdir", "--target", triple])
        .output()
        .ok()
        .and_then(|o| fs::read_dir(String::from_utf8_lossy(&o.stdout).trim()).ok())
        .is_some_and(|mut d| d.next().is_some())
}
