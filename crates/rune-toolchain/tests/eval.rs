//! Real evaluator runs through the CLI. They share one process-wide lock and
//! live in their own test binary so no other test's cargo builds skew the
//! benchmark.
mod common;
use common::*;
use rune_core::contract::{EvalReport, Reject, Verdict};
use std::fs;
use std::sync::{Mutex, MutexGuard};

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

fn eval(ws: &std::path::Path, cand: &str, out: &std::path::Path) -> std::process::Output {
    let k = fixtures().join("kernels");
    rune(
        ws,
        &[
            "eval",
            "--contract",
            contract_path().to_str().unwrap(),
            "--baseline",
            k.join("sqdist").to_str().unwrap(),
            "--candidate",
            k.join("sqdist-candidates").join(cand).to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    )
}

fn report(path: &std::path::Path) -> EvalReport {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn dropped_tail_is_rejected_by_the_hidden_holdout_and_not_recorded_as_evidence() {
    let _g = serial();
    let ws = fixture("hello");
    let out = tempfile::tempdir().unwrap();
    let rp = out.path().join("report.json");
    let o = eval(ws.path(), "dropped-tail", &rp);
    assert!(fails(&o).contains("REJECT"), "{}", err(&o));
    assert!(matches!(
        report(&rp).verdict,
        Verdict::Reject(Reject::HoldoutFailed(_))
    ));
    assert!(
        !ws.path()
            .join(".rune/eval/evidence")
            .read_dir()
            .unwrap()
            .any(|_| true),
        "a rejection is never validated evidence"
    );
    assert!(ws.path().join(".rune/eval/latest").exists());
}

#[test]
fn permission_expanding_candidates_are_rejected_before_running() {
    let _g = serial();
    let ws = fixture("hello");
    let out = tempfile::tempdir().unwrap();
    for (cand, want) in [("new-dep", "NewDependency"), ("unsafe-simd", "NewUnsafe")] {
        let rp = out.path().join(format!("{cand}.json"));
        let o = eval(ws.path(), cand, &rp);
        assert!(fails(&o).contains(want), "{cand}: {}", err(&o));
        assert_eq!(report(&rp).samples, 0);
    }
}

#[test]
fn good_candidate_is_accepted_recorded_and_explained() {
    let _g = serial();
    let ws = fixture("hello");
    ok(&rune(ws.path(), &["build", "--freeze"]));
    let out = tempfile::tempdir().unwrap();
    let rp = out.path().join("report.json");
    let t = std::time::Instant::now();
    let o = eval(ws.path(), "good", &rp);
    eprintln!(
        "good candidate end-to-end: {:.1}s",
        t.elapsed().as_secs_f64()
    );
    assert!(o.status.success(), "{}", err(&o));
    let r = report(&rp);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.speedup_ci95_low > 1.1, "{r:?}");
    let ev: Vec<_> = fs::read_dir(ws.path().join(".rune/eval/evidence"))
        .unwrap()
        .collect();
    assert_eq!(
        ev.len(),
        1,
        "accepted report recorded as validated evidence"
    );
    let ex = ok(&rune(ws.path(), &["explain"]));
    assert!(
        ex.contains("eval report:") && ex.contains("verdict accept"),
        "{ex}"
    );
    // The evidence object is owned by the live plan, so cleanup keeps it.
    assert!(ok(&rune(ws.path(), &["cleanup"])).contains("0 unreachable"));
}
