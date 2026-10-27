mod common;
use common::*;
use std::fs;

/// A workspace with an approved plan and the digest of its current tree.
fn setup() -> (tempfile::TempDir, String) {
    let d = fixture("hello");
    ok(&rune(d.path(), &["build", "--freeze"]));
    let tree = rune_core::tree_digest(d.path()).unwrap();
    (d, tree)
}

fn promote(d: &std::path::Path, report: &std::path::Path) -> std::process::Output {
    rune(
        d,
        &[
            "promote",
            "--plan",
            "current",
            "--report",
            report.to_str().unwrap(),
            "--contract",
            contract_path().to_str().unwrap(),
            "--target",
            "x86_64",
        ],
    )
}

#[test]
fn refuses_a_reject_report() {
    let (d, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let before = history(d.path());
    let rp = scratch.path().join("report.json");
    write_report(&rp, &reject_report(&tree));
    let e = fails(&promote(d.path(), &rp));
    assert!(e.contains("not Accept"), "{e}");
    assert_eq!(history(d.path()), before, "no plan may be written");
}

#[test]
fn refuses_a_report_for_a_different_artifact() {
    let (d, _) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let before = history(d.path());
    let rp = scratch.path().join("report.json");
    write_report(&rp, &accept_report(&"a".repeat(64)));
    let e = fails(&promote(d.path(), &rp));
    assert!(e.contains("evaluated candidate"), "{e}");
    assert_eq!(history(d.path()), before);
}

#[test]
fn refuses_when_the_workspace_changed_after_evaluation() {
    let (d, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let rp = scratch.path().join("report.json");
    write_report(&rp, &accept_report(&tree));
    fs::write(d.path().join("src/main.rs"), "fn main() { /* edited */ }\n").unwrap();
    let e = fails(&promote(d.path(), &rp));
    assert!(e.contains("evaluated candidate"), "{e}");
}

#[test]
fn refuses_a_report_for_another_contract() {
    let (d, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let mut r = accept_report(&tree);
    r.contract_digest = "c".repeat(64);
    let rp = scratch.path().join("report.json");
    write_report(&rp, &r);
    assert!(fails(&promote(d.path(), &rp)).contains("contract digest"));
}

#[test]
fn refuses_an_inconsistent_accept() {
    let (d, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let mut r = accept_report(&tree);
    r.speedup_ci95_low = 1.01; // does not clear min_improvement
    r.speedup_mean = 1.2;
    let rp = scratch.path().join("report.json");
    write_report(&rp, &r);
    assert!(fails(&promote(d.path(), &rp)).contains("inconsistent"));
}

#[test]
fn refuses_rvm_because_it_has_no_specification() {
    let (d, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let rp = scratch.path().join("report.json");
    write_report(&rp, &accept_report(&tree));
    let o = rune(
        d.path(),
        &[
            "promote",
            "--report",
            rp.to_str().unwrap(),
            "--contract",
            contract_path().to_str().unwrap(),
            "--target",
            "rvm",
        ],
    );
    assert!(fails(&o).contains("specification required"));
}

#[test]
fn accepted_report_promotes_binds_the_key_and_rolls_back() {
    let (d, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let first = history(d.path());
    let rp = scratch.path().join("report.json");
    let mut report = accept_report(&tree);
    report.baseline_digest = tree.clone();
    write_report(&rp, &report);
    let (receipt, public) = sign_test_report(d.path(), &rp);
    let s = ok(&rune(
        d.path(),
        &[
            "promote",
            "--report",
            rp.to_str().unwrap(),
            "--contract",
            contract_path().to_str().unwrap(),
            "--target",
            "x86_64",
            "--receipt",
            receipt.to_str().unwrap(),
            "--trusted-key",
            public.to_str().unwrap(),
        ],
    ));
    assert!(s.contains("approval key"), "{s}");
    let h = history(d.path());
    assert_eq!(h.len(), 2);
    let plan: serde_json::Value = serde_json::from_slice(
        &fs::read(d.path().join(format!(".rune/plans/{}.json", h[1]))).unwrap(),
    )
    .unwrap();
    assert_eq!(plan["parent"], first[0], "parent is the previous plan");
    assert_eq!(plan["source_digest"], tree);
    let rec: serde_json::Value = serde_json::from_slice(
        &fs::read(d.path().join(format!(".rune/promotions/{}.json", h[1]))).unwrap(),
    )
    .unwrap();
    for f in [
        "target_digest",
        "compiler_id",
        "policy_digest",
        "contract_digest",
        "evaluator_version",
    ] {
        assert!(
            rec["inputs"][f].as_str().is_some_and(|v| !v.is_empty()),
            "{f}"
        );
    }
    assert_eq!(rec["inputs"]["contract_digest"], contract().digest());
    let ex = ok(&rune(d.path(), &["explain"]));
    assert!(ex.contains("approval   key"), "{ex}");
    ok(&rune(d.path(), &["rollback"]));
    assert_eq!(history(d.path()), first);
}
