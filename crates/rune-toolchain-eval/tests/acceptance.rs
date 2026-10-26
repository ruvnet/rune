//! ADR 005 acceptance, as real evaluator runs against real fixture crates.
mod common;
use common::*;
use rune_core::contract::{EvalContract, Reject, Verdict};
use rune_eval::{EvalRequest, Evaluator, RuneEvaluator};

fn step_ok(r: &rune_eval::FullReport, name: &str) -> Option<bool> {
    r.evidence
        .steps
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.ok)
}

#[test]
fn dropped_tail_passes_visible_but_holdout_rejects() {
    let _g = serial();
    let r = eval_fixture("dropped-tail");
    assert!(
        matches!(rejection(&r), Reject::HoldoutFailed(_)),
        "{:?}",
        r.report.verdict
    );
    assert_eq!(
        step_ok(&r, "candidate:visible"),
        Some(true),
        "must pass visible tests"
    );
    assert_eq!(step_ok(&r, "candidate:holdout"), Some(false));
    assert_eq!(r.report.samples, 0, "no benchmark for a failed candidate");
}

#[test]
fn permission_expanding_faster_candidates_reject_before_running() {
    let _g = serial();
    for (name, want) in [
        ("unsafe-simd", "new_unsafe"),
        ("net-capability", "new_capability"),
        ("new-dep", "new_dependency"),
    ] {
        let r = eval_fixture(name);
        let reason = match rejection(&r) {
            Reject::NewUnsafe(_) => "new_unsafe",
            Reject::NewCapability(_) => "new_capability",
            Reject::NewDependency(_) => "new_dependency",
            other => panic!("{name}: unexpected {other:?}"),
        };
        assert_eq!(reason, want, "{name}");
        assert!(
            step_ok(&r, "candidate:build").is_none(),
            "{name}: candidate code must not run when a static gate fails"
        );
        let gates = r.evidence.gates.as_ref().expect("gates recorded");
        assert!(gates.results.iter().any(|g| !g.passed));
    }
}

#[test]
fn slow_and_marginal_candidates_lack_improvement() {
    let _g = serial();
    let slow = eval_fixture("slow");
    assert!(matches!(
        rejection(&slow),
        Reject::InsufficientImprovement(_)
    ));
    assert!(
        slow.report.speedup_ci95_high < 1.0,
        "slow is actually slower: {:?}",
        slow.report
    );
    let marginal = eval_fixture("noisy-claim");
    assert!(matches!(
        rejection(&marginal),
        Reject::InsufficientImprovement(_)
    ));
    assert!(marginal.report.speedup_ci95_low <= 1.10);
    eprintln!(
        "marginal: mean {:.4} CI [{:.4}, {:.4}] n={}",
        marginal.report.speedup_mean,
        marginal.report.speedup_ci95_low,
        marginal.report.speedup_ci95_high,
        marginal.report.samples
    );
}

#[test]
fn good_candidate_accepted_only_if_ci_clears_ten_percent() {
    let _g = serial();
    let r = eval_fixture("good");
    let rep = &r.report;
    eprintln!(
        "GOOD: speedup(geo-mean) {:.4} CI95 [{:.4}, {:.4}] n={} load {:?} pin {:?} isolation {:?}",
        rep.speedup_mean,
        rep.speedup_ci95_low,
        rep.speedup_ci95_high,
        rep.samples,
        r.evidence.load_before,
        r.evidence.pinning,
        rep.isolation
    );
    // The verdict must be exactly the preregistered rule applied to measured numbers.
    let clears = rep.speedup_ci95_low > 1.10 && rep.samples >= 30;
    assert_eq!(matches!(rep.verdict, Verdict::Accept), clears, "{rep:?}");
    assert!(clears, "good did not clear 10% on this machine: {rep:?}");
    // Guards measured and passed; bootstrap agrees with the t-interval.
    let m = r
        .evidence
        .metrics
        .iter()
        .find(|m| m.role == "improvement")
        .unwrap();
    assert!(
        (m.bootstrap_ci95_low / m.ci95_low - 1.0).abs() < 0.02,
        "{m:?}"
    );
    assert_eq!(
        r.evidence
            .metrics
            .iter()
            .filter(|m| m.role == "guard" && m.passed)
            .count(),
        2
    );
    assert_eq!(m.baseline_ns.len(), m.candidate_ns.len());
    assert!(m.baseline_ns.len() >= 30 && m.warmup_pairs >= 1);
}

