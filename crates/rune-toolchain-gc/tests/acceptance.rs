mod common;
use common::*;
use rune_core::hex_sha256;
use rune_gc::manifest::{Entry, Kind, Manifest, QEntry, Retention};
use rune_gc::{
    leases, plan_cleanup, plan_cleanup_with, purge, put_object, quarantine,
    register_candidate_workspace, restore, Error, Liveness, Mode, ProcLiveness,
};
use std::fs;
use std::path::Path;

const PLAN_X: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

fn orphan(ws: &Path, content: &[u8]) -> String {
    put_object(ws, Kind::Artifact, PLAN_X, None, content).unwrap()
}

fn entry(kind: Kind, seq: u64) -> Entry {
    Entry {
        kind,
        plan_id: PLAN_X.into(),
        seq,
        size: 1,
        retention: Retention::Transient,
        created_unix: 0,
        path: None,
    }
}

fn apply_all(ws: &Path, now: u64) -> rune_gc::Report {
    let plan = plan_cleanup(ws).unwrap();
    quarantine(&plan, now, Mode::Apply).unwrap()
}

#[test]
fn cleanup_preserves_active_source_user_files_and_rollback_artifacts() {
    let d = workspace();
    let ws = d.path();
    let art_a = put_object(ws, Kind::Artifact, PLAN_X, None, b"artifact-A").unwrap();
    let art_b = put_object(ws, Kind::Artifact, PLAN_X, None, b"artifact-B").unwrap();
    let a = approve(ws, "a", None, &art_a);
    let b = approve(ws, "b", Some(a.clone()), &art_b);
    // active build artifact protected only by a live lease
    let active = orphan(ws, b"active-build");
    leases::acquire(ws, "job1", std::slice::from_ref(&active)).unwrap();
    let junk = orphan(ws, b"junk");
    let ev_dead = put_object(
        ws,
        Kind::Evidence,
        PLAN_X,
        None,
        b"evidence of unknown plan",
    )
    .unwrap();
    let ev_live = put_object(ws, Kind::Evidence, &b, None, b"evidence of plan b").unwrap();
    let pinned = put_object(
        ws,
        Kind::Evidence,
        PLAN_X,
        Some(Retention::Pinned),
        b"pinned",
    )
    .unwrap();
    // user files, including inside .rune but outside the manifest
    fs::write(ws.join(".rune/notes.txt"), b"mine").unwrap();
    fs::write(ws.join(".rune/objects/hand-placed.bin"), b"also mine").unwrap();
    fs::write(ws.join("README.md"), b"readme").unwrap();
    // roll back: plan b leaves history, but a and b stay roots (retained evidence)
    rune_core::Store::open(ws).rollback().unwrap();

    let plan = plan_cleanup(ws).unwrap();
    let keys: Vec<_> = plan.candidates.iter().map(|c| c.key.clone()).collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&junk) && keys.contains(&ev_dead));
    // dry run (default) changes nothing
    let r = quarantine(&plan, 100, Mode::default()).unwrap();
    assert_eq!(r.mode, Mode::DryRun);
    assert!(rune_gc::object_path(ws, &junk).unwrap().exists());

    apply_all(ws, 100);
    for k in [&art_a, &art_b, &active, &ev_live, &pinned] {
        assert!(
            rune_gc::object_path(ws, k).unwrap().exists(),
            "{k} must survive"
        );
    }
    assert!(!rune_gc::object_path(ws, &junk).unwrap().exists());
    purge(ws, 100 + 30 * DAY, Mode::Apply).unwrap();
    for k in [&art_a, &art_b, &active, &ev_live, &pinned] {
        assert!(
            rune_gc::object_path(ws, k).unwrap().exists(),
            "{k} must survive purge"
        );
    }
    assert_eq!(fs::read(ws.join("src/main.rs")).unwrap(), b"fn main() {}\n");
    assert_eq!(fs::read(ws.join("README.md")).unwrap(), b"readme");
    assert_eq!(fs::read(ws.join(".rune/notes.txt")).unwrap(), b"mine");
    assert_eq!(
        fs::read(ws.join(".rune/objects/hand-placed.bin")).unwrap(),
        b"also mine"
    );
    assert!(ws.join(format!(".rune/plans/{a}.json")).exists());
    assert!(ws.join(format!(".rune/plans/{b}.json")).exists());
    assert!(ws.join(format!(".rune/receipts/{a}.json")).exists());
    assert!(ws.join(".rune/history").exists());
}

