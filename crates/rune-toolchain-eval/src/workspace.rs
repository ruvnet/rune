//! Copy-on-write candidate workspace (ADR 007).
//!
//! This is Rune's own filesystem fork. It is NOT AgenticOW (a vector-memory
//! store) and NOT a security sandbox: a fork only gives the candidate its own
//! tree to write into. Containment layers live in [`crate::runner`] and are
//! reported separately.

use rune_core::{file_digest, hex_sha256, tree_digest, Error};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

pub const MANIFEST: &str = ".rune-owned.json";
/// Never copied: build output, VCS state and Rune's own state.
const SKIP: &[&str] = &["target", ".git", ".rune"];
const CREATOR: &str = "rune-eval";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedEntry {
    /// Path relative to the fork root, `/`-separated.
    pub path: String,
    pub sha256: String,
}

/// Ownership manifest: every generated path and its digest. Contains no
/// timestamps, so forking the same source twice yields identical bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub creator: String,
    pub source_tree_digest: String,
    pub files: Vec<OwnedEntry>,
    /// Symlinks in the source that were deliberately not copied.
    pub skipped_symlinks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyMode {
    /// Every file was cloned with a reflink (true copy-on-write).
    Reflink,
    /// At least one file was physically copied (the filesystem has no reflink).
    PlainCopy,
}

#[derive(Debug)]
pub struct CowWorkspace {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub mode: CopyMode,
}

fn pol(msg: impl Into<String>) -> Error {
    Error::Policy(msg.into())
}

fn rel_string(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Resolve `rel` under `root` without trusting it: absolute paths, `..`,
/// prefixes and any existing symlink component are rejected.
pub fn resolve_inside(root: &Path, rel: &Path) -> Result<PathBuf, Error> {
    let mut cur = root.to_path_buf();
    for comp in rel.components() {
        match comp {
            Component::Normal(c) => cur.push(c),
            Component::CurDir => {}
            _ => return Err(pol(format!("path escapes root: {}", rel.display()))),
        }
        if let Ok(md) = fs::symlink_metadata(&cur) {
            if md.file_type().is_symlink() {
                return Err(pol(format!("symlink in path: {}", cur.display())));
            }
        }
    }
    Ok(cur)
}

fn is_within(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

/// Canonicalize the nearest existing ancestor and re-append the rest.
fn canonical_lenient(p: &Path) -> Result<PathBuf, Error> {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::io(p, e))?
            .join(p)
    };
    let mut tail = Vec::new();
    let mut cur = abs.as_path();
    loop {
        match fs::canonicalize(cur) {
            Ok(c) => {
                let mut out = c;
                for t in tail.iter().rev() {
                    out.push(t);
                }
                return Ok(out);
            }
            Err(_) => {
                let (Some(name), Some(parent)) = (cur.file_name(), cur.parent()) else {
                    return Ok(abs);
                };
                tail.push(name.to_os_string());
                cur = parent;
            }
        }
    }
}

struct Copier {
    reflink_ok: bool,
    all_reflinked: bool,
}

impl Copier {
    fn copy(&mut self, from: &Path, to: &Path) -> Result<(), Error> {
        if self.reflink_ok {
            let ok = Command::new("cp")
                .args(["--reflink=always", "--preserve=mode"])
                .arg(from)
                .arg(to)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
            self.reflink_ok = false;
            let _ = fs::remove_file(to);
        }
        self.all_reflinked = false;
        fs::copy(from, to).map_err(|e| Error::io(to, e))?;
        Ok(())
    }
}

fn walk(
    src: &Path,
    dst: &Path,
    rel: &Path,
    copier: &mut Copier,
    files: &mut Vec<OwnedEntry>,
    skipped: &mut Vec<String>,
) -> Result<(), Error> {
    let mut entries: Vec<_> = fs::read_dir(src)
        .map_err(|e| Error::io(src, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| Error::io(src, e))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let rel_child = rel.join(&name);
        let ft = entry.file_type().map_err(|e| Error::io(entry.path(), e))?;
        if ft.is_symlink() {
            skipped.push(rel_string(&rel_child));
        } else if ft.is_dir() {
            if SKIP.iter().any(|s| name == *s) {
                continue;
            }
            let d = dst.join(&name);
            fs::create_dir_all(&d).map_err(|e| Error::io(&d, e))?;
            walk(&entry.path(), &d, &rel_child, copier, files, skipped)?;
        } else if ft.is_file() {
            if name == MANIFEST {
                continue;
            }
            let d = dst.join(&name);
            copier.copy(&entry.path(), &d)?;
            files.push(OwnedEntry {
                path: rel_string(&rel_child),
                sha256: file_digest(&d)?,
            });
        } // sockets, devices, fifos are ignored
    }
    Ok(())
}

