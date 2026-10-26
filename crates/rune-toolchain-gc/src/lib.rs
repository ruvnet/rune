//! Isolation, retention and safe cleanup (ADR 007).
//!
//! Generated objects live in a content-addressed store with an ownership
//! manifest. Cleanup is reachability-based, quarantine-first, reversible for
//! seven days, dry-run by default, and never takes a path from the caller:
//! source files are not owned by the store, so they cannot be collected.

mod error;
pub mod gc;
pub mod leases;
pub mod liveness;
pub mod manifest;
pub mod objects;
pub mod paths;
pub mod quarantine;

pub use error::{Error, Result};
pub use gc::{plan_cleanup, plan_cleanup_with, Candidate, CleanupPlan, Reach};
pub use leases::Lease;
pub use liveness::{Liveness, ProcLiveness};
pub use manifest::{Entry, Kind, Manifest, QEntry, Retention};
pub use objects::{object_path, put_object, register_candidate_workspace};
pub use quarantine::{purge, quarantine, restore, Mode, Report, QUARANTINE_SECS};