#[test]
fn traversal_ids_and_absolute_paths_cannot_escape() {
    let d = workspace();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("passwd");
    fs::write(&victim, b"root:x:0:0").unwrap();
    let ws = d.path();
    let good = orphan(ws, b"x");
    let rel = format!("../../../..{}", victim.display());
    let mut m = Manifest::load(ws).unwrap();
    for bad in [
        rel.as_str(),
        "../../etc/passwd",
        "/etc/passwd",
        "..",
        ".",
        "a/b",
    ] {
        m.entries.insert(bad.into(), entry(Kind::Artifact, 9));
        m.quarantine.push(QEntry {
            batch: 1,
            key: bad.into(),
            entry: entry(Kind::Artifact, 9),
            quarantined_at: 0,
        });
    }
    m.commit(ws).unwrap();
    let plan = plan_cleanup(ws).unwrap();
    assert_eq!(plan.candidates.len(), 1);
    assert_eq!(plan.candidates[0].key, good);
    assert_eq!(plan.rejected.len(), 6);
    // even a hostile hand-built plan cannot widen what is moved
    let mut hostile = plan.clone();
    for (k, _) in &plan.rejected {
        hostile.candidates.push(rune_gc::Candidate {
            key: k.clone(),
            kind: Kind::Artifact,
            plan_id: PLAN_X.into(),
            seq: 9,
            size: 1,
            reason: "x".into(),
        });
    }
    let r = quarantine(&hostile, 0, Mode::Apply).unwrap();
    assert_eq!(r.done, vec![good]);
    purge(ws, 100 * DAY, Mode::Apply).unwrap();
    assert_eq!(fs::read(&victim).unwrap(), b"root:x:0:0");
    assert!(Path::new("/etc/passwd").exists());
    assert!(rune_gc::object_path(ws, "../../etc/passwd").is_err());
    assert!(put_object(ws, Kind::Artifact, "../../etc/passwd", None, b"x").is_err());
    assert!(leases::acquire(ws, "../evil", &[]).is_err());
}

#[test]
fn symlink_inside_artifact_root_is_not_followed() {
    let d = workspace();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("secret");
    fs::write(&victim, b"keep me").unwrap();
    let ws = d.path();
    fs::create_dir_all(ws.join(".rune/objects")).unwrap();
    let key = hex_sha256(b"link");
    std::os::unix::fs::symlink(&victim, ws.join(".rune/objects").join(&key)).unwrap();
    let mut m = Manifest::load(ws).unwrap();
    m.entries.insert(key.clone(), entry(Kind::Artifact, 1));
    m.commit(ws).unwrap();
    let r = apply_all(ws, 0);
    assert!(r.done.is_empty());
    assert_eq!(r.skipped.len(), 1);
    purge(ws, 100 * DAY, Mode::Apply).unwrap();
    assert_eq!(fs::read(&victim).unwrap(), b"keep me");
}