impl CowWorkspace {
    /// Fork `src` into the new directory `dest`. Symlinks are never followed
    /// or copied; `target/`, `.git/` and `.rune/` are skipped.
    pub fn fork(src: &Path, dest: &Path) -> Result<Self, Error> {
        let md = fs::symlink_metadata(src).map_err(|e| Error::io(src, e))?;
        if !md.is_dir() {
            return Err(pol(format!(
                "fork source is not a directory: {}",
                src.display()
            )));
        }
        let (csrc, cdest) = (canonical_lenient(src)?, canonical_lenient(dest)?);
        if is_within(&cdest, &csrc) || is_within(&csrc, &cdest) {
            return Err(pol("fork destination and source must not nest"));
        }
        if dest.exists()
            && fs::read_dir(dest)
                .map_err(|e| Error::io(dest, e))?
                .next()
                .is_some()
        {
            return Err(pol(format!(
                "fork destination not empty: {}",
                dest.display()
            )));
        }
        fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
        let mut copier = Copier {
            reflink_ok: true,
            all_reflinked: true,
        };
        let (mut files, mut skipped) = (Vec::new(), Vec::new());
        walk(
            src,
            dest,
            Path::new(""),
            &mut copier,
            &mut files,
            &mut skipped,
        )?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        skipped.sort();
        let manifest = Manifest {
            schema: 1,
            creator: CREATOR.into(),
            source_tree_digest: tree_digest(src)?,
            files,
            skipped_symlinks: skipped,
        };
        let ws = CowWorkspace {
            root: dest.to_path_buf(),
            manifest,
            mode: if copier.all_reflinked {
                CopyMode::Reflink
            } else {
                CopyMode::PlainCopy
            },
        };
        ws.write_manifest()?;
        Ok(ws)
    }

    fn write_manifest(&self) -> Result<(), Error> {
        let p = self.root.join(MANIFEST);
        let mut bytes = serde_json::to_vec_pretty(&self.manifest)?;
        bytes.push(b'\n');
        fs::write(&p, bytes).map_err(|e| Error::io(p, e))
    }

    /// Layer name for reports; honest about whether copy-on-write happened.
    pub fn layer_name(&self) -> &'static str {
        match self.mode {
            CopyMode::Reflink => "cow-reflink",
            CopyMode::PlainCopy => "plain-copy(no-reflink)",
        }
    }

    /// Install a generated file (e.g. a holdout) at `rel`, recording ownership.
    pub fn install_file(&mut self, rel: &Path, data: &[u8]) -> Result<(), Error> {
        let dest = resolve_inside(&self.root, rel)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            // re-check after creating directories
            resolve_inside(&self.root, rel)?;
        }
        if rel.file_name().is_some_and(|n| n == MANIFEST) {
            return Err(pol("cannot overwrite the ownership manifest"));
        }
        fs::write(&dest, data).map_err(|e| Error::io(&dest, e))?;
        let path = rel_string(rel);
        let sha256 = hex_sha256(data);
        match self.manifest.files.iter_mut().find(|f| f.path == path) {
            Some(f) => f.sha256 = sha256,
            None => {
                self.manifest.files.push(OwnedEntry { path, sha256 });
                self.manifest.files.sort_by(|a, b| a.path.cmp(&b.path));
            }
        }
        self.write_manifest()
    }

    /// Files whose digest no longer matches the manifest, or that are missing.
    /// Build output under `target/` is generated state and not listed.
    pub fn drifted(&self) -> Result<Vec<String>, Error> {
        let mut out = Vec::new();
        for f in &self.manifest.files {
            let p = resolve_inside(&self.root, Path::new(&f.path))?;
            match file_digest(&p) {
                Ok(d) if d == f.sha256 => {}
                _ => out.push(f.path.clone()),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_escapes() {
        let d = tempfile::tempdir().unwrap();
        assert!(resolve_inside(d.path(), Path::new("../x")).is_err());
        assert!(resolve_inside(d.path(), Path::new("/etc/passwd")).is_err());
        assert!(resolve_inside(d.path(), Path::new("a/../../x")).is_err());
        assert!(resolve_inside(d.path(), Path::new("a/b.rs")).is_ok());
    }

    #[test]
    fn resolve_rejects_symlink_component() {
        let d = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(out.path(), d.path().join("link")).unwrap();
        assert!(resolve_inside(d.path(), Path::new("link/evil")).is_err());
    }
}
