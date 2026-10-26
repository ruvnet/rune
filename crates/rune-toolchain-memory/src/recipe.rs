//! Recipe records (ADR 004): descriptors plus references to evidence.

use crate::Error;
use rune_core::hex_sha256;
use serde::{Deserialize, Serialize};

/// What a query or record is matched on. Compiler id is a digest of
/// `rustc -vV`; policy digest binds the budget/permission policy.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Fingerprint {
    pub source_tags: Vec<String>,
    pub target_triple: String,
    pub cpu_features: Vec<String>,
    pub compiler_id: String,
    pub abi: String,
    pub workload_tags: Vec<String>,
    pub policy_digest: String,
}

/// Description of an edit pattern. Documentation only: it is never applied
/// automatically and carries no authority.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EditTemplate {
    pub find_pattern: String,
    pub replace_pattern: String,
    pub description: String,
}

/// Pointer to raw evidence plus its summary statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// Workspace-relative path of the raw sample file.
    pub sample_path: String,
    pub ci95_low: f64,
    pub ci95_high: f64,
    pub samples: u32,
    pub cost_micro_usd: u64,
}

/// Where a failure applies. `None` / empty means "not narrowed on this axis".
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FailureScope {
    pub target_triple: Option<String>,
    pub compiler_id: Option<String>,
    pub workload_tags: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub reason: String,
    pub scope: FailureScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    Ephemeral,
    Standard,
    Permanent,
}

/// Advisory label. It never grants approval; see [`crate::is_approved`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Proposed,
    Approved,
    Rejected,
}

/// Private records never leave their tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Private,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub parent: Option<String>,
    pub proposer: String,
    pub descriptor: String,
    pub fingerprint: Fingerprint,
    pub template: EditTemplate,
    pub baseline_hash: String,
    pub candidate_hash: String,
    pub evaluation_version: String,
    pub evidence: Vec<EvidenceRef>,
    pub failures: Vec<Failure>,
    pub retention: Retention,
    pub status: Status,
    pub tenant: String,
    pub visibility: Visibility,
}

impl Recipe {
    /// Content address over the identity fields. Evidence, failures and status
    /// accumulate over time and are excluded.
    pub fn compute_id(&self) -> String {
        let identity = (
            &self.parent,
            &self.proposer,
            &self.descriptor,
            &self.fingerprint,
            &self.template,
            &self.baseline_hash,
            &self.candidate_hash,
            &self.evaluation_version,
            &self.tenant,
        );
        hex_sha256(&serde_json::to_vec(&identity).expect("identity serializes"))
    }

    /// Set `id` from content and return the recipe.
    pub fn sealed(mut self) -> Self {
        self.id = self.compute_id();
        self
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.id != self.compute_id() {
            return Err(Error::Invalid(format!(
                "recipe id {} does not match content",
                self.id
            )));
        }
        for e in &self.evidence {
            check_rel_path(&e.sample_path)?;
        }
        Ok(())
    }
}

/// Evidence paths are workspace-relative: no absolute, `..`, NUL or empty.
pub fn check_rel_path(p: &str) -> Result<(), Error> {
    let bad = p.is_empty()
        || p.contains('\0')
        || p.starts_with('/')
        || p.contains('\\')
        || p.split('/').any(|c| c == ".." || c.is_empty() || c == ".");
    if bad {
        Err(Error::UnsafePath(p.to_string()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_paths() {
        for ok in ["a/b.json", "samples/x.jsonl"] {
            assert!(check_rel_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "a\\b",
            "./a",
            "a\0",
        ] {
            assert!(check_rel_path(bad).is_err(), "{bad:?}");
        }
    }
}