#[test]
fn symlinked_parent_directories_are_refused() {
    let d = workspace();
    let outside = tempfile::tempdir().unwrap();
    let ws = d.path();
    let key = hex_sha256(b"victim");
    let victim = outside.path().join(&key);
    fs::write(&victim, b"keep me").unwrap();
    // .rune/objects -> outside
    fs::create_dir_all(ws.join(".rune")).unwrap();
    std::os::unix::fs::symlink(outside.path(), ws.join(".rune/objects")).unwrap();
    let mut m = Manifest::load(ws).unwrap();
    m.entries.insert(key.clone(), entry(Kind::Artifact, 1));
    m.commit(ws).unwrap();
    let r = apply_all(ws, 0);
    assert!(r.done.is_empty());
    assert!(put_object(ws, Kind::Artifact, PLAN_X, None, b"new").is_err());
    assert_eq!(fs::read(&victim).unwrap(), b"keep me");

    // .rune/quarantine/1 -> outside, with an old quarantine record
    fs::remove_file(ws.join(".rune/objects")).unwrap();
    fs::create_dir_all(ws.join(".rune/quarantine")).unwrap();
    std::os::unix::fs::symlink(outside.path(), ws.join(".rune/quarantine/1")).unwrap();
    let mut m = Manifest::load(ws).unwrap();
    m.entries.clear();
    m.quarantine.push(QEntry {
        batch: 1,
        key,
        entry: entry(Kind::Artifact, 1),
        quarantined_at: 0,
    });
    m.commit(ws).unwrap();
    fs::create_dir_all(ws.join(".rune/objects")).unwrap();
    let r = purge(ws, 100 * DAY, Mode::Apply).unwrap();
    assert!(r.done.is_empty(), "{r:?}");
    assert_eq!(fs::read(&victim).unwrap(), b"keep me");
}

#[test]
fn concurrent_ownership_change_is_detected() {
    let d = workspace();
    let ws = d.path();
    orphan(ws, b"seed");
    let mut a = Manifest::load(ws).unwrap();
    let mut b = Manifest::load(ws).unwrap();
    a.alloc_seq();
    b.alloc_seq();
    a.commit(ws).unwrap();
    match b.commit(ws) {
        Err(Error::Conflict { expected, found }) => assert_eq!(found, expected + 1),
        other => panic!("stale writer must fail, got {other:?}"),
    }
    // a fresh handle succeeds
    let mut c = Manifest::load(ws).unwrap();
    c.commit(ws).unwrap();
}

#[test]
fn quarantine_rolls_back_moves_when_a_second_writer_wins() {
    // Simulated by holding the manifest lock from a live pid.
    let d = workspace();
    let ws = d.path();
    let k = orphan(ws, b"orphan");
    let plan = plan_cleanup(ws).unwrap();
    let pid = std::process::id();
    let start = rune_gc::liveness::start_time(pid).unwrap();
    fs::write(ws.join(".rune/ownership.lock"), format!("{pid} {start}")).unwrap();
    assert!(matches!(
        quarantine(&plan, 0, Mode::Apply),
        Err(Error::Locked(_))
    ));
    assert!(
        rune_gc::object_path(ws, &k).unwrap().exists(),
        "move must be undone"
    );
}

fn dead_pid() -> u32 {
    let mut c = std::process::Command::new("true").spawn().unwrap();
    let pid = c.id();
    c.wait().unwrap();
    pid
}

#[test]
fn dead_lease_is_reclaimed_only_after_liveness_check_live_lease_protects() {
    let d = workspace();
    let ws = d.path();
    let live_obj = orphan(ws, b"live-protected");
    let dead_obj = orphan(ws, b"dead-protected");
    leases::acquire(ws, "live", std::slice::from_ref(&live_obj)).unwrap();
    leases::acquire_for(ws, "dead", dead_pid(), std::slice::from_ref(&dead_obj)).unwrap();
    let plan = plan_cleanup(ws).unwrap();
    let keys: Vec<_> = plan.candidates.iter().map(|c| &c.key).collect();
    assert_eq!(keys, vec![&dead_obj]);
    assert_eq!(plan.stale_leases, vec!["dead".to_string()]);
    // lease file is still present until reclaim runs, and reclaim spares the live one
    assert!(leases::read(ws, "dead").is_ok());
    let gone = leases::reclaim_stale(ws, &ProcLiveness).unwrap();
    assert_eq!(gone, vec!["dead".to_string()]);
    assert!(leases::read(ws, "dead").is_err());
    assert!(leases::read(ws, "live").is_ok());
    // heartbeat advances the sequence
    assert_eq!(leases::heartbeat(ws, "live").unwrap().heartbeat_seq, 1);
    // an always-alive stub keeps even the dead pid's lease honest to the trait
    struct Always;
    impl Liveness for Always {
        fn is_alive(&self, _: u32, _: u64) -> bool {
            true
        }
    }
    leases::acquire_for(ws, "dead2", dead_pid(), std::slice::from_ref(&dead_obj)).unwrap();
    assert!(plan_cleanup_with(ws, &Always)
        .unwrap()
        .candidates
        .is_empty());
}

