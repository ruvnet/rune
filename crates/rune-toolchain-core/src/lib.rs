//! Immutable build plans, tree digests, the plan store, cost ledger and the
//! shared contracts (edit, evaluation, target) behind Rune.
//!
//! A [`Plan`] binds the exact inputs of a build (source tree, lockfile,
//! toolchain, target, policy). Its id is the SHA-256 of its canonical JSON,
//! so any change to an input yields a different plan.

pub mod contract;
mod digest;
pub mod edit;
mod error;
pub mod ledger;
mod plan;
mod store;
pub mod target;

pub use digest::{file_digest, hex_sha256, tree_digest};
pub use error::Error;
pub use ledger::Ledger;
pub use plan::{Plan, Policy, Receipt, SCHEMA};
pub use store::Store;
