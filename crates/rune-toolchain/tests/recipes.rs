mod common;
use common::*;
use std::fs;

fn add(ws: &std::path::Path, cand: &str, target: &str) -> String {
    let o = rune(
        ws,
        &[
            "recipes",
            "add",
            "--descriptor",
            "unroll",
            "--baseline-hash",
            "b",
            "--candidate-hash",
            cand,
            "--target",
            target,
            "--shared",
            "--source-tag",
            "loop",
        ],
    );
    let s = ok(&o);
    s.split_whitespace().nth(1).unwrap().to_owned()
}

#[test]
fn add_search_show_label_ranking_as_not_approval() {
    let d = tempfile::tempdir().unwrap();
    let x86 = add(d.path(), "c1", "x86_64");
    let arm = add(d.path(), "c2", "aarch64");
    let s = ok(&rune(
        d.path(),
        &[
            "recipes",
            "search",
            "--target",
            "aarch64",
            "--source-tag",
            "loop",
        ],
    ));
    assert!(s.contains("ranking only \u{2014} not approval"), "{s}");
    let first = s.lines().find(|l| l.contains("score")).unwrap();
    assert!(first.contains(&arm), "same-target recipe ranks first: {s}");
    assert!(s.contains(&x86));
    let sh = ok(&rune(d.path(), &["recipes", "show", &arm[..10]]));
    assert!(sh.contains("\"candidate_hash\": \"c2\""), "{sh}");
    assert!(sh.contains("not an approval"));
    // A recipe is never approved by being added or found.
    let ex = fs::read_to_string(d.path().join(".rune/memory/approvals.jsonl")).unwrap_or_default();
    assert!(ex.is_empty());
}

#[test]
fn ingest_rejects_inconsistent_reports_and_accepts_consistent_ones() {
    let d = tempfile::tempdir().unwrap();
    let id = add(d.path(), "cand-x", "x86_64");
    let ingest = |report: &rune_core::contract::EvalReport, name: &str| {
        let p = d.path().join(name);
        write_report(&p, report);
        rune(
            d.path(),
            &[
                "recipes",
                "ingest",
                "--recipe",
                &id,
                "--report",
                p.to_str().unwrap(),
                "--contract",
                contract_path().to_str().unwrap(),
            ],
        )
    };
    let mut bad = accept_report("cand-x");
    bad.baseline_digest = "b".into();
    bad.samples = 0; // an Accept with no samples is inconsistent
    assert!(fails(&ingest(&bad, "bad.json")).contains("evidence rejected"));
    let mut wrong_contract = accept_report("cand-x");
    wrong_contract.baseline_digest = "b".into();
    wrong_contract.contract_digest = "z".into();
    fails(&ingest(&wrong_contract, "wc.json"));
    let mut good = accept_report("cand-x");
    good.baseline_digest = "b".into();
    ok(&ingest(&good, "good.json"));
    let ev = fs::read_to_string(d.path().join(".rune/memory/evidence.jsonl")).unwrap();
    assert_eq!(
        ev.lines().count(),
        1,
        "only the valid report entered the store"
    );
    // Ingesting never approves.
    assert!(!d.path().join(".rune/memory/approvals.jsonl").exists());
}

#[test]
fn snapshot_is_frozen_per_campaign_and_immutable() {
    let d = tempfile::tempdir().unwrap();
    add(d.path(), "c1", "x86_64");
    let s = ok(&rune(
        d.path(),
        &["recipes", "snapshot", "--campaign", "camp-1"],
    ));
    assert!(s.contains("1 recipes"), "{s}");
    let path = d.path().join(".rune/memory/snapshots/camp-1.json");
    let frozen = fs::read(&path).unwrap();
    add(d.path(), "c2", "x86_64");
    let e = fails(&rune(
        d.path(),
        &["recipes", "snapshot", "--campaign", "camp-1"],
    ));
    assert!(e.contains("immutable"), "{e}");
    assert_eq!(fs::read(&path).unwrap(), frozen);
    ok(&rune(
        d.path(),
        &["recipes", "snapshot", "--campaign", "camp-2"],
    ));
    let two: serde_json::Value = serde_json::from_slice(
        &fs::read(d.path().join(".rune/memory/snapshots/camp-2.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(two["recipes"].as_array().unwrap().len(), 2);
    fails(&rune(
        d.path(),
        &["recipes", "snapshot", "--campaign", "../evil"],
    ));
}
