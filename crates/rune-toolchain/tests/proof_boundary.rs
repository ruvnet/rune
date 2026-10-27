//! Regression checks for the previously reproduced promotion bypasses.
mod common;
use common::*;
use std::{fs, path::Path};

fn setup() -> (tempfile::TempDir, String) {
    let d = fixture("hello");
    ok(&rune(d.path(), &["build", "--freeze"]));
    let tree = rune_core::tree_digest(d.path()).unwrap();
    (d, tree)
}

fn promote(
    ws: &Path,
    report: &Path,
    target: &str,
    signed: Option<(&Path, &Path)>,
) -> std::process::Output {
    let cp = contract_path();
    let mut args = vec![
        "promote",
        "--report",
        report.to_str().unwrap(),
        "--contract",
        cp.to_str().unwrap(),
        "--target",
        target,
    ];
    if let Some((r, k)) = signed {
        args.extend([
            "--receipt",
            r.to_str().unwrap(),
            "--trusted-key",
            k.to_str().unwrap(),
        ]);
    }
    rune(ws, &args)
}

#[test]
fn fabricated_unsigned_accept_is_rejected() {
    let (ws, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let rp = scratch.path().join("fabricated.json");
    write_report(&rp, &accept_report(&tree));
    assert!(fails(&promote(ws.path(), &rp, "x86_64", None))
        .contains("signed evaluator receipt required"));
    assert_eq!(history(ws.path()).len(), 1);
}

#[test]
fn authenticated_report_cannot_be_relabelled_for_arm() {
    let (ws, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let rp = scratch.path().join("report.json");
    let mut report = accept_report(&tree);
    report.baseline_digest = tree;
    write_report(&rp, &report);
    let (receipt, public) = sign_test_report(ws.path(), &rp);
    assert!(fails(&promote(
        ws.path(),
        &rp,
        "aarch64",
        Some((&receipt, &public))
    ))
    .contains("evaluated target"));
    assert_eq!(history(ws.path()).len(), 1);
}

#[test]
fn modified_report_and_wrong_trust_anchor_are_rejected() {
    let (ws, tree) = setup();
    let scratch = tempfile::tempdir().unwrap();
    let rp = scratch.path().join("report.json");
    let mut report = accept_report(&tree);
    report.baseline_digest = tree;
    write_report(&rp, &report);
    let original = fs::read(&rp).unwrap();
    let (receipt, public) = sign_test_report(ws.path(), &rp);
    report.speedup_mean = 1.6;
    write_report(&rp, &report);
    assert!(fails(&promote(
        ws.path(),
        &rp,
        "x86_64",
        Some((&receipt, &public))
    ))
    .contains("report digest mismatch"));
    fs::write(&rp, original).unwrap();
    let receipt_bytes = fs::read(&receipt).unwrap();
    let mut tampered: serde_json::Value = serde_json::from_slice(&receipt_bytes).unwrap();
    tampered["identity"]["compiler_id"] = "forged".into();
    fs::write(&receipt, serde_json::to_vec(&tampered).unwrap()).unwrap();
    assert!(fails(&promote(
        ws.path(),
        &rp,
        "x86_64",
        Some((&receipt, &public))
    ))
    .contains("signature mismatch"));
    fs::write(&receipt, receipt_bytes).unwrap();
    let planted = ws.path().join(".rune/planted.pub");
    fs::copy(&public, &planted).unwrap();
    assert!(fails(&promote(
        ws.path(),
        &rp,
        "x86_64",
        Some((&receipt, &planted))
    ))
    .contains("outside the candidate workspace"));
    let key = scratch.path().join("wrong.key");
    fs::write(&key, [9u8; 32]).unwrap();
    let wrong = rune_target::sign::load_signing_key(&key).unwrap();
    fs::write(&public, wrong.verifying_key().as_bytes()).unwrap();
    assert!(fails(&promote(
        ws.path(),
        &rp,
        "x86_64",
        Some((&receipt, &public))
    ))
    .contains("not the trusted key"));
    assert_eq!(history(ws.path()).len(), 1);
}

#[test]
fn trusted_receipts_still_require_matching_execution_context() {
    for field in [
        "compiler_id",
        "policy_digest",
        "baseline_digest",
        "evaluator_version",
    ] {
        let (ws, tree) = setup();
        let scratch = tempfile::tempdir().unwrap();
        let rp = scratch.path().join("report.json");
        let mut report = accept_report(&tree);
        report.baseline_digest = tree;
        write_report(&rp, &report);
        let (receipt, public) = sign_test_report(ws.path(), &rp);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        value["identity"][field] = "mismatch".into();
        let identity = serde_json::from_value(value["identity"].clone()).unwrap();
        let signed = rune_target::receipt::EvaluationReceipt::sign(
            identity,
            &scratch.path().join("test-only.key"),
        )
        .unwrap();
        fs::write(&receipt, serde_json::to_vec_pretty(&signed).unwrap()).unwrap();
        assert!(
            fails(&promote(
                ws.path(),
                &rp,
                "x86_64",
                Some((&receipt, &public))
            ))
            .contains("does not match promotion"),
            "{field}"
        );
        assert_eq!(history(ws.path()).len(), 1);
    }
}