#[test]
fn lease_with_reused_pid_start_time_is_stale() {
    let d = workspace();
    let ws = d.path();
    let k = orphan(ws, b"x");
    let mut l = leases::acquire(ws, "j", std::slice::from_ref(&k)).unwrap();
    l.pid_start += 1; // same pid, different process incarnation
    let p = ws.join(".rune/leases/j.json");
    fs::write(&p, serde_json::to_vec(&l).unwrap()).unwrap();
    assert_eq!(plan_cleanup(ws).unwrap().candidates.len(), 1);
}

#[test]
fn quarantine_restore_roundtrip_is_byte_identical() {
    let d = workspace();
    let ws = d.path();
    let content: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    let k = orphan(ws, &content);
    let before = Manifest::load(ws).unwrap().entries.clone();
    let r = apply_all(ws, 50);
    let batch = r.batch.unwrap();
    let path = rune_gc::object_path(ws, &k).unwrap();
    assert!(!path.exists());
    assert!(ws.join(format!(".rune/quarantine/{batch}/{k}")).exists());
    let rr = restore(ws, batch).unwrap();
    assert_eq!(rr.restored, vec![k.clone()]);
    assert_eq!(fs::read(&path).unwrap(), content);
    assert_eq!(Manifest::load(ws).unwrap().entries, before);
    assert!(restore(ws, batch).is_err(), "batch is consumed");
}

#[test]
fn purge_waits_seven_days_and_deletes_only_quarantined_unreachable() {
    let d = workspace();
    let ws = d.path();
    let q = orphan(ws, b"quarantined");
    let t0 = 1_000_000;
    let r = apply_all(ws, t0);
    let batch = r.batch.unwrap();
    let still_owned = orphan(ws, b"new orphan not yet quarantined");
    let held = ws.join(format!(".rune/quarantine/{batch}/{q}"));
    // before 7 days: nothing, and dry-run (default) never deletes
    let early = purge(ws, t0 + 7 * DAY - 1, Mode::Apply).unwrap();
    assert!(early.done.is_empty() && held.exists());
    let dry = purge(ws, t0 + 30 * DAY, Mode::default()).unwrap();
    assert_eq!(dry.done, vec![q.clone()]);
    assert!(held.exists());
    // clock going backwards deletes nothing
    assert!(purge(ws, 0, Mode::Apply).unwrap().done.is_empty());
    let done = purge(ws, t0 + 7 * DAY, Mode::Apply).unwrap();
    assert_eq!(done.done, vec![q]);
    assert!(!held.exists());
    assert!(rune_gc::object_path(ws, &still_owned).unwrap().exists());
    assert!(Manifest::load(ws).unwrap().quarantine.is_empty());
}

#[test]
fn object_that_regained_reachability_is_restored_not_deleted() {
    let d = workspace();
    let ws = d.path();
    let k = orphan(ws, b"comes back");
    apply_all(ws, 0);
    // a new plan's receipt now references it
    approve(ws, "late", None, &k);
    let r = purge(ws, 100 * DAY, Mode::Apply).unwrap();
    assert!(r.done.is_empty());
    assert_eq!(r.restored, vec![k.clone()]);
    assert_eq!(
        fs::read(rune_gc::object_path(ws, &k).unwrap()).unwrap(),
        b"comes back"
    );
    assert!(Manifest::load(ws).unwrap().entries.contains_key(&k));
    // same via a live lease
    let k2 = orphan(ws, b"lease brings back");
    apply_all(ws, 0);
    leases::acquire(ws, "j", std::slice::from_ref(&k2)).unwrap();
    assert_eq!(
        purge(ws, 100 * DAY, Mode::Apply).unwrap().restored,
        vec![k2]
    );
}

