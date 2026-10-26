//! Envelope verification: manifest, file digests, no stray files, signature.

use crate::envelope::{safe_relative, FileEntry, Manifest, MANIFEST_FILE, SCHEMA};
use crate::sign::{self, SigFile, SIG_FILE};
use crate::Error;
use ed25519_dalek::VerifyingKey;
use rune_core::hex_sha256;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    /// No `artifact.sig` and none was required.
    Absent,
    /// Verified against a key the verifier already trusts.
    Trusted,
    /// Verified only against the key embedded in the signature file: this
    /// proves integrity since signing, not who signed.
    SelfAsserted,
}

#[derive(Debug, Default)]
pub struct VerifyOptions {
    /// Key the verifier trusts (from its own configuration, never the envelope).
    pub trusted_key: Option<VerifyingKey>,
    /// Fail when there is no signature.
    pub require_signature: bool,
}

#[derive(Debug)]
pub struct VerifyReport {
    pub manifest: Manifest,
    pub signature: SignatureStatus,
    pub files_checked: usize,
}

fn fail<T>(msg: impl Into<String>) -> Result<T, Error> {
    Err(Error::Verify(msg.into()))
}

fn check_entry(dir: &Path, e: &FileEntry, prefix: &str) -> Result<(), Error> {
    safe_relative(&e.path)?;
    if !e.path.starts_with(prefix) {
        return fail(format!("{} is not under {prefix}", e.path));
    }
    let path = dir.join(&e.path);
    let meta = fs::symlink_metadata(&path).map_err(|err| Error::io(&path, err))?;
    if !meta.is_file() {
        return fail(format!("{} is not a regular file", e.path));
    }
    let data = fs::read(&path).map_err(|err| Error::io(&path, err))?;
    if data.len() as u64 != e.size || hex_sha256(&data) != e.sha256 {
        return fail(format!("{} does not match its recorded digest", e.path));
    }
    Ok(())
}

fn walk(dir: &Path, base: &Path, out: &mut BTreeSet<String>) -> Result<(), Error> {
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, e))? {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| Error::io(&path, e))?;
        if ft.is_dir() {
            walk(&path, base, out)?;
        } else {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

pub fn verify_envelope(dir: &Path, opts: &VerifyOptions) -> Result<VerifyReport, Error> {
    let mpath = dir.join(MANIFEST_FILE);
    let bytes = fs::read(&mpath).map_err(|e| Error::io(&mpath, e))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Verify(format!("manifest unreadable: {e}")))?;
    if manifest.schema != SCHEMA {
        return fail(format!("unsupported schema {}", manifest.schema));
    }
    if manifest.target_digest != manifest.target.digest() {
        return fail("target descriptor does not match target_digest");
    }

    let spath = dir.join(SIG_FILE);
    let signature = if spath.exists() {
        let raw = fs::read(&spath).map_err(|e| Error::io(&spath, e))?;
        let sig: SigFile = serde_json::from_slice(&raw)
            .map_err(|e| Error::Verify(format!("signature unreadable: {e}")))?;
        let embedded = sign::embedded_key(&sig)?;
        match &opts.trusted_key {
            Some(trusted) => {
                if trusted.as_bytes() != embedded.as_bytes() {
                    return fail("signature was made by a key that is not trusted");
                }
                sign::verify_manifest(trusted, &sig, &bytes)?;
                SignatureStatus::Trusted
            }
            None => {
                sign::verify_manifest(&embedded, &sig, &bytes)?;
                SignatureStatus::SelfAsserted
            }
        }
    } else if opts.require_signature || opts.trusted_key.is_some() {
        return fail("signature required but artifact.sig is missing");
    } else {
        SignatureStatus::Absent
    };

    let mut listed = BTreeSet::new();
    for e in &manifest.executables {
        check_entry(dir, e, "bin/")?;
        listed.insert(e.path.clone());
    }
    for e in &manifest.files {
        check_entry(dir, e, "files/")?;
        listed.insert(e.path.clone());
    }
    let mut present = BTreeSet::new();
    walk(dir, dir, &mut present)?;
    for p in &present {
        if p != MANIFEST_FILE && p != SIG_FILE && !listed.contains(p) {
            return fail(format!("unlisted file {p}"));
        }
    }
    Ok(VerifyReport {
        files_checked: listed.len(),
        manifest,
        signature,
    })
}
