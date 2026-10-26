//! Harness integrity and holdout installation.

use crate::gates::read_tree;
use crate::workspace::CowWorkspace;
use rune_core::contract::EvalContract;
use rune_core::{hex_sha256, Error};
use std::collections::BTreeMap;
use std::path::Path;

const HARNESS_DIRS: &[&str] = &["tests/", "benches/", "src/bin/", "examples/"];

/// Protected harness files (tests, benches, bench binaries) must be identical
/// to the baseline's: a candidate may not rewrite the thing that judges it.
pub fn harness_tampering(base: &Path, cand: &Path, kernel_dir: &str) -> Result<Vec<String>, Error> {
    let k = kernel_dir.trim_matches('/').trim_start_matches("./");
    let prefix = if k.is_empty() || k == "." {
        String::new()
    } else {
        format!("{k}/")
    };
    let (bt, ct) = (read_tree(base)?, read_tree(cand)?);
    let protected = |p: &str| {
        p.strip_prefix(&prefix)
            .is_some_and(|r| HARNESS_DIRS.iter().any(|d| r.starts_with(d)))
    };
    let mut bad = Vec::new();
    for (p, b) in &bt.files {
        if protected(p) && ct.files.get(p).map(|c| hex_sha256(c)) != Some(hex_sha256(b)) {
            bad.push(format!("{p}: protected harness file modified or removed"));
        }
    }
    for p in ct.files.keys() {
        if protected(p) && !bt.files.contains_key(p) {
            bad.push(format!("{p}: protected harness file added"));
        }
    }
    Ok(bad)
}

pub fn install_holdout(
    ws: &mut CowWorkspace,
    c: &EvalContract,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), Error> {
    for (rel, data) in files {
        let dest = Path::new(&c.kernel_dir).join(&c.holdout_dest).join(rel);
        ws.install_file(&dest, data)?;
    }
    Ok(())
}
