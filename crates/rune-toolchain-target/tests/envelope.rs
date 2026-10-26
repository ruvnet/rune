use rune_core::target::Evidence;
use rune_core::{Plan, Policy};
use rune_target::descriptors::*;
use rune_target::envelope::{write_envelope, EnvelopeInputs, TestSummary};
use rune_target::exec::{Executor, NativeExecutor, QemuUserExecutor, UnavailableExecutor};
use rune_target::gate::{check_loadable, check_loadable_with, LoadError, LoadStatus};
use rune_target::host::Host;
use rune_target::sign::{load_verifying_key, to_hex};
use rune_target::verify::{verify_envelope, SignatureStatus, VerifyOptions};
use std::fs;
use std::path::{Path, PathBuf};

fn plan(target: &str) -> Plan {
    Plan::new(
        "s".repeat(64),
        "l".repeat(64),
        "rustc test".into(),
        target.into(),
        Policy {
            max_usd: 1.0,
            max_tokens: 10,
            max_seconds: 10,
            max_memory_mb: 10,
        },
        None,
    )
}

/// Ephemeral key from the OS RNG; never persisted outside the tempdir.
fn write_ephemeral_key(dir: &Path, name: &str) -> PathBuf {
    use std::io::Read;
    let mut buf = [0u8; 32];
    fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut buf)
        .unwrap();
    let p = dir.join(name);
    fs::write(&p, to_hex(&buf)).unwrap();
    p
}

fn make(dir: &Path, key: Option<&Path>) -> PathBuf {
    let exe = dir.join("prog.bin");
    fs::write(&exe, b"\x7fELF-fake-executable").unwrap();
    let log = dir.join("tests.log");
    fs::write(&log, "ok").unwrap();
    let target = x86_64_linux_gnu(&["sse2"]);
    let p = plan(&target.triple);
    let out = dir.join("env");
    write_envelope(
        &out,
        &EnvelopeInputs {
            plan: &p,
            target: &target,
            evidence: Evidence::Native,
            tests: TestSummary {
                command: "cargo test".into(),
                passed: 3,
                failed: 0,
                ignored: 0,
            },
            contract_digest: "c".repeat(64),
            executables: vec![("prog".into(), exe)],
            files: vec![("tests.log".into(), log)],
        },
        key,
    )
    .unwrap();
    out
}

#[test]
fn unsigned_roundtrip_binds_inputs() {
    let d = tempfile::tempdir().unwrap();
    let out = make(d.path(), None);
    let r = verify_envelope(&out, &VerifyOptions::default()).unwrap();
    assert_eq!(r.signature, SignatureStatus::Absent);
    assert_eq!(r.files_checked, 2);
    let m = r.manifest;
    assert_eq!(m.plan_id, plan(&m.target.triple).id());
    assert_eq!(m.target_digest, m.target.digest());
    assert_eq!(m.executables[0].path, "bin/prog");
    let required = VerifyOptions {
        require_signature: true,
        ..Default::default()
    };
    assert!(verify_envelope(&out, &required).is_err());
}

#[test]
fn signed_roundtrip_and_trusted_key() {
    let d = tempfile::tempdir().unwrap();
    let key = write_ephemeral_key(d.path(), "k.hex");
    let out = make(d.path(), Some(&key));
    let r = verify_envelope(&out, &VerifyOptions::default()).unwrap();
    assert_eq!(r.signature, SignatureStatus::SelfAsserted);
    // Trusted public key from the signer's own secret.
    let sk = rune_target::sign::load_signing_key(&key).unwrap();
    let pubp = d.path().join("pub.hex");
    fs::write(&pubp, to_hex(sk.verifying_key().as_bytes())).unwrap();
    let trusted = VerifyOptions {
        trusted_key: Some(load_verifying_key(&pubp).unwrap()),
        require_signature: true,
    };
    assert_eq!(
        verify_envelope(&out, &trusted).unwrap().signature,
        SignatureStatus::Trusted
    );
    // A different trusted key rejects it.
    let other = write_ephemeral_key(d.path(), "o.hex");
    let osk = rune_target::sign::load_signing_key(&other).unwrap();
    fs::write(&pubp, to_hex(osk.verifying_key().as_bytes())).unwrap();
    let wrong = VerifyOptions {
        trusted_key: Some(load_verifying_key(&pubp).unwrap()),
        require_signature: true,
    };
    assert!(verify_envelope(&out, &wrong).is_err());
}

