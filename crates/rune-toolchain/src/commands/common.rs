//! Helpers shared by the command modules.

use super::Failure;
use rune_core::contract::{EvalContract, EvalReport};
use rune_core::target::{Evidence, TargetDescriptor};
use rune_core::{hex_sha256, Policy};
use rune_target::descriptors as d;
use serde::Deserialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall clock, read only at the CLI boundary and handed to library code.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .unwrap_or(0)
}

/// Digest of the full `rustc -vV` text.
pub fn compiler_id() -> Result<String, Failure> {
    Ok(hex_sha256(rune_cargo::toolchain_id()?.as_bytes()))
}

/// Same formula the artifact envelope uses for its `policy_digest`.
pub fn policy_digest(p: &Policy) -> String {
    hex_sha256(&serde_json::to_vec(p).expect("policy serializes"))
}

/// Target by short name or full triple. `features` are the CPU flags the
/// artifact requires (only meaningful for native and aarch64 descriptors).
pub fn descriptor(name: &str, features: &[String]) -> Result<TargetDescriptor, Failure> {
    let f: Vec<&str> = features.iter().map(String::as_str).collect();
    match name {
        "x86_64" | d::X86_64_LINUX => Ok(d::x86_64_linux_gnu(&f)),
        "aarch64" | d::AARCH64_LINUX => Ok(d::aarch64_linux_gnu(&f)),
        "wasm32" | d::WASM32_WASI => Ok(d::wasm32_wasip1()),
        "rvm" => Ok(d::rvm()),
        other => Err(Failure::Other(format!(
            "unknown target `{other}` (try: x86_64, aarch64, wasm32, rvm)"
        ))),
    }
}

/// Serialized name of an evidence class: native, emulated, compile_only, unvalidated.
pub fn evidence_name(e: Evidence) -> String {
    serde_json::to_value(e)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub fn perf_line(e: Evidence) -> &'static str {
    if rune_target::perf_claim_allowed(e) {
        "performance claims: allowed"
    } else {
        "performance claims: refused"
    }
}

pub fn load_contract(path: &Path) -> Result<EvalContract, Failure> {
    let bytes =
        std::fs::read(path).map_err(|e| Failure::Other(format!("{}: {e}", path.display())))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| Failure::Other(format!("contract {} is invalid: {e}", path.display())))
}

/// An evaluator report as a consumer sees it: the shared [`EvalReport`] plus
/// whatever evidence block the evaluator attached (not required).
#[derive(Deserialize)]
pub struct ReportFile {
    #[serde(flatten)]
    pub report: EvalReport,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
}

impl ReportFile {
    pub fn load(path: &Path) -> Result<Self, Failure> {
        let bytes =
            std::fs::read(path).map_err(|e| Failure::Other(format!("{}: {e}", path.display())))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Failure::Other(format!("report {} is invalid: {e}", path.display())))
    }

    pub fn evaluator_version(&self) -> String {
        self.evidence
            .as_ref()
            .and_then(|e| e.get("evaluator"))
            .and_then(|v| v.as_str())
            .unwrap_or("unversioned")
            .to_owned()
    }
}

pub fn short(s: &str) -> &str {
    &s[..s.len().min(12)]
}
