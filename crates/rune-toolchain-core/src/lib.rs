//! Immutable build plans, tree digests and the plan store behind Rune.
//!
//! A [`Plan`] binds the exact inputs of a build (source tree, lockfile,
//! toolchain, target, policy). Its id is the SHA-256 of its canonical JSON,
//! so any change to an input yields a different plan.

mod digest;
mod error;
mod plan;
mod store;

pub use digest::{file_digest, hex_sha256, tree_digest};
pub use error::Error;
pub use plan::{Plan, Policy, Receipt, SCHEMA};
pub use store::Store;
