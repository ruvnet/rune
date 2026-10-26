use rune_core::contract::{EvalContract, EvalReport, Reject, Verdict};
use rune_memory::*;
use std::sync::Arc;

fn contract() -> EvalContract {
    EvalContract {
        name: "k".into(),
        kernel_dir: "k".into(),
        test_cmd: vec![],
        holdout_dir: "h".into(),
        holdout_dest: "d".into(),
        holdout_cmd: vec![],
        bench_cmd: vec![],
        guard_benches: vec![],
        samples: 30,
        warmup: 3,
        min_improvement: 0.10,
        forbid_unsafe: true,
        forbid_new_deps: true,
        forbid_new_capabilities: true,
        max_seconds: 60,
        max_memory_mb: 512,
    }
}

fn recipe(name: &str, tenant: &str, vis: Visibility) -> Recipe {
    Recipe {
        id: String::new(),
        parent: None,
        proposer: "agent".into(),
        descriptor: name.into(),
        fingerprint: Fingerprint {
            source_tags: vec![name.into()],
            ..Default::default()
        },
        template: EditTemplate::default(),
        baseline_hash: "base".into(),
        candidate_hash: format!("cand-{name}"),
        evaluation_version: "ev1".into(),
        evidence: vec![],
        failures: vec![],
        retention: Retention::Standard,
        status: Status::Proposed,
        tenant: tenant.into(),
        visibility: vis,
    }
    .sealed()
}

fn evidence(r: &Recipe, verdict: Verdict, lo: f64, mean: f64, hi: f64, samples: u32) -> Evidence {
    Evidence {
        recipe_id: r.id.clone(),
        report: EvalReport {
            contract_digest: contract().digest(),
            baseline_digest: r.baseline_hash.clone(),
            candidate_digest: r.candidate_hash.clone(),
            samples,
            speedup_mean: mean,
            speedup_ci95_low: lo,
            speedup_ci95_high: hi,
            verdict,
            isolation: vec!["cow-copy".into()],
        },
        target_digest: "tx86".into(),
        compiler_id: "cc1".into(),
        policy_digest: "pol".into(),
        evaluator_version: "ev1".into(),
        sample_path: "samples/a.jsonl".into(),
        cost_micro_usd: 1200,
        scope: FailureScope::default(),
    }
}

fn accept(r: &Recipe) -> Evidence {
    evidence(r, Verdict::Accept, 1.15, 1.2, 1.25, 30)
}

fn inputs() -> KeyInputs {
    KeyInputs {
        target_digest: "tx86".into(),
        compiler_id: "cc1".into(),
        policy_digest: "pol".into(),
        contract_digest: contract().digest(),
        evaluator_version: "ev1".into(),
    }
}

#[test]
fn snapshot_is_immutable_after_later_writes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let a = recipe("a", "t", Visibility::Shared);
    store.add_recipe(&a).unwrap();
    let snap = store.freeze("t").unwrap();
    let (id, len) = (snap.id().to_string(), snap.len());
    store
        .add_recipe(&recipe("b", "t", Visibility::Shared))
        .unwrap();
    store.ingest(&accept(&a), &contract()).unwrap();
    assert_eq!((snap.id(), snap.len()), (id.as_str(), len));
    assert!(snap.recipes()[0].evidence.is_empty());
    let later = store.freeze("t").unwrap();
    assert_ne!(later.id(), snap.id());
    assert_eq!(later.len(), 2);
}

#[test]
fn ingest_validates_before_appending() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let r = recipe("a", "t", Visibility::Shared);
    store.add_recipe(&r).unwrap();
    let c = contract();
    let mut wrong_contract = accept(&r);
    wrong_contract.report.contract_digest = "other".into();
    let inconsistent_ci = evidence(&r, Verdict::Accept, 1.3, 1.2, 1.25, 30);
    let below_bar = evidence(&r, Verdict::Accept, 1.05, 1.08, 1.1, 30);
    let few_samples = evidence(&r, Verdict::Accept, 1.15, 1.2, 1.25, 5);
    let nan = evidence(&r, Verdict::Accept, f64::NAN, 1.2, 1.25, 30);
    let liar = evidence(
        &r,
        Verdict::Reject(Reject::InsufficientImprovement("x".into())),
        1.15,
        1.2,
        1.25,
        30,
    );
    let mut bad_path = accept(&r);
    bad_path.sample_path = "../etc/passwd".into();
    let mut other_cand = accept(&r);
    other_cand.report.candidate_digest = "different".into();
    let mut unknown = accept(&r);
    unknown.recipe_id = "nope".into();
    for ev in [
        wrong_contract,
        inconsistent_ci,
        below_bar,
        few_samples,
        nan,
        liar,
        bad_path,
        other_cand,
        unknown,
    ] {
        assert!(store.ingest(&ev, &c).is_err(), "{ev:?}");
    }
    assert!(store.freeze("t").unwrap().recipes()[0].evidence.is_empty());
    store.ingest(&accept(&r), &c).unwrap();
    assert_eq!(store.freeze("t").unwrap().recipes()[0].evidence.len(), 1);
}

