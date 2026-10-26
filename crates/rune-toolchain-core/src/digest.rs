use crate::Error;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Directories never part of the source identity.
const SKIP: &[&str] = &["target", ".git", ".rune", "node_modules"];

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn file_digest(path: &Path) -> Result<String, Error> {
    Ok(hex_sha256(&fs::read(path).map_err(|e| Error::io(path, e))?))
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Error> {
    for entry in fs::read_dir(dir).map_err(|e| Error::io(dir, e))? {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| Error::io(&path, e))?;
        if ft.is_dir() {
            if !SKIP.iter().any(|s| entry.file_name() == *s) {
                collect(&path, out)?;
            }
        } else if ft.is_file() {
            out.push(path);
        } // symlinks are ignored: they cannot escape the tree digest
    }
    Ok(())
}

/// Digest of every regular file under `root` (relative path + content),
/// in sorted order, excluding build output and VCS directories.
pub fn tree_digest(root: &Path) -> Result<String, Error> {
    let mut files = Vec::new();
    collect(root, &mut files)?;
    files.sort();
    let mut h = Sha256::new();
    for f in files {
        let rel = f
            .strip_prefix(root)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        h.update((rel.len() as u64).to_le_bytes());
        h.update(rel.as_bytes());
        let data = fs::read(&f).map_err(|e| Error::io(&f, e))?;
        h.update((data.len() as u64).to_le_bytes());
        h.update(&data);
    }
    Ok(hex(&h.finalize()))
}
