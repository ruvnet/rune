mod common;
use common::*;
use std::fs;

fn sha(ws: &std::path::Path, rel: &str) -> String {
    rune_core::file_digest(&ws.join(rel)).unwrap()
}

#[test]
fn exact_replace_edits_and_prints_both_hashes() {
    let d = fixture("hello");
    let before = sha(d.path(), "src/main.rs");
    let text = fs::read_to_string(d.path().join("src/main.rs")).unwrap();
    let find = text.lines().find(|l| l.contains("println")).unwrap().trim();
    let o = rune(
        d.path(),
        &[
            "edit",
            "--path",
            "src/main.rs",
            "--expected-sha256",
            &before,
            "--find",
            find,
            "--replace",
            "println!(\"edited\");",
        ],
    );
    let s = ok(&o);
    let after = sha(d.path(), "src/main.rs");
    assert_ne!(before, after);
    assert!(s.contains(&format!("sha256:{before}")) && s.contains(&format!("sha256:{after}")));
    assert!(fs::read_to_string(d.path().join("src/main.rs"))
        .unwrap()
        .contains("edited"));
}

#[test]
fn stale_hash_is_rejected_and_file_untouched() {
    let d = fixture("hello");
    let orig = fs::read(d.path().join("src/main.rs")).unwrap();
    let o = rune(
        d.path(),
        &[
            "edit",
            "--path",
            "src/main.rs",
            "--expected-sha256",
            &"0".repeat(64),
            "--find",
            "fn main",
            "--replace",
            "fn other",
        ],
    );
    assert!(fails(&o).contains("stale"), "{}", err(&o));
    assert_eq!(fs::read(d.path().join("src/main.rs")).unwrap(), orig);
}

#[test]
fn path_escape_is_rejected() {
    let d = fixture("hello");
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("x.rs"), "fn main() {}\n").unwrap();
    let h = rune_core::file_digest(&outside.path().join("x.rs")).unwrap();
    let abs = outside.path().join("x.rs");
    for p in ["../x.rs", abs.to_str().unwrap()] {
        let o = rune(
            d.path(),
            &[
                "edit",
                "--path",
                p,
                "--expected-sha256",
                &h,
                "--find",
                "main",
                "--replace",
                "evil",
            ],
        );
        assert_eq!(o.status.code(), Some(1), "{p}: {}", err(&o));
    }
    assert_eq!(
        fs::read_to_string(outside.path().join("x.rs")).unwrap(),
        "fn main() {}\n"
    );
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_rejected() {
    let d = fixture("hello");
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("t.rs"), "fn main() {}\n").unwrap();
    std::os::unix::fs::symlink(outside.path().join("t.rs"), d.path().join("link.rs")).unwrap();
    let h = rune_core::file_digest(&outside.path().join("t.rs")).unwrap();
    let o = rune(
        d.path(),
        &[
            "edit",
            "--path",
            "link.rs",
            "--expected-sha256",
            &h,
            "--find",
            "main",
            "--replace",
            "x",
        ],
    );
    fails(&o);
    assert_eq!(
        fs::read_to_string(outside.path().join("t.rs")).unwrap(),
        "fn main() {}\n"
    );
}

#[test]
fn missing_or_ambiguous_find_text_is_rejected() {
    let d = fixture("hello");
    let h = sha(d.path(), "src/main.rs");
    let o = rune(
        d.path(),
        &[
            "edit",
            "--path",
            "src/main.rs",
            "--expected-sha256",
            &h,
            "--find",
            "no such text anywhere",
            "--replace",
            "x",
        ],
    );
    fails(&o);
    assert_eq!(sha(d.path(), "src/main.rs"), h);
}

#[test]
fn booster_without_the_package_fails_and_leaves_the_file() {
    let d = fixture("hello");
    let h = sha(d.path(), "src/main.rs");
    let empty = tempfile::tempdir().unwrap();
    let o = rune(
        d.path(),
        &[
            "edit",
            "--booster",
            "--node-modules",
            empty.path().to_str().unwrap(),
            "--path",
            "src/main.rs",
            "--expected-sha256",
            &h,
            "--find",
            "fn main() {}",
        ],
    );
    fails(&o);
    assert_eq!(sha(d.path(), "src/main.rs"), h);
}
