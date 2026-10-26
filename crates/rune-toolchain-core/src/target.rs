//! Hardware/target contracts (ADR 006). A recipe approved on one fingerprint is
//! never approved on another by similarity.

use serde::{Deserialize, Serialize};

/// How execution evidence for a target was obtained. Only `Native` supports
/// performance claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// Built and run on matching hardware.
    Native,
    /// Built and run under an emulator or foreign runtime: correctness only.
    Emulated,
    /// Built but never executed.
    CompileOnly,
    /// No executor available: status is unvalidated.
    Unvalidated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetDescriptor {
    pub triple: String,
    /// Required CPU features, e.g. `avx2`, `neon`. Loading fails if the host lacks any.
    pub cpu_features: Vec<String>,
    pub os: String,
    pub linker: String,
    pub sysroot: Option<String>,
    /// Runtime/executor, e.g. `native`, `qemu-aarch64-static`, `node-wasi`.
    pub runtime: String,
    pub abi: String,
}

impl TargetDescriptor {
    /// Stable digest binding every field; a change to any field is a different target.
    pub fn digest(&self) -> String {
        crate::hex_sha256(&serde_json::to_vec(self).expect("target serializes"))
    }
}
