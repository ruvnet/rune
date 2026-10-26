//! Independent-evaluation contract (ADR 005). The proposer produces a candidate;
//! only the evaluator, driven by a frozen contract, can accept it.

use serde::{Deserialize, Serialize};

/// Frozen before any candidate exists. Its digest is part of the approval key,
/// so a proposer that edits it invalidates every approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalContract {
    pub name: String,
    /// Workspace-relative crate/dir holding the kernel under test.
    pub kernel_dir: String,
    /// Command (argv) that runs the visible tests in the kernel dir.
    pub test_cmd: Vec<String>,
    /// Directory (outside the candidate tree) of hidden holdout files; each is
    /// copied into `kernel_dir/<holdout_dest>` just before the holdout run.
    pub holdout_dir: String,
    pub holdout_dest: String,
    pub holdout_cmd: Vec<String>,
    /// Command that builds and prints one JSON object `{"ns_per_op": f64}` per run.
    pub bench_cmd: Vec<String>,
    /// Additional preregistered metrics (name -> bench argv) that must not regress.
    pub guard_benches: Vec<GuardBench>,
    pub samples: u32,
    pub warmup: u32,
    /// Lower bound of the 95% CI of the speedup must exceed `1 + min_improvement`.
    pub min_improvement: f64,
    pub forbid_unsafe: bool,
    pub forbid_new_deps: bool,
    pub forbid_new_capabilities: bool,
    pub max_seconds: u64,
    pub max_memory_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardBench {
    pub name: String,
    pub bench_cmd: Vec<String>,
    /// Allowed slowdown fraction (noise-aware tolerance), e.g. 0.05.
    pub tolerance: f64,
}

impl EvalContract {
    pub fn digest(&self) -> String {
        crate::hex_sha256(&serde_json::to_vec(self).expect("contract serializes"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", content = "detail", rename_all = "snake_case")]
pub enum Reject {
    BuildFailed(String),
    VisibleTestsFailed(String),
    HoldoutFailed(String),
    NewUnsafe(String),
    NewDependency(String),
    NewCapability(String),
    GuardRegression(String),
    InsufficientImprovement(String),
    Timeout(String),
    MissingEvidence(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    pub contract_digest: String,
    pub baseline_digest: String,
    pub candidate_digest: String,
    pub samples: u32,
    /// Speedup = baseline_ns / candidate_ns, paired per sample.
    pub speedup_mean: f64,
    pub speedup_ci95_low: f64,
    pub speedup_ci95_high: f64,
    pub verdict: Verdict,
    /// Layers of isolation actually applied (e.g. "cow-copy", "timeout", "rlimit-as", "netns").
    pub isolation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "reject", rename_all = "snake_case")]
pub enum Verdict {
    Accept,
    Reject(Reject),
}
