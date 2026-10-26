//! Rune's independent evaluator (ADR 005) and copy-on-write candidate
//! workspace (ADR 007).
//!
//! Honest scope: this is Rune's own implementation. It is not MetaHarness
//! (no performance-evaluation API exists there) and not AgenticOW (a vector
//! memory store). A filesystem fork plus best-effort OS limits is reported as
//! exactly that; it is not a security sandbox.

pub mod bench;
pub mod cargo_toml;
pub mod evaluator;
pub mod gates;
pub mod harness;
pub mod lexer;
pub mod report;
pub mod runner;
pub mod scan;
pub mod stats;
pub mod sysinfo;
pub mod tdist;
pub mod workspace;

pub use evaluator::{EvalRequest, Evaluator, RuneEvaluator};
pub use report::FullReport;
pub use workspace::CowWorkspace;