#[test]
fn queue_is_invisible_until_validated_and_rejections_become_scoped_failures() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let r = recipe("a", "t", Visibility::Shared);
    store.add_recipe(&r).unwrap();
    let mut rej = evidence(
        &r,
        Verdict::Reject(Reject::HoldoutFailed("edge".into())),
        0.0,
        0.0,
        0.0,
        0,
    );
    rej.scope = FailureScope {
        target_triple: Some("aarch64".into()),
        ..Default::default()
    };
    let mut bogus = accept(&r);
    bogus.report.contract_digest = "x".into();
    store.enqueue(&rej).unwrap();
    store.enqueue(&bogus).unwrap();
    assert!(store.freeze("t").unwrap().recipes()[0].failures.is_empty());
    let rep = store.drain_queue(&contract()).unwrap();
    assert_eq!((rep.ingested, rep.rejected.len()), (1, 1));
    let snap = store.freeze("t").unwrap();
    let got = &snap.recipes()[0];
    assert_eq!(got.status, Status::Rejected);
    assert_eq!(
        got.failures[0].scope.target_triple.as_deref(),
        Some("aarch64")
    );
    let hits = HashedFeatureRanker.rank(&got.fingerprint, &snap, 5);
    assert_eq!(hits.len(), 1); // rejected recipes stay searchable
    assert!(store.drain_queue(&contract()).unwrap().ingested == 0);
}

#[test]
fn approval_requires_matching_accepted_evidence_and_is_exact() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let r = recipe("a", "t", Visibility::Shared);
    store.add_recipe(&r).unwrap();
    assert!(store.record_approval(&r.id, &inputs()).is_err()); // no evidence yet
    store.ingest(&accept(&r), &contract()).unwrap();
    let mut arm = inputs();
    arm.target_digest = "taarch64".into();
    assert!(store.record_approval(&r.id, &arm).is_err()); // evidence was x86 only
    store.record_approval(&r.id, &inputs()).unwrap();
    let book = store.approvals().unwrap();
    assert!(is_approved(&r, &inputs(), &book));
    assert!(!is_approved(&r, &arm, &book));
    let mut approved = recipe("z", "t", Visibility::Shared);
    approved.status = Status::Approved;
    assert!(store.add_recipe(&approved).is_err());
}

#[test]
fn private_records_stay_in_their_tenant() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mine = recipe("mine", "acme", Visibility::Private);
    let theirs = recipe("theirs", "other", Visibility::Private);
    let public = recipe("public", "other", Visibility::Shared);
    for r in [&mine, &theirs, &public] {
        store.add_recipe(r).unwrap();
    }
    let snap = store.freeze("acme").unwrap();
    let ids: Vec<&str> = snap.recipes().iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&mine.id.as_str()) && ids.contains(&public.id.as_str()));
    assert!(!ids.contains(&theirs.id.as_str()));
    let hits = HashedFeatureRanker.rank(&theirs.fingerprint, &snap, 10);
    assert!(hits.iter().all(|h| h.id != theirs.id));
    assert!(matches!(
        export_shareable(snap.recipes()),
        Err(Error::PrivateExport(_))
    ));
    assert!(export_shareable(&[public]).unwrap().lines().count() == 1);
}

#[test]
fn concurrent_appends_never_interleave_and_paths_are_safe() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let s = store.clone();
            std::thread::spawn(move || {
                for i in 0..40 {
                    s.add_recipe(&recipe(&format!("t{t}-{i}"), "t", Visibility::Shared))
                        .unwrap();
                }
            })
        })
        .collect();
    handles.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(store.freeze("t").unwrap().len(), 320);

    let link = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), link.path().join(".rune")).unwrap();
    assert!(Store::open(link.path()).is_err());
}

#[test]
fn torn_trailing_line_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .add_recipe(&recipe("a", "t", Visibility::Shared))
        .unwrap();
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join(".rune/memory/recipes.jsonl"))
        .unwrap();
    f.write_all(b"{\"id\":\"trunc").unwrap();
    assert_eq!(store.freeze("t").unwrap().len(), 1);
}
