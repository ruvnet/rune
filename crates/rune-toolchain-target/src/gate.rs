//! Load gate (ADR 006 acceptance): a mismatched ABI or a missing CPU feature
//! blocks loading; a missing executor yields an unvalidated status.

use crate::envelope::Manifest;
use crate::exec::{executor_for, Executor};
use crate::host::Host;
use rune_core::target::Evidence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    /// May be loaded; `evidence` is what running it here would prove.
    Loadable { evidence: Evidence },
    /// Cannot be run here at all: not a failure, but not validated either.
    Unvalidated { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("ABI mismatch: artifact needs {artifact}, runner provides {provided}")]
    AbiMismatch { artifact: String, provided: String },
    #[error("target mismatch: artifact is {artifact}, host is {host}")]
    TripleMismatch { artifact: String, host: String },
    #[error("host lacks required cpu features: {}", .0.join(", "))]
    MissingCpuFeatures(Vec<String>),
}

pub fn check_loadable(envelope: &Manifest, host: &Host) -> Result<LoadStatus, LoadError> {
    check_loadable_with(envelope, host, executor_for(&envelope.target).as_ref())
}

/// Same as [`check_loadable`] with an explicit executor (for tests and for
/// callers that resolve executors themselves).
///
/// An unavailable executor is reported first as `Unvalidated`. For emulated
/// runs the CPU is the emulator's, so host features are not consulted; only
/// native execution is gated on the host's CPU flags.
pub fn check_loadable_with(
    envelope: &Manifest,
    host: &Host,
    exec: &dyn Executor,
) -> Result<LoadStatus, LoadError> {
    let t = &envelope.target;
    if let Err(reason) = exec.availability() {
        return Ok(LoadStatus::Unvalidated { reason });
    }
    let native = exec.evidence() == Evidence::Native;
    let provided_abi = exec.supported_abi().unwrap_or(&host.abi);
    if t.abi != provided_abi {
        return Err(LoadError::AbiMismatch {
            artifact: t.abi.clone(),
            provided: provided_abi.to_owned(),
        });
    }
    let provided_triple = exec.supported_triple().unwrap_or(&host.triple);
    if t.triple != provided_triple {
        return Err(LoadError::TripleMismatch {
            artifact: t.triple.clone(),
            host: provided_triple.to_owned(),
        });
    }
    if native {
        let missing: Vec<String> = t
            .cpu_features
            .iter()
            .filter(|f| !host.features.contains(*f))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(LoadError::MissingCpuFeatures(missing));
        }
    }
    Ok(LoadStatus::Loadable {
        evidence: exec.evidence(),
    })
}
