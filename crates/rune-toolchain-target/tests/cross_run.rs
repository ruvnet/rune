//! Real cross builds of tests/fixtures/targets/portable, run under the
//! available emulator/runtime. A test is skipped (with a printed note) only
//! when its tool is truly missing.

use rune_core::target::{Evidence, TargetDescriptor};
use rune_target::cross::{cross_build, CrossOptions};
use rune_target::descriptors::*;
use rune_target::exec::{find_executable, perf_claim_allowed, run_and_classify};
use rune_target::host::Host;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/targets/portable")
}

fn rustup_has(triple: &str) -> bool {
    std::process::Command::new("rustc")
        .args(["--print", "target-libdir", "--target", triple])
        .output()
        .ok()
        .and_then(|o| std::fs::read_dir(String::from_utf8_lossy(&o.stdout).trim()).ok())
        .is_some_and(|mut d| d.next().is_some())
}

fn build_and_run(
    t: &TargetDescriptor,
) -> (
    rune_target::cross::CrossArtifact,
    rune_target::exec::Outcome,
) {
    let td = tempfile::tempdir().unwrap();
    let art = cross_build(&fixture(), td.path(), t, &CrossOptions::default()).unwrap();
    assert_eq!(art.evidence, Evidence::CompileOnly);
    assert_eq!(art.digest.len(), 64);
    let out = run_and_classify(t, &Host::detect(), &art.path, &[]).unwrap();
    (art, out)
}

fn stdout(o: &rune_target::exec::Outcome) -> String {
    let out = o.output.as_ref().expect("executed");
    assert!(
        out.status.success(),
        "exit {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn native_x86_64_runs_native() {
    let host = Host::detect();
    if host.triple != X86_64_LINUX {
        eprintln!("SKIP: host is {}", host.triple);
        return;
    }
    let (_, o) = build_and_run(&x86_64_linux_gnu(&["sse2"]));
    assert_eq!(o.evidence, Evidence::Native);
    assert!(perf_claim_allowed(o.evidence));
    assert!(stdout(&o).contains("arch=x86_64"));
}

#[test]
fn aarch64_cross_builds_and_runs_emulated() {
    if !rustup_has(AARCH64_LINUX)
        || find_executable("aarch64-linux-gnu-gcc").is_none()
        || find_executable(RUNTIME_QEMU_AARCH64).is_none()
    {
        eprintln!("SKIP aarch64: rust target, cross gcc or qemu-aarch64-static missing");
        return;
    }
    let t = aarch64_linux_gnu(&[]);
    let (art, o) = build_and_run(&t);
    assert_eq!(art.triple, AARCH64_LINUX);
    assert_eq!(o.evidence, Evidence::Emulated);
    assert!(!perf_claim_allowed(o.evidence));
    assert!(stdout(&o).contains("arch=aarch64"), "{}", stdout(&o));
    eprintln!("RAN aarch64 under qemu-user: {}", stdout(&o).trim());
}

#[test]
fn wasm32_cross_builds_and_runs_under_node_wasi_emulated() {
    if !rustup_has(WASM32_WASI) || find_executable("node").is_none() {
        eprintln!("SKIP wasm32: rust target or node missing");
        return;
    }
    let (art, o) = build_and_run(&wasm32_wasip1());
    assert!(art.path.extension().is_some_and(|e| e == "wasm"));
    assert_eq!(o.evidence, Evidence::Emulated);
    assert!(!perf_claim_allowed(o.evidence));
    assert!(stdout(&o).contains("arch=wasm32"), "{}", stdout(&o));
    eprintln!("RAN wasm32 under node:wasi: {}", stdout(&o).trim());
}

/// The developer shell may export mold `link-arg`s in RUSTFLAGS, which break
/// cross and wasm links. The builds above succeed regardless because
/// `cross_build` always sets RUSTFLAGS explicitly; this pins that the
/// command carries an explicit (empty) value rather than inheriting.
#[test]
fn build_command_overrides_ambient_rustflags() {
    let cmd = rune_target::cross::build_command(
        &fixture(),
        std::path::Path::new("/t"),
        &wasm32_wasip1(),
        &CrossOptions::default(),
    );
    let rf = cmd.get_envs().find(|(k, _)| *k == "RUSTFLAGS").unwrap().1;
    assert_eq!(
        rf.map(|v| v.to_string_lossy().into_owned()),
        Some(String::new())
    );
}
