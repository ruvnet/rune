use rune_core::edit::{EditApplier, EditRequest};
use rune_core::hex_sha256;
use rune_edit::{AgentBoosterSidecar, ExactReplace};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SRC: &str = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
const EDIT: &str = "pub fn sum(a: i32, b: i32) -> i32 { a + b }\n";

fn req(root: &Path, path: &str, src: &str, find: &str, replace: &str) -> EditRequest {
    EditRequest {
        root: root.into(),
        path: path.into(),
        expected_sha256: hex_sha256(src.as_bytes()),
        find: find.into(),
        replace: replace.into(),
        language: "rust".into(),
        max_bytes: 1 << 20,
    }
}

fn tree(src: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("src")).unwrap();
    fs::write(d.path().join("src/lib.rs"), src).unwrap();
    d
}

fn bytes(d: &tempfile::TempDir) -> Vec<u8> {
    fs::read(d.path().join("src/lib.rs")).unwrap()
}

fn node_modules() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../node_modules")
}

fn have_booster() -> bool {
    let ok = node_modules().join("agent-booster").exists();
    if !ok {
        eprintln!("SKIP: node_modules/agent-booster absent; run `npm install --ignore-scripts` at repo root");
    }
    ok
}

// ---- ExactReplace -------------------------------------------------------

#[test]
fn exact_output_equals_expected_text() {
    let d = tree(SRC);
    let r = req(d.path(), "src/lib.rs", SRC, "fn add", "fn sum");
    let o = ExactReplace.apply(&r).unwrap();
    assert_eq!(bytes(&d), EDIT.as_bytes());
    assert_eq!(o.before_sha256, hex_sha256(SRC.as_bytes()));
    assert_eq!(o.after_sha256, hex_sha256(EDIT.as_bytes()));
    assert_eq!(o.applier, "exact_replace");
    assert!(
        fs::read_dir(d.path().join("src")).unwrap().count() == 1,
        "no temp left"
    );
}

#[test]
fn exact_stale_hash_rejected_file_unchanged() {
    let d = tree(SRC);
    let mut r = req(d.path(), "src/lib.rs", SRC, "fn add", "fn sum");
    r.expected_sha256 = hex_sha256(b"something else");
    let e = ExactReplace.apply(&r).unwrap_err();
    assert!(e.to_string().contains("stale"), "{e}");
    assert_eq!(bytes(&d), SRC.as_bytes());
}

#[test]
fn exact_find_must_occur_exactly_once() {
    let src = "a a\n";
    let d = tree(src);
    assert!(ExactReplace
        .apply(&req(d.path(), "src/lib.rs", src, "a", "b"))
        .is_err());
    assert!(ExactReplace
        .apply(&req(d.path(), "src/lib.rs", src, "zzz", "b"))
        .is_err());
    assert!(ExactReplace
        .apply(&req(d.path(), "src/lib.rs", src, "", "b"))
        .is_err());
    assert_eq!(bytes(&d), src.as_bytes());
}

