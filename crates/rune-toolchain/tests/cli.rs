use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture() -> tempfile::TempDir {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/hello");
    let dir = tempfile::tempdir().unwrap();
    copy(&src, dir.path());
    dir
}

fn copy(from: &Path, to: &Path) {
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            fs::create_dir_all(&dest).unwrap();
            copy(&e.path(), &dest);
        } else {
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn rune(ws: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rune"))
        .arg("-C")
        .arg(ws)
        .args(args)
        .env_remove("OPENROUTER_API_KEY")
        // Blocked egress: any attempted network use fails fast.
        .env("https_proxy", "http://127.0.0.1:9")
        .env("http_proxy", "http://127.0.0.1:9")
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn digest_of(out: &str) -> String {
    out.split("sha256:")
        .nth(1)
        .expect("digest in output")
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

fn plain_cargo_digest(ws: &Path) -> String {
    let target: PathBuf = ws.join("plain-target");
    let ok = Command::new("cargo")
        .current_dir(ws)
        .args([
            "build",
            "--release",
            "--locked",
            "--offline",
            "--target-dir",
        ])
        .arg(&target)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    rune_core::file_digest(&target.join("release/hello")).unwrap()
}

#[test]
fn build_matches_plain_cargo_on_unchanged_workspace() {
    let d = fixture();
    let o = rune(d.path(), &["build", "--freeze"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(digest_of(&stdout(&o)), plain_cargo_digest(d.path()));
}

#[test]
fn verify_reproduces_recorded_artifact() {
    let d = fixture();
    assert!(rune(d.path(), &["build", "--freeze"]).status.success());
    let v = rune(d.path(), &["verify"]);
    assert!(v.status.success(), "{}", stderr(&v));
    assert!(stdout(&v).contains("verified"));
}

#[test]
fn replay_records_no_model_calls_and_offline() {
    let d = fixture();
    assert!(rune(d.path(), &["build", "--freeze"]).status.success());
    let e = rune(d.path(), &["explain"]);
    assert!(
        stdout(&e).contains("offline=true, model calls=0"),
        "{}",
        stdout(&e)
    );
}

#[test]
fn stale_source_is_rejected() {
    let d = fixture();
    assert!(rune(d.path(), &["build", "--freeze"]).status.success());
    fs::write(
        d.path().join("src/main.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .unwrap();
    let o = rune(d.path(), &["build"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("stale plan: source_digest"),
        "{}",
        stderr(&o)
    );
    // Re-freezing approves the new inputs.
    assert!(rune(d.path(), &["build", "--freeze"]).status.success());
}

#[test]
fn build_without_plan_is_rejected() {
    let d = fixture();
    let o = rune(d.path(), &["build"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("no approved plan"));
}

#[test]
fn rollback_restores_previous_plan() {
    let d = fixture();
    assert!(rune(d.path(), &["build", "--freeze"]).status.success());
    let first = stdout(&rune(d.path(), &["explain"]));
    fs::write(d.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    assert!(rune(d.path(), &["build", "--freeze"]).status.success());
    let r = rune(d.path(), &["rollback"]);
    assert!(r.status.success(), "{}", stderr(&r));
    assert_eq!(stdout(&rune(d.path(), &["explain"])), first);
    assert_eq!(rune(d.path(), &["rollback"]).status.code(), Some(1));
}

#[test]
fn zero_budget_rejected_before_any_work() {
    let d = fixture();
    let o = rune(d.path(), &["build", "--freeze", "--max-usd", "0"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("budget"));
    assert!(!d.path().join(".rune/history").exists());
}

#[test]
fn improve_without_credentials_exits_2() {
    let d = fixture();
    let o = rune(d.path(), &["improve"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("OPENROUTER_API_KEY"));
}

#[test]
fn tampered_plan_is_rejected() {
    let d = fixture();
    assert!(rune(d.path(), &["build", "--freeze"]).status.success());
    let plans = d.path().join(".rune/plans");
    let f = fs::read_dir(&plans)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let t = fs::read_to_string(&f)
        .unwrap()
        .replace("\"max_tokens\": 200000", "\"max_tokens\": 999999999");
    fs::write(&f, t).unwrap();
    let o = rune(d.path(), &["build"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("plan id mismatch"), "{}", stderr(&o));
}