#[test]
fn tampered_executable_fails() {
    let d = tempfile::tempdir().unwrap();
    let out = make(d.path(), None);
    let exe = out.join("bin/prog");
    let mut b = fs::read(&exe).unwrap();
    b[3] ^= 1;
    fs::write(&exe, b).unwrap();
    assert!(verify_envelope(&out, &VerifyOptions::default()).is_err());
}

#[test]
fn tampered_manifest_fails_signature() {
    let d = tempfile::tempdir().unwrap();
    let key = write_ephemeral_key(d.path(), "k.hex");
    let out = make(d.path(), Some(&key));
    let mp = out.join("artifact.json");
    let s = fs::read_to_string(&mp)
        .unwrap()
        .replace("\"passed\": 3", "\"passed\": 9");
    fs::write(&mp, s).unwrap();
    let err = verify_envelope(&out, &VerifyOptions::default()).unwrap_err();
    assert!(err.to_string().contains("signature"), "{err}");
}

#[test]
fn tampered_manifest_fails_unsigned_digest_binding() {
    let d = tempfile::tempdir().unwrap();
    let out = make(d.path(), None);
    let mp = out.join("artifact.json");
    let m = fs::read_to_string(&mp).unwrap();
    let sha = m.split("\"sha256\": \"").nth(1).unwrap()[..64].to_owned();
    fs::write(&mp, m.replace(&sha, &"0".repeat(64))).unwrap();
    assert!(verify_envelope(&out, &VerifyOptions::default()).is_err());
}

#[test]
fn tampered_signature_and_stray_file_fail() {
    let d = tempfile::tempdir().unwrap();
    let key = write_ephemeral_key(d.path(), "k.hex");
    let out = make(d.path(), Some(&key));
    let sp = out.join("artifact.sig");
    let orig = fs::read_to_string(&sp).unwrap();
    let sig = orig.split("\"signature\": \"").nth(1).unwrap()[..8].to_owned();
    let flipped = if sig.starts_with('0') { "1" } else { "0" }.to_owned() + &sig[1..];
    fs::write(&sp, orig.replacen(&sig, &flipped, 1)).unwrap();
    assert!(verify_envelope(&out, &VerifyOptions::default()).is_err());
    fs::write(&sp, orig).unwrap();
    assert!(verify_envelope(&out, &VerifyOptions::default()).is_ok());
    fs::write(out.join("bin/extra"), "x").unwrap();
    assert!(verify_envelope(&out, &VerifyOptions::default()).is_err());
}

#[test]
fn unsafe_names_and_nonempty_dir_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    let exe = d.path().join("e");
    fs::write(&exe, "x").unwrap();
    let target = x86_64_linux_gnu(&[]);
    let p = plan(&target.triple);
    let inputs = |name: &str| EnvelopeInputs {
        plan: &p,
        target: &target,
        evidence: Evidence::CompileOnly,
        tests: TestSummary {
            command: String::new(),
            passed: 0,
            failed: 0,
            ignored: 0,
        },
        contract_digest: String::new(),
        executables: vec![(name.into(), exe.clone())],
        files: vec![],
    };
    for bad in ["../evil", "/abs", "a/../b", ""] {
        assert!(
            write_envelope(&d.path().join("o"), &inputs(bad), None).is_err(),
            "{bad}"
        );
        let _ = fs::remove_dir_all(d.path().join("o"));
    }
    // Plan/descriptor target disagreement is refused.
    let wrong = plan("aarch64-unknown-linux-gnu");
    let mut i = inputs("ok");
    i.plan = &wrong;
    assert!(write_envelope(&d.path().join("o2"), &i, None).is_err());
}

