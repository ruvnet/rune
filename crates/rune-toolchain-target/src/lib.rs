//! Hardware contracts and artifact envelopes (ADR 006).
//!
//! * [`descriptors`] pins triple, features, OS, linker, sysroot, runtime and ABI.
//! * [`cross`] builds a Cargo project for a target. A build alone is
//!   [`Evidence::CompileOnly`](rune_core::target::Evidence).
//! * [`exec`] runs artifacts and classifies the evidence honestly: only a run on
//!   matching hardware is `Native`; qemu-user and Node's WASI are `Emulated`
//!   (correctness only, never performance); a missing executor is `Unvalidated`.
//! * [`gate`] blocks loading on ABI mismatch or a missing CPU feature.
//! * [`envelope`] writes and verifies an ordinary development evidence
//!   directory. Signatures prove provenance and integrity, not correctness.
//!
//! RVF packaging is deliberately not provided: `rvf-runtime` 0.3.2 on crates.io
//! exposes no arbitrary-payload storage (only vectors and typed kernel, eBPF,
//! dashboard and WASM segments), so the evidence directory is the only format.

pub mod cross;
pub mod descriptors;
pub mod envelope;
mod error;
pub mod exec;
pub mod gate;
pub mod host;
pub mod sign;
pub mod verify;

pub use error::Error;
pub use exec::perf_claim_allowed;
