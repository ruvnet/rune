mod common;
use common::*;

fn portable() -> tempfile::TempDir {
    fixture("targets/portable")
}

#[test]
fn list_shows_all_four_descriptors() {
    let d = tempfile::tempdir().unwrap();
    let s = ok(&rune(d.path(), &["target", "list"]));
    for t in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "wasm32-wasip1",
        "rvm",
    ] {
        assert!(s.contains(t), "{t}: {s}");
    }
    assert!(s.contains("qemu-aarch64-static") && s.contains("node-wasi"));
}

#[test]
fn native_run_is_classified_native_and_allows_performance_claims() {
    if !cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        eprintln!("SKIP: not an x86_64 Linux host");
        return;
    }
    let d = portable();
    let s = ok(&rune(d.path(), &["target", "run", "--target", "x86_64"]));
    assert!(s.contains("evidence: native"), "{s}");
    assert!(s.contains("performance claims: allowed"), "{s}");
    assert!(s.contains("arch=x86_64"), "{s}");
}

#[test]
fn build_alone_is_compile_only() {
    if !cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        return;
    }
    let d = portable();
    let s = ok(&rune(d.path(), &["target", "build", "--target", "x86_64"]));
    assert!(
        s.contains("evidence: compile_only") && s.contains("performance claims: refused"),
        "{s}"
    );
}

#[test]
fn missing_cpu_feature_is_unvalidated_and_refuses_claims() {
    if !cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        return;
    }
    let d = portable();
    let s = ok(&rune(
        d.path(),
        &[
            "target",
            "run",
            "--target",
            "x86_64",
            "--cpu-feature",
            "no-such-feature",
        ],
    ));
    assert!(s.contains("evidence: unvalidated"), "{s}");
    assert!(s.contains("performance claims: refused"), "{s}");
}

#[test]
fn rvm_is_unvalidated_specification_required() {
    let d = portable();
    let s = ok(&rune(d.path(), &["target", "run", "--target", "rvm"]));
    assert!(
        s.contains("evidence: unvalidated") && s.contains("specification required"),
        "{s}"
    );
    assert!(s.contains("performance claims: refused"), "{s}");
    fails(&rune(d.path(), &["target", "build", "--target", "rvm"]));
}

#[test]
fn aarch64_runs_emulated_when_the_toolchain_exists() {
    if !rust_target_installed("aarch64-unknown-linux-gnu")
        || !tool("aarch64-linux-gnu-gcc")
        || !tool("qemu-aarch64-static")
    {
        eprintln!("SKIP aarch64: cross toolchain or qemu missing");
        return;
    }
    let d = portable();
    let s = ok(&rune(d.path(), &["target", "run", "--target", "aarch64"]));
    assert!(s.contains("evidence: emulated"), "{s}");
    assert!(s.contains("performance claims: refused"), "{s}");
    assert!(s.contains("arch=aarch64"), "{s}");
}

#[test]
fn wasm_runs_emulated_when_node_and_the_target_exist() {
    if !rust_target_installed("wasm32-wasip1") || !tool("node") {
        eprintln!("SKIP wasm32: target or node missing");
        return;
    }
    let d = portable();
    let o = rune(d.path(), &["target", "run", "--target", "wasm32"]);
    let s = out(&o);
    // Older node builds lack a usable node:wasi; the classification must still be honest.
    assert!(
        s.contains("performance claims: refused"),
        "{s}\n{}",
        err(&o)
    );
    assert!(
        s.contains("evidence: emulated") || s.contains("evidence: unvalidated"),
        "{s}"
    );
}
