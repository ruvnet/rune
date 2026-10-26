//! The artifact envelope: an ordinary development evidence directory.
//!
//! ```text
//! <dir>/artifact.json   manifest binding plan, source, lock, toolchain, target,
//!                       tests, contract/policy digests and file digests
//! <dir>/artifact.sig    optional detached ed25519 signature (see `sign`)
//! <dir>/bin/<name>      executables, each listed with its sha256
//! <dir>/files/<name>    other evidence files
//! ```
//!
//! This is not an RVF and not a ZIP. Signatures prove provenance and integrity,
//! never correctness.

use crate::sign::{self, SIG_FILE};
use crate::Error;
use rune_core::target::{Evidence, TargetDescriptor};
use rune_core::{hex_sha256, Plan};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "rune.artifact/1";
pub const MANIFEST_FILE: &str = "artifact.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative, `/`-separated, inside the envelope directory.
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSummary {
    pub command: String,
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub plan_id: String,
    pub source_digest: String,
    pub lock_digest: String,
    pub toolchain: String,
    pub target_digest: String,
    pub target: TargetDescriptor,
    /// Strongest evidence attained for the target. Gates performance claims.
    pub evidence: Evidence,
    pub tests: TestSummary,
    pub contract_digest: String,
    pub policy_digest: String,
    pub executables: Vec<FileEntry>,
    pub files: Vec<FileEntry>,
}

/// Everything the envelope binds. Executables and files are `(name, source)`.
pub struct EnvelopeInputs<'a> {
    pub plan: &'a Plan,
    pub target: &'a TargetDescriptor,
    pub evidence: Evidence,
    pub tests: TestSummary,
    pub contract_digest: String,
    pub executables: Vec<(String, PathBuf)>,
    pub files: Vec<(String, PathBuf)>,
}

/// Reject absolute paths, `..`, empty components and backslashes.
pub fn safe_relative(p: &str) -> Result<(), Error> {
    let bad = p.is_empty()
        || p.starts_with('/')
        || p.contains('\\')
        || p.contains('\0')
        || p.split('/').any(|c| c.is_empty() || c == "." || c == "..");
    if bad {
        Err(Error::UnsafePath(p.to_owned()))
    } else {
        Ok(())
    }
}

fn copy_in(
    dir: &Path,
    prefix: &str,
    name: &str,
    src: &Path,
    exec: bool,
) -> Result<FileEntry, Error> {
    safe_relative(name)?;
    let rel = format!("{prefix}/{name}");
    let dest = dir.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let data = fs::read(src).map_err(|e| Error::io(src, e))?;
    fs::write(&dest, &data).map_err(|e| Error::io(&dest, e))?;
    #[cfg(unix)]
    if exec {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::io(&dest, e))?;
    }
    #[cfg(not(unix))]
    let _ = exec;
    Ok(FileEntry {
        path: rel,
        sha256: hex_sha256(&data),
        size: data.len() as u64,
    })
}

/// Write the envelope into `dir` (which must be absent or empty). With
/// `signing_key` (a path to a 32-byte key file) a detached signature is added.
pub fn write_envelope(
    dir: &Path,
    inputs: &EnvelopeInputs<'_>,
    signing_key: Option<&Path>,
) -> Result<Manifest, Error> {
    let plan = inputs.plan;
    if plan.target != inputs.target.triple {
        return Err(Error::Verify(format!(
            "plan targets {} but descriptor is {}",
            plan.target, inputs.target.triple
        )));
    }
    if dir.exists()
        && fs::read_dir(dir)
            .map_err(|e| Error::io(dir, e))?
            .next()
            .is_some()
    {
        return Err(Error::Verify(format!("{} is not empty", dir.display())));
    }
    // Load the key first so a bad key path leaves no half-written envelope.
    let key = signing_key.map(sign::load_signing_key).transpose()?;
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let mut executables = Vec::new();
    for (name, src) in &inputs.executables {
        executables.push(copy_in(dir, "bin", name, src, true)?);
    }
    let mut files = Vec::new();
    for (name, src) in &inputs.files {
        files.push(copy_in(dir, "files", name, src, false)?);
    }
    executables.sort_by(|a, b| a.path.cmp(&b.path));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        schema: SCHEMA.into(),
        plan_id: plan.id(),
        source_digest: plan.source_digest.clone(),
        lock_digest: plan.lock_digest.clone(),
        toolchain: plan.toolchain.clone(),
        target_digest: inputs.target.digest(),
        target: inputs.target.clone(),
        evidence: inputs.evidence,
        tests: inputs.tests.clone(),
        contract_digest: inputs.contract_digest.clone(),
        policy_digest: hex_sha256(&serde_json::to_vec(&plan.policy)?),
        executables,
        files,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    let mpath = dir.join(MANIFEST_FILE);
    fs::write(&mpath, &bytes).map_err(|e| Error::io(&mpath, e))?;
    if let Some(key) = key {
        let sig = sign::sign_manifest(&key, &bytes);
        let spath = dir.join(SIG_FILE);
        fs::write(&spath, serde_json::to_vec_pretty(&sig)?).map_err(|e| Error::io(&spath, e))?;
    }
    Ok(manifest)
}