#[test]
fn candidate_workspace_is_collected_through_quarantine() {
    let d = workspace();
    let ws = d.path();
    let fork_root = tempfile::tempdir().unwrap();
    let fork = fork_root.path().join("fork-1");
    fs::create_dir_all(fork.join("src")).unwrap();
    fs::write(fork.join("src/lib.rs"), b"pub fn f() {}").unwrap();
    let key = register_candidate_workspace(ws, &fork, PLAN_X).unwrap();

    // while leased it is untouchable
    leases::acquire(ws, "eval", std::slice::from_ref(&key)).unwrap();
    assert!(plan_cleanup(ws).unwrap().candidates.is_empty());
    leases::release(ws, "eval").unwrap();

    let r = apply_all(ws, 10);
    assert_eq!(r.done, vec![key.clone()]);
    assert!(!fork.exists());
    let batch = r.batch.unwrap();
    restore(ws, batch).unwrap();
    assert_eq!(fs::read(fork.join("src/lib.rs")).unwrap(), b"pub fn f() {}");
    apply_all(ws, 10);
    let p = purge(ws, 10 + 7 * DAY, Mode::Apply).unwrap();
    assert_eq!(p.done, vec![key]);
    assert!(fork_root.path().read_dir().unwrap().next().is_none());
    assert!(fork_root.path().exists());
}

#[test]
fn candidate_registration_and_collection_are_contained() {
    let d = workspace();
    let ws = d.path();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("precious"), b"user data").unwrap();
    let link = outside.path().join("link");
    std::os::unix::fs::symlink(d.path(), &link).unwrap();
    // symlink, source tree, a parent of the workspace and a subdir of it are refused
    assert!(register_candidate_workspace(ws, &link, PLAN_X).is_err());
    assert!(register_candidate_workspace(ws, ws, PLAN_X).is_err());
    assert!(register_candidate_workspace(ws, ws.parent().unwrap(), PLAN_X).is_err());
    assert!(register_candidate_workspace(ws, &ws.join("src"), PLAN_X).is_err());
    assert!(register_candidate_workspace(ws, &outside.path().join("nope"), PLAN_X).is_err());
    // a forged manifest entry pointing at a user dir without our marker is refused
    let forged = outside.path().canonicalize().unwrap();
    let key = rune_gc::objects::candidate_key(&forged);
    let mut m = Manifest::load(ws).unwrap();
    let mut e = entry(Kind::CandidateWorkspace, 1);
    e.path = Some(forged.to_string_lossy().into_owned());
    m.entries.insert(key.clone(), e.clone());
    m.commit(ws).unwrap();
    let r = apply_all(ws, 0);
    assert!(r.done.is_empty(), "{r:?}");
    let mut m = Manifest::load(ws).unwrap();
    m.quarantine.push(QEntry {
        batch: 7,
        key,
        entry: e,
        quarantined_at: 0,
    });
    m.commit(ws).unwrap();
    purge(ws, 100 * DAY, Mode::Apply).unwrap();
    assert_eq!(
        fs::read(outside.path().join("precious")).unwrap(),
        b"user data"
    );
    assert!(ws.join("src/main.rs").exists());
}

#[test]
fn unreadable_plan_fails_closed() {
    let d = workspace();
    let ws = d.path();
    let k = orphan(ws, b"x");
    let id = approve(ws, "a", None, &k);
    fs::write(ws.join(format!(".rune/plans/{id}.json")), b"garbage").unwrap();
    assert!(plan_cleanup(ws).is_err());
}