#[test]
fn traversal_and_absolute_rejected() {
    let outer = tempfile::tempdir().unwrap();
    let root = outer.path().join("root");
    fs::create_dir_all(&root).unwrap();
    fs::write(outer.path().join("secret.rs"), SRC).unwrap();
    let abs = outer.path().join("secret.rs");
    for p in [
        "../secret.rs",
        "src/../../secret.rs",
        abs.to_str().unwrap(),
        "",
    ] {
        let r = req(&root, p, SRC, "fn add", "fn sum");
        assert!(ExactReplace.apply(&r).is_err(), "{p}");
        assert!(
            AgentBoosterSidecar::new("/nonexistent").apply(&r).is_err(),
            "{p}"
        );
    }
    assert_eq!(
        fs::read(outer.path().join("secret.rs")).unwrap(),
        SRC.as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn symlink_escape_rejected() {
    let outer = tempfile::tempdir().unwrap();
    let root = outer.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let secret = outer.path().join("secret.rs");
    fs::write(&secret, SRC).unwrap();
    std::os::unix::fs::symlink(&secret, root.join("link.rs")).unwrap();
    std::os::unix::fs::symlink(outer.path(), root.join("dir")).unwrap();
    for p in ["link.rs", "dir/secret.rs"] {
        let r = req(&root, p, SRC, "fn add", "fn sum");
        let e = ExactReplace.apply(&r).unwrap_err();
        assert!(e.to_string().contains("escapes"), "{p}: {e}");
    }
    assert_eq!(fs::read(&secret).unwrap(), SRC.as_bytes());
}

#[test]
fn oversize_rejected() {
    let d = tree(SRC);
    let mut r = req(d.path(), "src/lib.rs", SRC, "fn add", "fn sum");
    r.max_bytes = 10;
    assert!(ExactReplace.apply(&r).is_err());
    // Result growing past the bound is rejected too.
    let mut r = req(d.path(), "src/lib.rs", SRC, "fn add", &"x".repeat(200));
    r.max_bytes = 100;
    assert!(ExactReplace.apply(&r).is_err());
    assert_eq!(bytes(&d), SRC.as_bytes());
}

// ---- Sidecar with a fake `node` (no package needed) ----------------------

#[cfg(unix)]
fn fake_node(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join("fake-node");
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[cfg(unix)]
#[test]
fn unusable_sidecar_results_never_enter_the_tree() {
    let bins = tempfile::tempdir().unwrap();
    let cases = [
        ("garbage", "cat >/dev/null; echo 'not json'"),
        ("empty stdout", "cat >/dev/null"),
        (
            "success false",
            r#"cat >/dev/null; echo '{"success":false,"output":"X"}'"#,
        ),
        (
            "empty output",
            r#"cat >/dev/null; echo '{"success":true,"output":""}'"#,
        ),
        (
            "null output",
            r#"cat >/dev/null; echo '{"success":true,"output":null}'"#,
        ),
        (
            "code not output",
            r#"cat >/dev/null; echo '{"success":true,"code":"X"}'"#,
        ),
        (
            "success string",
            r#"cat >/dev/null; echo '{"success":"true","output":"X"}'"#,
        ),
        (
            "oversize",
            r#"cat >/dev/null; echo '{"success":true,"output":"0123456789012345678901234567890123456789"}'"#,
        ),
        (
            "nonzero exit",
            r#"cat >/dev/null; echo '{"success":true,"output":"X"}'; exit 3"#,
        ),
    ];
    for (i, (label, body)) in cases.iter().enumerate() {
        let sub = bins.path().join(i.to_string());
        fs::create_dir_all(&sub).unwrap();
        let node = fake_node(&sub, body);
        let d = tree(SRC);
        let mut r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
        r.max_bytes = 100;
        if *label == "oversize" {
            r.max_bytes = 43;
        }
        let a = AgentBoosterSidecar::new("/nonexistent").with_node(node);
        assert!(a.apply(&r).is_err(), "{label} must be rejected");
        assert_eq!(bytes(&d), SRC.as_bytes(), "{label}: file must be untouched");
    }
}

#[cfg(unix)]
#[test]
fn stale_hash_rejected_before_sidecar_is_spawned() {
    let bins = tempfile::tempdir().unwrap();
    let marker = bins.path().join("spawned");
    let node = fake_node(
        bins.path(),
        &format!("touch {}; cat >/dev/null", marker.display()),
    );
    let d = tree(SRC);
    let mut r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
    r.expected_sha256 = hex_sha256(b"stale");
    let a = AgentBoosterSidecar::new("/nonexistent").with_node(&node);
    let e = a.apply(&r).unwrap_err();
    assert!(e.to_string().contains("stale"), "{e}");
    assert!(!marker.exists(), "sidecar must not run on a stale hash");
    assert_eq!(bytes(&d), SRC.as_bytes());
    // Control: with the right hash the same fake IS spawned (then rejected: no output).
    let r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
    assert!(a.apply(&r).is_err());
    assert!(marker.exists());
}

#[cfg(unix)]
#[test]
fn sidecar_timeout_kills_and_rejects() {
    let bins = tempfile::tempdir().unwrap();
    let node = fake_node(bins.path(), "exec sleep 30");
    let d = tree(SRC);
    let r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
    let a = AgentBoosterSidecar::new("/nonexistent")
        .with_node(node)
        .with_timeout(Duration::from_millis(300));
    let t = std::time::Instant::now();
    let e = a.apply(&r).unwrap_err();
    assert!(e.to_string().contains("timed out"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(10));
    assert_eq!(bytes(&d), SRC.as_bytes());
}

#[cfg(unix)]
#[test]
fn fake_success_is_applied_with_confidence() {
    let bins = tempfile::tempdir().unwrap();
    let node = fake_node(
        bins.path(),
        r#"cat >/dev/null; echo '{"success":true,"output":"fn x() {}","confidence":0.5}'"#,
    );
    let d = tree(SRC);
    let r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
    let o = AgentBoosterSidecar::new("/nonexistent")
        .with_node(node)
        .apply(&r)
        .unwrap();
    assert_eq!(bytes(&d), b"fn x() {}");
    assert_eq!(o.confidence, Some(0.5));
}

// ---- Real agent-booster 0.2.2 ---------------------------------------------

#[test]
fn real_agent_booster_output_exactly_equals_expected() {
    if !have_booster() {
        return;
    }
    let d = tree(SRC);
    let r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
    let o = AgentBoosterSidecar::new(node_modules()).apply(&r).unwrap();
    assert_eq!(bytes(&d), EDIT.as_bytes());
    assert_eq!(o.after_sha256, hex_sha256(EDIT.as_bytes()));
    assert_eq!(o.applier, "agent_booster_sidecar");
    assert!(o.confidence.is_some());
    eprintln!("real agent-booster ran; confidence {:?}", o.confidence);
}

#[test]
fn real_agent_booster_stale_hash_leaves_file() {
    if !have_booster() {
        return;
    }
    let d = tree(SRC);
    let mut r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
    r.expected_sha256 = hex_sha256(b"old");
    assert!(AgentBoosterSidecar::new(node_modules()).apply(&r).is_err());
    assert_eq!(bytes(&d), SRC.as_bytes());
}

#[test]
fn real_agent_booster_missing_package_is_rejected() {
    if !have_booster() {
        return;
    }
    let d = tree(SRC);
    let r = req(d.path(), "src/lib.rs", SRC, EDIT, "");
    let empty = tempfile::tempdir().unwrap();
    assert!(AgentBoosterSidecar::new(empty.path()).apply(&r).is_err());
    assert_eq!(bytes(&d), SRC.as_bytes());
}
