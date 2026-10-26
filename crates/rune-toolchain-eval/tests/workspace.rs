//! ADR 007: the copy-on-write fork, its manifest, and containment of the
//! processes run inside it.
mod common;
use common::*;
use rune_eval::runner::{run_limited, Capabilities, Limits, SandboxEnv};
use rune_eval::workspace::{resolve_inside, MANIFEST};
use rune_eval::CowWorkspace;
use std::fs;
use std::path::Path;

fn limits(deny_net: bool) -> Limits {
    Limits {
        wall_secs: 20,
        mem_mb: 2048,
        cpu_secs: 20,
        deny_net,
        pin_core: None,
    }
}

#[test]
fn fork_is_skipping_deterministic_and_owned() {
    let src = tempfile::tempdir().unwrap();
    fs::create_dir_all(src.path().join("src")).unwrap();
    fs::create_dir_all(src.path().join("target/debug")).unwrap();
    fs::create_dir_all(src.path().join(".git")).unwrap();
    fs::write(src.path().join("src/lib.rs"), "pub fn f() {}").unwrap();
    fs::write(src.path().join("target/debug/junk"), "x").unwrap();
    fs::write(src.path().join(".git/HEAD"), "ref").unwrap();
    let out = tempfile::tempdir().unwrap();
    let a = CowWorkspace::fork(src.path(), &out.path().join("a")).unwrap();
    let b = CowWorkspace::fork(src.path(), &out.path().join("b")).unwrap();
    assert!(!out.path().join("a/target").exists() && !out.path().join("a/.git").exists());
    assert_eq!(a.manifest.files.len(), 1);
    assert_eq!(a.manifest.files[0].path, "src/lib.rs");
    // timestamp-free: identical bytes for identical sources
    assert_eq!(
        fs::read(out.path().join("a").join(MANIFEST)).unwrap(),
        fs::read(out.path().join("b").join(MANIFEST)).unwrap()
    );
    assert!(a.drifted().unwrap().is_empty());
    fs::write(out.path().join("a/src/lib.rs"), "tampered").unwrap();
    assert_eq!(a.drifted().unwrap(), vec!["src/lib.rs".to_string()]);
    // writes in the fork never reach the source
    assert_eq!(
        fs::read_to_string(src.path().join("src/lib.rs")).unwrap(),
        "pub fn f() {}"
    );
    drop(b);
}

#[test]
fn symlink_escape_and_traversal_cannot_leave_the_fork() {
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "TOP SECRET").unwrap();
    let src = tempfile::tempdir().unwrap();
    fs::write(src.path().join("ok.rs"), "").unwrap();
    std::os::unix::fs::symlink(outside.path(), src.path().join("dirlink")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        src.path().join("filelink"),
    )
    .unwrap();
    let dest = tempfile::tempdir().unwrap();
    let mut ws = CowWorkspace::fork(src.path(), &dest.path().join("f")).unwrap();
    // neither the links nor the files behind them were copied
    assert!(!dest.path().join("f/dirlink").exists() && !dest.path().join("f/filelink").exists());
    assert_eq!(ws.manifest.skipped_symlinks, vec!["dirlink", "filelink"]);
    assert!(!ws.manifest.files.iter().any(|f| f.path.contains("secret")));
    // traversal in installs is refused
    assert!(ws.install_file(Path::new("../evil"), b"x").is_err());
    assert!(ws.install_file(Path::new("/tmp/evil"), b"x").is_err());
    assert!(ws.install_file(Path::new(MANIFEST), b"x").is_err());
    // a link planted after the fork (e.g. by a build) is refused too
    std::os::unix::fs::symlink(outside.path(), ws.root.join("planted")).unwrap();
    assert!(resolve_inside(&ws.root, Path::new("planted/x")).is_err());
    assert!(ws.install_file(Path::new("planted/x"), b"x").is_err());
    assert!(!outside.path().join("x").exists());
    // nested fork destinations are refused
    assert!(CowWorkspace::fork(src.path(), &src.path().join("inside")).is_err());
}

#[test]
fn network_is_denied_for_processes_run_in_a_fork() {
    let caps = Capabilities::probe();
    if !caps.netns {
        eprintln!("netns unavailable: {:?}", caps.notes);
        return;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let tmp = tempfile::tempdir().unwrap();
    let ws = CowWorkspace::fork(&baseline(), &tmp.path().join("fork")).unwrap();
    let env = SandboxEnv::new(&tmp.path().join("home"), &tmp.path().join("cargo-home"));
    let probe = [
        "bash".to_string(),
        "-c".to_string(),
        format!("exec 3<>/dev/tcp/127.0.0.1/{port}"),
    ];
    let denied = run_limited(&probe, &ws.root, &limits(true), &env).unwrap();
    assert!(!denied.success(), "connect must fail inside the namespace");
    assert!(denied.layers.iter().any(|l| l == "netns"));
    let control = run_limited(&probe, &ws.root, &limits(false), &env).unwrap();
    assert!(
        control.success(),
        "same probe reaches the listener without the namespace"
    );
}

#[test]
fn cargo_builds_offline_in_a_denied_network_fork() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = CowWorkspace::fork(&baseline(), &tmp.path().join("fork")).unwrap();
    let env = SandboxEnv::new(&tmp.path().join("home"), &tmp.path().join("cargo-home"));
    fs::create_dir_all(&env.cargo_home).unwrap();
    let cmd: Vec<String> = ["cargo", "build", "--release", "--offline"]
        .map(String::from)
        .to_vec();
    let mut l = limits(true);
    l.wall_secs = 180;
    l.mem_mb = 8192;
    let r = run_limited(&cmd, &ws.root, &l, &env).unwrap();
    assert!(r.success(), "{}", r.stderr);
}
