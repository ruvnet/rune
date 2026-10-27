//! Authenticated evaluator provenance. Trust anchors are supplied by the caller,
//! never taken from the receipt itself. Keys must belong to a trusted evaluator
//! account; signatures do not isolate malicious code running as that account.
use crate::{sign, Error};
use rune_core::hex_sha256;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: &str = "rune.evaluation/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationIdentity {
    pub schema: String,
    pub report_sha256: String,
    pub baseline_digest: String,
    pub compiler_id: String,
    pub target_digest: String,
    pub policy_digest: String,
    pub evaluator_version: String,
    pub degraded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReceipt {
    pub identity: EvaluationIdentity,
    pub signature: sign::SigFile,
}

impl EvaluationReceipt {
    pub fn sign(identity: EvaluationIdentity, key: &Path) -> Result<Self, Error> {
        let signature = sign::sign_evaluation(
            &sign::load_signing_key(key)?,
            &serde_json::to_vec(&identity)?,
        );
        Ok(Self {
            identity,
            signature,
        })
    }

    pub fn verify(&self, report: &[u8], trusted_key: &Path) -> Result<(), Error> {
        if self.identity.schema != SCHEMA || self.identity.report_sha256 != hex_sha256(report) {
            return Err(Error::Verify(
                "evaluation schema or report digest mismatch".into(),
            ));
        }
        sign::verify_evaluation(
            &sign::load_verifying_key(trusted_key)?,
            &self.signature,
            &serde_json::to_vec(&self.identity)?,
        )
    }
}
