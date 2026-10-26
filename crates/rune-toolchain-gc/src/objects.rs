//! Content-addressed store for GENERATED objects under `.rune/objects/<sha256>`
//! plus registration of candidate workspaces (COW forks created elsewhere).

use crate::manifest::{Entry, Kind, Manifest, Retention};
use crate::paths::{dir_size, is_hex64, is_symlink, secure_child, secure_dir, CANDIDATE_PREFIX};
use crate::{Error, Result};
use rune_core::hex_sha256;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CANDIDATE_MARKER: &str = ".rune-candidate";

fn stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn check_plan_id(plan_id: &str) -> Result<()> {
    if is_hex64(plan_id) {
        Ok(())
    } else {
        Err(Error::InvalidKey(plan_id.to_owned()))
    }
}

/// Store `bytes` and record ownership. Returns the object digest. Re-storing
/// the same content keeps the first owner and raises retention to the maximum.
pub fn put_object(
    workspace: &Path,
    kind: Kind,
    plan_id: &str,
    retention: Option<Retention>,
    bytes: &[u8],
) -> Result<String> {
    check_plan_id(plan_id)?;
    if kind == Kind::CandidateWorkspace {
        return Err(Error::Refused(
            "candidate workspaces are registered, not stored".into(),
        ));
    }
    let key = hex_sha256(bytes);
    let dir = secure_dir(workspace, &["objects"], true)?.expect("created");
    let path = secure_child(&dir, &key)?;
    if !path.exists() {
        let tmp = secure_child(&dir, &format!("{key}.tmp"))?;
        fs::write(&tmp, bytes).map_err(|e| Error::io(&tmp, e))?;
        fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))?;
    }
    let ret = retention.unwrap_or_else(|| kind.default_retention());
    Manifest::update(workspace, |m| {
        let seq = m.alloc_seq();
        m.entries
            .entry(key.clone())
            .and_modify(|e| e.retention = e.retention.max(ret))
            .or_insert(Entry {
                kind,
                plan_id: plan_id.to_owned(),
                seq,
                size: bytes.len() as u64,
                retention: ret,
                created_unix: stamp(),
                path: None,
            });
        Ok(())
    })?;
    Ok(key)
}

/// Absolute path of a stored object (validated, symlink-free).
pub fn object_path(workspace: &Path, key: &str) -> Result<PathBuf> {
    crate::paths::validate_key(key)?;
    let dir = secure_dir(workspace, &["objects"], false)?
        .ok_or_else(|| Error::NotFound("objects directory".into()))?;
    secure_child(&dir, key)
}

pub fn candidate_key(canonical: &Path) -> String {
    format!(
        "{CANDIDATE_PREFIX}{}",
        hex_sha256(canonical.to_string_lossy().as_bytes())
    )
}

/// Take ownership of a candidate workspace directory (for example an
/// evaluator COW fork under /data/scratch) so it can later be quarantined and
/// collected through the same path as any other generated object. The path
/// must be a real directory (not a symlink), outside the workspace, and is
/// stamped with an ownership marker that quarantine/purge re-verify.
pub fn register_candidate_workspace(
    workspace: &Path,
    path: &Path,
    plan_id: &str,
) -> Result<String> {
    check_plan_id(plan_id)?;
    match is_symlink(path)? {
        Some(false) => {}
        Some(true) => return Err(Error::Escape(path.display().to_string())),
        None => return Err(Error::NotFound(path.display().to_string())),
    }
    let canon = fs::canonicalize(path).map_err(|e| Error::io(path, e))?;
    let ws = fs::canonicalize(workspace).map_err(|e| Error::io(workspace, e))?;
    if !canon.is_dir() || canon.parent().is_none() {
        return Err(Error::Refused(format!(
            "{} is not a directory",
            canon.display()
        )));
    }
    if canon.starts_with(&ws) || ws.starts_with(&canon) {
        return Err(Error::Refused(
            "candidate workspace must be disjoint from the source workspace".into(),
        ));
    }
    if canon.file_name().and_then(|n| n.to_str()).is_none() {
        return Err(Error::Refused("candidate name must be utf-8".into()));
    }
    let key = candidate_key(&canon);
    let marker = canon.join(CANDIDATE_MARKER);
    fs::write(&marker, &key).map_err(|e| Error::io(&marker, e))?;
    let size = dir_size(&canon);
    Manifest::update(workspace, |m| {
        let seq = m.alloc_seq();
        m.entries.entry(key.clone()).or_insert(Entry {
            kind: Kind::CandidateWorkspace,
            plan_id: plan_id.to_owned(),
            seq,
            size,
            retention: Retention::Transient,
            created_unix: stamp(),
            path: Some(canon.to_string_lossy().into_owned()),
        });
        Ok(())
    })?;
    Ok(key)
}
