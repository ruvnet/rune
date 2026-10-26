//! Shared filesystem rules: confinement, checked read, atomic write.

use rune_core::edit::EditRequest;
use rune_core::{hex_sha256, Error};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn reject(msg: impl Into<String>) -> Error {
    Error::Policy(msg.into())
}

/// Resolve `rel` under `root`. Rejects absolute paths, `..`, and anything whose
/// canonical form (symlinks resolved) leaves the canonical root. The file must exist.
pub fn confine(root: &Path, rel: &Path) -> Result<PathBuf, Error> {
    if rel.as_os_str().is_empty() || rel.is_absolute() {
        return Err(reject("edit path must be relative and non-empty"));
    }
    for c in rel.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err(reject("edit path may not contain `..` or a root/prefix")),
        }
    }
    let root_c = root.canonicalize().map_err(|e| Error::io(root, e))?;
    let full = root_c.join(rel);
    let canon = full.canonicalize().map_err(|e| Error::io(&full, e))?;
    if !canon.starts_with(&root_c) {
        return Err(reject("edit path escapes the root (symlink)"));
    }
    Ok(canon)
}

pub(crate) struct Loaded {
    pub path: PathBuf,
    pub text: String,
    pub sha256: String,
}

/// Confine, bound, read and hash-check. Nothing is written.
pub(crate) fn load(req: &EditRequest) -> Result<Loaded, Error> {
    let path = confine(&req.root, &req.path)?;
    let meta = fs::metadata(&path).map_err(|e| Error::io(&path, e))?;
    if !meta.is_file() {
        return Err(reject("edit target is not a regular file"));
    }
    if meta.len() > req.max_bytes {
        return Err(reject(format!(
            "file is {} bytes, over the {} byte bound",
            meta.len(),
            req.max_bytes
        )));
    }
    let bytes = fs::read(&path).map_err(|e| Error::io(&path, e))?;
    if bytes.len() as u64 > req.max_bytes {
        return Err(reject("file grew past the size bound"));
    }
    let sha256 = hex_sha256(&bytes);
    if !sha256.eq_ignore_ascii_case(&req.expected_sha256) {
        return Err(Error::Stale {
            field: "expected_sha256",
            planned: req.expected_sha256.clone(),
            actual: sha256,
        });
    }
    let text = String::from_utf8(bytes).map_err(|_| reject("edit target is not valid UTF-8"))?;
    Ok(Loaded { path, text, sha256 })
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Write `new` over `path` atomically, only if the file still hashes to `before_sha256`.
pub(crate) fn write_atomic(path: &Path, before_sha256: &str, new: &[u8]) -> Result<(), Error> {
    let current = fs::read(path).map_err(|e| Error::io(path, e))?;
    if hex_sha256(&current) != before_sha256 {
        return Err(reject("file changed during the edit; nothing written"));
    }
    let dir = path.parent().ok_or_else(|| reject("path has no parent"))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!(
        ".{}.rune-tmp-{}-{}",
        name.unwrap_or_default(),
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(new)?;
        f.sync_all()?;
        fs::set_permissions(&tmp, fs::metadata(path)?.permissions())?;
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(Error::io(path, e));
    }
    Ok(())
}
