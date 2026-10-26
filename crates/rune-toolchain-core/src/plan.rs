use crate::{hex_sha256, Error};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "rune.plan/1";

/// Explicit limits for exploratory work. `build` ignores them; `improve`
/// refuses to run without them (ADR 002/008).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    pub max_usd: f64,
    pub max_tokens: u64,
    pub max_seconds: u64,
    pub max_memory_mb: u64,
}

impl Policy {
    pub fn validate(&self) -> Result<(), Error> {
        let ok = self.max_usd.is_finite()
            && self.max_usd > 0.0
            && self.max_tokens > 0
            && self.max_seconds > 0
            && self.max_memory_mb > 0;
        if ok {
            Ok(())
        } else {
            Err(Error::Policy(
                "every budget must be finite and greater than zero".into(),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub schema: String,
    pub source_digest: String,
    pub lock_digest: String,
    pub toolchain: String,
    pub target: String,
    pub policy: Policy,
    pub parent: Option<String>,
}

impl Plan {
    pub fn new(
        source_digest: String,
        lock_digest: String,
        toolchain: String,
        target: String,
        policy: Policy,
        parent: Option<String>,
    ) -> Self {
        Plan {
            schema: SCHEMA.into(),
            source_digest,
            lock_digest,
            toolchain,
            target,
            policy,
            parent,
        }
    }

    /// Content address: SHA-256 of the canonical JSON (fixed field order).
    pub fn id(&self) -> String {
        hex_sha256(&serde_json::to_vec(self).expect("plan serializes"))
    }

    pub fn check_schema(&self) -> Result<(), Error> {
        if self.schema == SCHEMA {
            Ok(())
        } else {
            Err(Error::Schema(self.schema.clone()))
        }
    }

    /// Reject the plan if any input differs from the observed value.
    pub fn check_fresh(&self, source: &str, lock: &str, toolchain: &str) -> Result<(), Error> {
        for (field, planned, actual) in [
            ("source_digest", &self.source_digest, source),
            ("lock_digest", &self.lock_digest, lock),
            ("toolchain", &self.toolchain, toolchain),
        ] {
            if planned != actual {
                return Err(Error::Stale {
                    field,
                    planned: planned.clone(),
                    actual: actual.into(),
                });
            }
        }
        Ok(())
    }
}

/// Evidence that a plan was executed and produced a specific artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub plan_id: String,
    pub artifact: String,
    pub artifact_digest: String,
    pub offline: bool,
    pub model_calls: u32,
}
