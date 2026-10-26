mod common;
use common::*;
use std::fs;
use std::path::{Path, PathBuf};

fn host_x86() -> bool {
    cfg!(all(target_arch = "x86_64", target_os = "linux"))
}

fn planned() -> tempfile::TempDir {
    let d = fixture("targets/portable");
    ok(&rune(d.path(), &["build", "--freeze"]));
    d
}

fn envelope_dir(ws: &Path) -> PathBuf {
    ws.join(".rune/artifacts").join(&history(ws)[0])
}

fn key_file(dir: &Path) -> PathBuf {
    let p = dir.join("signing.key");
    fs::write(&p, [7u8; 32]).unwrap();
    p
}

#[test]
fn unsigned_round_trip_and_tamper_detection() {
    if !host_x86() {
        return;
    }
    let d = planned();
    let s = ok(&rune(d.path(), &["package", "--target", "x86_64"]));
    assert!(s.contains("evidence directory, not RVF"), "{s}");
    assert!(
        s.contains("provenance and integrity, not correctness"),
        "{s}"
    );
    assert!(s.contains("evidence native"), "{s}");
    let env = envelope_dir(d.path());
    let v = ok(&rune(d.path(), &["verify-artifact", env.to_str().unwrap()]));
    assert!(
        v.contains("envelope ok") && v.contains("signature: absent"),
        "{v}"
    );
    assert!(v.contains("load gate: loadable"), "{v}");
    let ex = ok(&rune(d.path(), &["explain"]));
    assert!(ex.contains("evidence class: native"), "{ex}");
    assert!(ex.contains("envelope   verified"), "{ex}");
    // Tamper with the executable.
    let bin = fs::read_dir(env.join("bin"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut bytes = fs::read(&bin).unwrap();
    bytes.push(0);
    fs::write(&bin, bytes).unwrap();
    let e = fails(&rune(d.path(), &["verify-artifact", env.to_str().unwrap()]));
    assert!(e.contains("digest"), "{e}");
    assert!(ok(&rune(d.path(), &["explain"])).contains("FAILED verification"));
}

#[test]
fn signed_envelope_verifies_against_a_trusted_key_only() {
    if !host_x86() {
        return;
    }
    let d = planned();
    // Keys live outside the tree so the source digest stays fresh.
    let keys = tempfile::tempdir().unwrap();
    let key = key_file(keys.path());
    ok(&rune(
        d.path(),
        &[
            "package",
            "--target",
            "x86_64",
            "--sign-key",
            key.to_str().unwrap(),
        ],
    ));
    let env = envelope_dir(d.path());
    let v = ok(&rune(d.path(), &["verify-artifact", env.to_str().unwrap()]));
    assert!(v.contains("embedded key only"), "{v}");
    // Trusted key: taken from the signature file the signer published.
    let sig: serde_json::Value =
        serde_json::from_slice(&fs::read(env.join("artifact.sig")).unwrap()).unwrap();
    let good = keys.path().join("trusted.pub");
    fs::write(&good, sig["public_key"].as_str().unwrap()).unwrap();
    let t = ok(&rune(
        d.path(),
        &[
            "verify-artifact",
            env.to_str().unwrap(),
            "--trusted-pubkey",
            good.to_str().unwrap(),
        ],
    ));
    assert!(t.contains("trusted key"), "{t}");
    let other = keys.path().join("other.pub");
    fs::write(&other, "11".repeat(32)).unwrap();
    let e = fails(&rune(
        d.path(),
        &[
            "verify-artifact",
            env.to_str().unwrap(),
            "--trusted-pubkey",
            other.to_str().unwrap(),
        ],
    ));
    assert!(e.contains("not trusted") || e.contains("invalid"), "{e}");
    // Tampering the manifest breaks the signature.
    let m = env.join("artifact.json");
    let text = fs::read_to_string(&m)
        .unwrap()
        .replace("\"failed\": 0", "\"failed\": 5");
    fs::write(&m, text).unwrap();
    fails(&rune(d.path(), &["verify-artifact", env.to_str().unwrap()]));
}

fn rewrite_target(env: &Path, edit: impl FnOnce(&mut rune_core::target::TargetDescriptor)) {
    let p = env.join("artifact.json");
    let mut m: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    let mut t: rune_core::target::TargetDescriptor =
        serde_json::from_value(m["target"].clone()).unwrap();
    edit(&mut t);
    m["target_digest"] = t.digest().into();
    m["target"] = serde_json::to_value(&t).unwrap();
    fs::write(&p, serde_json::to_vec_pretty(&m).unwrap()).unwrap();
}

#[test]
fn load_gate_blocks_abi_and_cpu_mismatch_and_reports_missing_executor() {
    if !host_x86() {
        return;
    }
    let d = planned();
    ok(&rune(d.path(), &["package", "--target", "x86_64"]));
    let env = envelope_dir(d.path());
    let arg = env.to_str().unwrap();

    rewrite_target(&env, |t| t.abi = "aapcs64".into());
    assert!(fails(&rune(d.path(), &["verify-artifact", arg])).contains("ABI mismatch"));

    rewrite_target(&env, |t| {
        t.abi = "sysv-x86_64".into();
        t.cpu_features = vec!["no-such-feature".into()];
    });
    assert!(fails(&rune(d.path(), &["verify-artifact", arg])).contains("cpu features"));

    rewrite_target(&env, |t| *t = rune_target::descriptors::rvm());
    let s = ok(&rune(d.path(), &["verify-artifact", arg]));
    assert!(
        s.contains("load gate: unvalidated") && s.contains("performance claims: refused"),
        "{s}"
    );
}

#[test]
fn package_for_a_target_the_plan_does_not_cover_is_refused() {
    let d = planned();
    let e = fails(&rune(d.path(), &["package", "--target", "wasm32"]));
    assert!(e.contains("current plan targets"), "{e}");
    assert!(!d.path().join(".rune/artifacts").exists());
}

#[test]
fn packaging_twice_does_not_overwrite_an_existing_envelope() {
    if !host_x86() {
        return;
    }
    let d = planned();
    ok(&rune(
        d.path(),
        &["package", "--target", "x86_64", "--no-run"],
    ));
    let again = fails(&rune(d.path(), &["package", "--target", "x86_64"]));
    assert!(again.contains("not empty"), "{again}");
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(envelope_dir(d.path()).join("artifact.json")).unwrap())
            .unwrap();
    assert_eq!(m["evidence"], "compile_only");
}