#[test]
fn isolation_layers_are_reported_truthfully() {
    let _g = serial();
    let r = eval_fixture("dropped-tail");
    let caps = rune_eval::runner::Capabilities::probe();
    let has = |l: &str| r.report.isolation.iter().any(|x| x.starts_with(l));
    assert!(has("timeout") && has("env-scrub"));
    assert_eq!(has("netns"), caps.netns);
    assert_eq!(has("rlimit-as"), caps.prlimit);
    assert!(has("cow-reflink") || has("plain-copy"));
    assert_eq!(r.evidence.isolation_unavailable, caps.notes);
    assert!(r.evidence.evaluator.contains("not MetaHarness"));
}

#[test]
fn tampered_contract_digest_is_detected() {
    let _g = serial();
    let work = tempfile::tempdir().unwrap();
    let (cpath, cand) = scratch_copy(work.path(), "good");
    let original: EvalContract = serde_json::from_slice(&std::fs::read(&cpath).unwrap()).unwrap();
    let approved = original.digest();
    // A proposer-side edit that lowers the bar.
    let mut c: serde_json::Value = serde_json::from_slice(&std::fs::read(&cpath).unwrap()).unwrap();
    c["min_improvement"] = 0.0.into();
    std::fs::write(&cpath, serde_json::to_vec_pretty(&c).unwrap()).unwrap();
    let mut req = EvalRequest::new(&cpath, &baseline(), &cand, &work.path().join("w"));
    req.expect_contract_digest = Some(approved.clone());
    let r = RuneEvaluator.evaluate(&req).unwrap();
    assert!(matches!(rejection(&r), Reject::MissingEvidence(m) if m.contains("digest mismatch")));
    assert_ne!(r.report.contract_digest, approved);
}

#[test]
fn evaluator_inputs_inside_candidate_tree_are_refused() {
    let _g = serial();
    let work = tempfile::tempdir().unwrap();
    let (cpath, cand) = scratch_copy(work.path(), "good");
    // contract planted inside the candidate
    let planted = cand.join("contract.json");
    std::fs::copy(&cpath, &planted).unwrap();
    let r = eval(&planted, &cand, &work.path().join("w1"));
    assert!(
        matches!(rejection(&r), Reject::MissingEvidence(m) if m.contains("inside the candidate"))
    );
    // holdout inside the candidate
    let mut c: serde_json::Value = serde_json::from_slice(&std::fs::read(&cpath).unwrap()).unwrap();
    c["holdout_dir"] = "../../cand/holdout".into();
    copy_tree(&work.path().join("frozen/holdout"), &cand.join("holdout"));
    std::fs::write(&cpath, serde_json::to_vec(&c).unwrap()).unwrap();
    let r = eval(&cpath, &cand, &work.path().join("w2"));
    assert!(matches!(rejection(&r), Reject::MissingEvidence(m) if m.contains("holdout")));
}

#[test]
fn candidate_cannot_rewrite_its_own_tests() {
    let _g = serial();
    let work = tempfile::tempdir().unwrap();
    let (cpath, cand) = scratch_copy(work.path(), "dropped-tail");
    std::fs::write(cand.join("tests/visible.rs"), "#[test] fn vacuous() {}\n").unwrap();
    let r = eval(&cpath, &cand, &work.path().join("w"));
    assert!(
        matches!(rejection(&r), Reject::VisibleTestsFailed(m) if m.contains("protected harness"))
    );
    assert!(step_ok(&r, "candidate:build").is_none());
}

#[test]
fn cargo_config_in_candidate_is_a_capability_expansion() {
    let _g = serial();
    let work = tempfile::tempdir().unwrap();
    let (cpath, cand) = scratch_copy(work.path(), "good");
    std::fs::create_dir_all(cand.join(".cargo")).unwrap();
    std::fs::write(
        cand.join(".cargo/config.toml"),
        "[build]\nrustc-wrapper = \"/tmp/x\"\n",
    )
    .unwrap();
    let r = eval(&cpath, &cand, &work.path().join("w"));
    assert!(matches!(rejection(&r), Reject::NewCapability(_)));
}
