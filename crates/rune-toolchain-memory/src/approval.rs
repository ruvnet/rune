//! Approval keys (ADR 004). Approval is a stored record with an exact key;
//! nothing computed from similarity or retrieval can produce one.

use crate::recipe::{Recipe, Status};
use rune_core::hex_sha256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Everything besides the candidate that an approval is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyInputs {
    pub target_digest: String,
    pub compiler_id: String,
    pub policy_digest: String,
    pub contract_digest: String,
    pub evaluator_version: String,
}

/// SHA-256 over length-prefixed fields, so field boundaries cannot be shifted.
pub fn approval_key(
    recipe_candidate_hash: &str,
    target_digest: &str,
    compiler_id: &str,
    policy_digest: &str,
    contract_digest: &str,
    evaluator_version: &str,
) -> String {
    let mut buf = b"rune.approval/1".to_vec();
    for f in [
        recipe_candidate_hash,
        target_digest,
        compiler_id,
        policy_digest,
        contract_digest,
        evaluator_version,
    ] {
        buf.extend_from_slice(&(f.len() as u64).to_le_bytes());
        buf.extend_from_slice(f.as_bytes());
    }
    hex_sha256(&buf)
}

impl KeyInputs {
    pub fn key_for(&self, candidate_hash: &str) -> String {
        approval_key(
            candidate_hash,
            &self.target_digest,
            &self.compiler_id,
            &self.policy_digest,
            &self.contract_digest,
            &self.evaluator_version,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub key: String,
    pub recipe_id: String,
}

/// The set of stored approval records.
#[derive(Debug, Clone, Default)]
pub struct ApprovalBook {
    by_key: HashMap<String, String>,
}

impl ApprovalBook {
    pub fn from_records(records: impl IntoIterator<Item = ApprovalRecord>) -> Self {
        ApprovalBook {
            by_key: records.into_iter().map(|r| (r.key, r.recipe_id)).collect(),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.by_key.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// True only when a stored record has exactly the key derived from the
/// recipe's candidate hash and `inputs`. A rejected recipe is never approved.
/// The recipe's fingerprint and `status` field play no part.
pub fn is_approved(recipe: &Recipe, inputs: &KeyInputs, book: &ApprovalBook) -> bool {
    recipe.status != Status::Rejected
        && book
            .by_key
            .get(&inputs.key_for(&recipe.candidate_hash))
            .is_some_and(|rid| *rid == recipe.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rank::{HashedFeatureRanker, Ranker};
    use crate::recipe::*;
    use crate::store::Snapshot;

    fn fp(triple: &str) -> Fingerprint {
        Fingerprint {
            source_tags: vec!["loop".into()],
            target_triple: triple.into(),
            cpu_features: vec!["avx2".into()],
            compiler_id: "cc1".into(),
            abi: "sysv".into(),
            workload_tags: vec!["dense".into()],
            policy_digest: "pol".into(),
        }
    }

    fn recipe(triple: &str, cand: &str) -> Recipe {
        Recipe {
            id: String::new(),
            parent: None,
            proposer: "p".into(),
            descriptor: "unroll".into(),
            fingerprint: fp(triple),
            template: EditTemplate::default(),
            baseline_hash: "b".into(),
            candidate_hash: cand.into(),
            evaluation_version: "ev1".into(),
            evidence: vec![],
            failures: vec![],
            retention: Retention::Standard,
            status: Status::Proposed,
            tenant: "t".into(),
            visibility: Visibility::Shared,
        }
        .sealed()
    }

    fn inputs(target: &str) -> KeyInputs {
        KeyInputs {
            target_digest: target.into(),
            compiler_id: "cc1".into(),
            policy_digest: "pol".into(),
            contract_digest: "ctr".into(),
            evaluator_version: "ev1".into(),
        }
    }

    fn book_for(r: &Recipe, i: &KeyInputs) -> ApprovalBook {
        ApprovalBook::from_records([ApprovalRecord {
            key: i.key_for(&r.candidate_hash),
            recipe_id: r.id.clone(),
        }])
    }

    #[test]
    fn approval_matches_exact_key_only() {
        let r = recipe("x86_64-unknown-linux-gnu", "cand");
        let x86 = inputs("target-x86");
        let book = book_for(&r, &x86);
        assert!(is_approved(&r, &x86, &book));
        assert!(!is_approved(&r, &inputs("target-aarch64"), &book));
    }

    #[test]
    fn x86_approval_never_transfers_to_aarch64_by_similarity() {
        let x86 = recipe("x86_64-unknown-linux-gnu", "cand");
        let book = book_for(&x86, &inputs("target-x86"));
        // Same candidate, fingerprint identical except the triple label.
        let arm = recipe("aarch64-unknown-linux-gnu", "cand");
        let q = arm.fingerprint.clone();
        let mut same = x86.fingerprint.clone();
        same.target_triple = q.target_triple.clone();
        assert_eq!(same, q); // similarity 1.0 on every other field
        let snap = Snapshot::from_recipes("t", vec![x86.clone(), arm.clone()]);
        let hits = HashedFeatureRanker.rank(&q, &snap, 2);
        assert_eq!(hits[0].id, arm.id); // retrieval ranks it first...
        assert!(!is_approved(&arm, &inputs("target-aarch64"), &book)); // ...approves nothing
        assert!(!is_approved(&x86, &inputs("target-aarch64"), &book));
        // Even a recipe labelled Approved gains nothing without a record.
        let mut labelled = arm;
        labelled.status = Status::Approved;
        assert!(!is_approved(&labelled, &inputs("target-aarch64"), &book));
    }

    #[test]
    fn compiler_policy_contract_evaluator_change_invalidate() {
        let r = recipe("x86_64-unknown-linux-gnu", "cand");
        let base = inputs("target-x86");
        let book = book_for(&r, &base);
        let mut c = base.clone();
        c.compiler_id = "cc2".into();
        let mut p = base.clone();
        p.policy_digest = "pol2".into();
        let mut k = base.clone();
        k.contract_digest = "ctr2".into();
        let mut e = base.clone();
        e.evaluator_version = "ev2".into();
        for changed in [c, p, k, e] {
            assert!(!is_approved(&r, &changed, &book));
        }
        assert!(is_approved(&r, &base, &book));
    }

    #[test]
    fn rejected_recipe_is_not_approved_and_key_is_unambiguous() {
        let mut r = recipe("x86_64-unknown-linux-gnu", "cand");
        let i = inputs("t");
        let book = book_for(&r, &i);
        r.status = Status::Rejected;
        assert!(!is_approved(&r, &i, &book));
        assert_ne!(
            approval_key("ab", "c", "", "", "", ""),
            approval_key("a", "bc", "", "", "", "")
        );
    }
}
