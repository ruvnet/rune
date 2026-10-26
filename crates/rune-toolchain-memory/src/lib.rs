//! Evidence and transfer memory (ADR 004).
//!
//! Records are descriptors that point at evidence, never instructions.
//! Retrieval ranks candidates; only an approval record whose key binds the
//! exact candidate, target, compiler, policy, contract and evaluator can mark
//! a recipe approved ([`approval::is_approved`]). Similarity never approves.

pub mod approval;
pub mod control;
mod error;
pub mod ingest;
pub mod rank;
#[cfg(feature = "ruvector")]
pub mod rank_ruvector;
pub mod recipe;
pub mod store;

pub use approval::{approval_key, is_approved, ApprovalBook, ApprovalRecord, KeyInputs};
pub use error::Error;
pub use ingest::Evidence;
pub use rank::{HashedFeatureRanker, Ranker, Scored};
pub use recipe::{
    EditTemplate, EvidenceRef, Failure, FailureScope, Fingerprint, Recipe, Retention, Status,
    Visibility,
};
pub use store::{export_shareable, Snapshot, Store};
