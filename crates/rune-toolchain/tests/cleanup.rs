mod common;
use common::*;
use rune_gc::{put_object, Kind, Retention};
use std::fs;
use std::path::Path;

fn setup() -> (tempfile::TempDir, String, String, String) {
    let d = fixture("hello");
    ok(&rune(d.path(), &["build", "--freeze"]));
    let plan = history(d.path())[0].clone();
    // Unreachable generated object (owned by a plan that is not a root).
    let junk = put_object(
        d.path(),
        Kind::Artifact,
        &"e".repeat(64),
        None,
        b"orphaned build output",
    )
    .unwrap();
    // Reachable: evidence retained while its plan is a root.
    let kept = put_object(
        d.path(),
        Kind::Evidence,
        &plan,
        Some(Retention::WhilePlanReachable),
        b"evidence for the live plan",
    )
    .unwrap();
    (d, plan, junk, kept)
}

fn obj(ws: &Path, key: &str) -> bool {
    ws.join(".rune/objects").join(key).exists()
}

fn in_quarantine(ws: &Path, key: &str) -> bool {
    listing(&ws.join(".rune/quarantine"))
        .iter()
        .any(|(p, _)| p.ends_with(key))
}

fn manifest(ws: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(ws.join(".rune/ownership.json")).unwrap()).unwrap()
}

#[test]
fn dry_run_is_the_default_and_changes_nothing() {
    let (d, _, junk, kept) = setup();
    let before = listing(d.path());
    let m = fs::read(d.path().join(".rune/ownership.json")).unwrap();
    for args in [&["cleanup"][..], &["cleanup", "--purge"]] {
        let s = ok(&rune(d.path(), args));
        assert!(s.contains("dry run"), "{s}");
        assert!(s.contains(&format!("collect  {junk}")), "{s}");
        assert!(s.contains(&format!("keep     {kept}")), "{s}");
        assert!(
            s.contains("unreachable from plans"),
            "reasons are printed: {s}"
        );
    }
    assert_eq!(listing(d.path()), before);
    assert_eq!(fs::read(d.path().join(".rune/ownership.json")).unwrap(), m);
}

#[test]
fn apply_quarantines_only_unreachable_generated_objects() {
    let (d, _, junk, kept) = setup();
    let src = fs::read(d.path().join("src/main.rs")).unwrap();
    let art = manifest(d.path())["entries"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, e)| e["kind"] == "artifact")
        .map(|(k, _)| k.clone())
        .find(|k| *k != junk)
        .expect("build registered its artifact");
    let s = ok(&rune(d.path(), &["cleanup", "--apply"]));
    assert!(s.contains("rune cleanup --restore"), "{s}");
    assert!(!obj(d.path(), &junk) && in_quarantine(d.path(), &junk));
    assert!(obj(d.path(), &kept), "evidence of a live plan stays");
    assert!(obj(d.path(), &art), "the receipt keeps the built artifact");
    assert_eq!(fs::read(d.path().join("src/main.rs")).unwrap(), src);
    assert!(d.path().join("Cargo.toml").exists());
    // Source is not part of the ownership manifest at all.
    assert!(!manifest(d.path()).to_string().contains("main.rs"));
    // Nothing is deleted yet: fresh quarantine survives an applied purge.
    let p = ok(&rune(d.path(), &["cleanup", "--apply", "--purge"]));
    assert!(p.contains("purged (older than 7 days): 0 object"), "{p}");
    assert!(in_quarantine(d.path(), &junk));
    ok(&rune(d.path(), &["build"]));
}

#[test]
fn purge_removes_only_quarantined_objects_older_than_seven_days() {
    let (d, _, junk, kept) = setup();
    ok(&rune(d.path(), &["cleanup", "--apply"]));
    // A dry-run purge of an expired object still deletes nothing.
    let path = d.path().join(".rune/ownership.json");
    let mut m = manifest(d.path());
    m["quarantine"][0]["quarantined_at"] = 0.into();
    m["version"] = (m["version"].as_u64().unwrap()).into();
    fs::write(&path, serde_json::to_vec_pretty(&m).unwrap()).unwrap();
    let dry = ok(&rune(d.path(), &["cleanup", "--purge"]));
    assert!(dry.contains("would purge"), "{dry}");
    assert!(in_quarantine(d.path(), &junk));
    let s = ok(&rune(d.path(), &["cleanup", "--apply", "--purge"]));
    assert!(s.contains("purged (older than 7 days): 1 object"), "{s}");
    assert!(!in_quarantine(d.path(), &junk) && !obj(d.path(), &junk));
    assert!(obj(d.path(), &kept), "reachable objects are never purged");
    assert!(d.path().join("src/main.rs").exists());
}

#[test]
fn restore_undoes_a_quarantine_batch() {
    let (d, _, junk, _) = setup();
    let s = ok(&rune(d.path(), &["cleanup", "--apply"]));
    let batch = s
        .split("--restore ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert!(!obj(d.path(), &junk));
    ok(&rune(d.path(), &["cleanup", "--restore", &batch]));
    assert!(obj(d.path(), &junk));
    fails(&rune(d.path(), &["cleanup", "--restore", "999"]));
}

#[test]
fn a_live_lease_protects_its_object() {
    let (d, _, junk, _) = setup();
    rune_gc::leases::acquire(d.path(), "held-by-this-test", std::slice::from_ref(&junk)).unwrap();
    let s = ok(&rune(d.path(), &["cleanup", "--apply"]));
    assert!(s.contains("held by live lease"), "{s}");
    assert!(obj(d.path(), &junk));
}
