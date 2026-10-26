//! Safe path resolution. Every filesystem path the collector touches is built
//! from a validated key or name joined onto a directory that was walked
//! component by component with `symlink_metadata`, so a symlink anywhere on
//! the way (or `..`, or an absolute path) is refused rather than followed.

use crate::{Error, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const CANDIDATE_PREFIX: &str = "cand-";

pub fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A key is a lowercase sha256 hex digest, or `cand-<sha256 hex>` for
/// registered candidate workspaces. Nothing else ever becomes a path.
pub fn validate_key(key: &str) -> Result<()> {
    let ok = is_hex64(key) || key.strip_prefix(CANDIDATE_PREFIX).is_some_and(is_hex64);
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidKey(key.to_owned()))
    }
}

/// Lease job names and similar single-component names.
pub fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidKey(name.to_owned()))
    }
}

pub fn is_symlink(path: &Path) -> Result<Option<bool>> {
    match fs::symlink_metadata(path) {
        Ok(m) => Ok(Some(m.file_type().is_symlink())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// Resolve `<workspace>/.rune/<sub...>` refusing symlinks at every level.
/// Returns `None` when a level does not exist and `create` is false.
pub fn secure_dir(workspace: &Path, sub: &[&str], create: bool) -> Result<Option<PathBuf>> {
    let mut cur = fs::canonicalize(workspace).map_err(|e| Error::io(workspace, e))?;
    for comp in std::iter::once(".rune").chain(sub.iter().copied()) {
        if comp != ".rune" {
            validate_component(comp)?;
        }
        cur = cur.join(comp);
        match is_symlink(&cur)? {
            Some(true) => return Err(Error::Escape(cur.display().to_string())),
            Some(false) => {
                if !cur.is_dir() {
                    return Err(Error::Escape(format!(
                        "{} is not a directory",
                        cur.display()
                    )));
                }
            }
            None if create => fs::create_dir(&cur).map_err(|e| Error::io(&cur, e))?,
            None => return Ok(None),
        }
    }
    Ok(Some(cur))
}

fn validate_component(c: &str) -> Result<()> {
    let mut it = Path::new(c).components();
    match (it.next(), it.next()) {
        (Some(Component::Normal(n)), None) if n == c => Ok(()),
        _ => Err(Error::Escape(c.to_owned())),
    }
}

/// Join a single validated name onto an already-secured directory and verify
/// the result is not a symlink and (if it exists) stays inside `dir`.
pub fn secure_child(dir: &Path, name: &str) -> Result<PathBuf> {
    validate_component(name)?;
    let child = dir.join(name);
    if is_symlink(&child)? == Some(true) {
        return Err(Error::Escape(child.display().to_string()));
    }
    if child.exists() {
        let real = fs::canonicalize(&child).map_err(|e| Error::io(&child, e))?;
        let root = fs::canonicalize(dir).map_err(|e| Error::io(dir, e))?;
        if !real.starts_with(&root) {
            return Err(Error::Escape(child.display().to_string()));
        }
    }
    Ok(child)
}

/// Directory size in bytes, never following symlinks.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(md) = fs::symlink_metadata(path) else {
        return 0;
    };
    if md.is_dir() {
        fs::read_dir(path)
            .map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum())
            .unwrap_or(0)
    } else if md.is_file() {
        md.len()
    } else {
        0
    }
}
