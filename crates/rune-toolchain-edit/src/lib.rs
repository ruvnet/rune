//! Edit appliers for Rune (ADR 003). Both implement [`rune_core::edit::EditApplier`]:
//!
//! * [`ExactReplace`]: pure Rust, `find` must occur exactly once.
//! * [`AgentBoosterSidecar`]: applies an edit snippet through a pinned
//!   `agent-booster` package running in a short-lived Node process.
//!
//! Every applier confines the path to the request root, rejects a stale
//! `expected_sha256` before touching anything, bounds sizes, and writes
//! atomically (temp file + rename). A rejected edit leaves the file untouched.

mod exact;
mod fsio;
mod sidecar;

pub use exact::ExactReplace;
pub use fsio::confine;
pub use sidecar::AgentBoosterSidecar;
