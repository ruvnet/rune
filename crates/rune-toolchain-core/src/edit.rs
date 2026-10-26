//! Local edit contract (ADR 003). An applier turns an explicit snippet into
//! new file text; it never decides correctness.

use crate::Error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditRequest {
    /// Tree the edit is confined to.
    pub root: PathBuf,
    /// Path relative to `root`; absolute paths, `..` and symlink escapes are rejected.
    pub path: PathBuf,
    /// SHA-256 of the file the proposer saw; a mismatch rejects before editing.
    pub expected_sha256: String,
    /// Exact text to find (for exact replace) or the edit snippet (for Agent Booster).
    pub find: String,
    pub replace: String,
    pub language: String,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditOutcome {
    pub applier: String,
    pub before_sha256: String,
    pub after_sha256: String,
    /// Package-reported confidence, if any. Placement evidence, never proof.
    pub confidence: Option<f64>,
}

pub trait EditApplier {
    fn name(&self) -> &'static str;
    /// Apply the edit in place. Must reject (leaving the file untouched) on stale hash,
    /// oversize file, path escape, unsuccessful or empty result.
    fn apply(&self, req: &EditRequest) -> Result<EditOutcome, Error>;
}
