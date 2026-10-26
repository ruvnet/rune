#![allow(dead_code)]
use rune_core::{Plan, Policy, Receipt, Store};
use std::path::Path;

pub fn plan(src: &str, parent: Option<String>) -> Plan {
    let pol = Policy {
        max_usd: 2.0,
        max_tokens: 1000,
        max_seconds: 60,
        max_memory_mb: 64,
    };
    Plan::new(
        src.into(),
        "l".into(),
        "rustc 1".into(),
        "x86_64".into(),
        pol,
        parent,
    )
}

/// Approve a plan and record a receipt naming `artifact_digest`.
pub fn approve(ws: &Path, src: &str, parent: Option<String>, artifact_digest: &str) -> String {
    let s = Store::open(ws);
    let id = s.approve(&plan(src, parent)).unwrap();
    s.record(&Receipt {
        plan_id: id.clone(),
        artifact: "bin".into(),
        artifact_digest: artifact_digest.into(),
        offline: true,
        model_calls: 0,
    })
    .unwrap();
    id
}

pub fn workspace() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    std::fs::write(d.path().join("src/main.rs"), b"fn main() {}\n").unwrap();
    d
}

pub const DAY: u64 = 24 * 60 * 60;
