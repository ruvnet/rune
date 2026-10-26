//! Evidence validation. Evidence enters the store only through [`Evidence::validate`].

use crate::recipe::{check_rel_path, FailureScope};
use crate::Error;
use rune_core::contract::{EvalContract, EvalReport, Reject, Verdict};
use serde::{Deserialize, Serialize};

/// One evaluator run bound to the recipe it evaluated and to the exact
/// target, compiler, policy and evaluator it ran under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub recipe_id: String,
    pub report: EvalReport,
    pub target_digest: String,
    pub compiler_id: String,
    pub policy_digest: String,
    pub evaluator_version: String,
    /// Workspace-relative path of the raw samples.
    pub sample_path: String,
    pub cost_micro_usd: u64,
    /// Where a rejection applies (used when the verdict is Reject).
    pub scope: FailureScope,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::EvidenceRejected(msg.into())
}

impl Evidence {
    /// Check internal consistency of the report and that it was produced
    /// under `contract`.
    pub fn validate(&self, contract: &EvalContract) -> Result<(), Error> {
        let r = &self.report;
        check_rel_path(&self.sample_path)?;
        if r.contract_digest != contract.digest() {
            return Err(bad("contract digest mismatch"));
        }
        let stats = [r.speedup_mean, r.speedup_ci95_low, r.speedup_ci95_high];
        let statistical = matches!(
            r.verdict,
            Verdict::Accept
                | Verdict::Reject(Reject::InsufficientImprovement(_))
                | Verdict::Reject(Reject::GuardRegression(_))
        );
        if !statistical {
            // Build/test/policy rejections may carry no measurements, never NaN.
            return if stats.iter().all(|x| x.is_finite()) {
                Ok(())
            } else {
                Err(bad("non-finite statistics"))
            };
        }
        if r.samples == 0 {
            return Err(bad("zero samples"));
        }
        let finite = [r.speedup_mean, r.speedup_ci95_low, r.speedup_ci95_high]
            .iter()
            .all(|x| x.is_finite());
        if !finite || r.speedup_ci95_low <= 0.0 {
            return Err(bad("non-finite or non-positive speedup/ci"));
        }
        if !(r.speedup_ci95_low <= r.speedup_mean && r.speedup_mean <= r.speedup_ci95_high) {
            return Err(bad("ci does not bracket the mean"));
        }
        let clears = r.speedup_ci95_low > 1.0 + contract.min_improvement;
        match &r.verdict {
            Verdict::Accept => {
                if r.samples < contract.samples {
                    return Err(bad("accept with fewer samples than the contract requires"));
                }
                if !clears {
                    return Err(bad(
                        "accept but ci lower bound does not clear min_improvement",
                    ));
                }
                if !r.isolation.iter().any(|s| !s.is_empty()) {
                    return Err(bad("accept without recorded isolation"));
                }
            }
            Verdict::Reject(Reject::InsufficientImprovement(_)) if clears => {
                return Err(bad(
                    "rejected for insufficient improvement but ci clears the bar",
                ));
            }
            Verdict::Reject(_) => {}
        }
        Ok(())
    }
}