fn manifest_for(t: rune_core::target::TargetDescriptor) -> rune_target::envelope::Manifest {
    let d = tempfile::tempdir().unwrap();
    let exe = d.path().join("e");
    fs::write(&exe, "x").unwrap();
    let p = plan(&t.triple);
    write_envelope(
        &d.path().join("o"),
        &EnvelopeInputs {
            plan: &p,
            target: &t,
            evidence: Evidence::CompileOnly,
            tests: TestSummary {
                command: String::new(),
                passed: 0,
                failed: 0,
                ignored: 0,
            },
            contract_digest: String::new(),
            executables: vec![("e".into(), exe)],
            files: vec![],
        },
        None,
    )
    .unwrap()
}

#[test]
fn gate_blocks_missing_cpu_feature() {
    let host = Host::detect();
    let mut t = x86_64_linux_gnu(&["definitely_not_a_cpu_flag"]);
    t.triple = host.triple.clone();
    t.abi = host.abi.clone();
    let m = manifest_for(t);
    let e = check_loadable_with(&m, &host, &NativeExecutor).unwrap_err();
    assert_eq!(
        e,
        LoadError::MissingCpuFeatures(vec!["definitely_not_a_cpu_flag".into()])
    );
}

#[test]
fn gate_blocks_abi_mismatch_and_passes_matching() {
    let host = Host::detect();
    let mut t = x86_64_linux_gnu(&[]);
    t.triple = host.triple.clone();
    t.abi = "some-other-abi".into();
    let e = check_loadable_with(&manifest_for(t.clone()), &host, &NativeExecutor).unwrap_err();
    assert!(matches!(e, LoadError::AbiMismatch { .. }));
    t.abi = host.abi.clone();
    let ok = check_loadable_with(&manifest_for(t), &host, &NativeExecutor).unwrap();
    assert_eq!(
        ok,
        LoadStatus::Loadable {
            evidence: Evidence::Native
        }
    );
}

#[test]
fn gate_blocks_triple_mismatch_for_native_runtime() {
    let host = Host::detect();
    let mut t = x86_64_linux_gnu(&[]);
    t.triple = "riscv64gc-unknown-linux-gnu".into();
    t.abi = host.abi.clone();
    let e = check_loadable_with(&manifest_for(t), &host, &NativeExecutor).unwrap_err();
    assert!(matches!(e, LoadError::TripleMismatch { .. }));
}

#[test]
fn gate_missing_executor_is_unvalidated_not_loadable() {
    let host = Host::detect();
    let m = manifest_for(aarch64_linux_gnu(&[]));
    let q = QemuUserExecutor::new("/nonexistent/qemu-aarch64-static", None);
    match check_loadable_with(&m, &host, &q).unwrap() {
        LoadStatus::Unvalidated { reason } => assert!(reason.contains("not found")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn gate_rvm_is_unvalidated_specification_required() {
    let m = manifest_for(rvm());
    assert_eq!(
        check_loadable(&m, &Host::detect()).unwrap(),
        LoadStatus::Unvalidated {
            reason: "specification required".into()
        }
    );
    let u = UnavailableExecutor::new("specification required");
    assert!(matches!(
        check_loadable_with(&m, &Host::detect(), &u).unwrap(),
        LoadStatus::Unvalidated { .. }
    ));
}

#[test]
fn gate_foreign_abi_mismatch_blocks_when_executor_present() {
    // Use `node` as a present wasi executor with a wrong declared ABI.
    let host = Host::detect();
    let mut t = wasm32_wasip1();
    t.abi = "wasi-preview2".into();
    let m = manifest_for(t);
    let n = rune_target::exec::NodeWasiExecutor::default();
    if n.availability().is_ok() {
        let e = check_loadable_with(&m, &host, &n).unwrap_err();
        assert!(matches!(e, LoadError::AbiMismatch { .. }));
    } else {
        eprintln!("SKIP: node not installed");
    }
}
