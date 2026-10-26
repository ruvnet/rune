//! Report types. [`FullReport`] flattens the shared [`EvalReport`] contract
//! (so any consumer of that type can still read it) and adds the evidence an
//! auditor needs. Rejections before any measurement have `samples = 0`.

use crate::gates::GateReport;
use crate::sysinfo::{Load, Pinning};
use crate::workspace::CopyMode;
use rune_core::contract::EvalReport;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricResult {
    pub name: String,
    /// "improvement" or "guard"
    pub role: String,
    pub pairs: usize,
    pub warmup_pairs: usize,
    pub geo_mean_speedup: f64,
    pub arith_mean_speedup: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
    pub bootstrap_ci95_low: f64,
    pub bootstrap_ci95_high: f64,
    pub baseline_median_ns: f64,
    pub candidate_median_ns: f64,
    pub threshold: String,
    pub passed: bool,
    pub baseline_ns: Vec<f64>,
    pub candidate_ns: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub ok: bool,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub evaluator: String,
    pub evaluator_exe_sha256: String,
    pub contract_file_sha256: String,
    /// Digest of the hidden holdout directory, read from outside the candidate tree.
    pub holdout_tree_sha256: String,
    pub rules: Vec<String>,
    pub isolation_unavailable: Vec<String>,
    pub fork_mode: Option<CopyMode>,
    pub pinning: Option<Pinning>,
    pub load_before: Option<Load>,
    pub load_after: Option<Load>,
    pub gates: Option<GateReport>,
    pub steps: Vec<Step>,
    pub metrics: Vec<MetricResult>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FullReport {
    #[serde(flatten)]
    pub report: EvalReport,
    pub evidence: Evidence,
}

pub const RULES: &[&str] = &[
    "paired samples: warmup pairs discarded, then >= contract.samples (min 30) pairs; order alternates every pair",
    "speedup = baseline_ns / candidate_ns per pair; reported mean is the geometric mean; 95% CI is a Student-t interval on ln(speedup)",
    "improvement gate: CI lower bound must be strictly greater than 1 + min_improvement",
    "guard gate (non-inferiority): CI lower bound of the speed ratio must be >= 1 - tolerance; a guard too noisy to show this fails",
    "bootstrap (10000 resamples, percentile) recorded as a cross-check, never used to decide",
    "static security gates run before any candidate code is built or executed",
    "timeouts and missing evidence reject",
];
