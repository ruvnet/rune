//! Optional HNSW candidate retrieval through `ruvector-core` (feature
//! `ruvector`, off by default). Ranking only: it can neither approve nor
//! promote. HNSW is approximate and `ruvector-core` draws node levels from
//! entropy, so the candidate set (not the ordering of what is returned) may
//! differ between builds; scores are recomputed exactly from the snapshot's
//! vectors and sorted with the same deterministic tie-break as the default.

use crate::rank::{featurize, finish, Ranker, Scored, DIM};
use crate::recipe::Fingerprint;
use crate::store::Snapshot;
use crate::Error;
use ruvector_core::index::hnsw::HnswIndex;
use ruvector_core::index::VectorIndex;
use ruvector_core::types::{DistanceMetric, HnswConfig};
use std::collections::HashMap;

/// HNSW index over one snapshot. Build once per campaign.
pub struct RuvectorRanker {
    snapshot_id: String,
    index: HnswIndex,
    by_id: HashMap<String, usize>,
}

impl RuvectorRanker {
    pub fn build(snapshot: &Snapshot) -> Result<Self, Error> {
        let cfg = HnswConfig {
            max_elements: snapshot.len().max(1),
            ..HnswConfig::default()
        };
        let mut index = HnswIndex::new(DIM, DistanceMetric::Cosine, cfg)
            .map_err(|e| Error::Invalid(format!("ruvector: {e}")))?;
        let mut by_id = HashMap::new();
        for (i, r) in snapshot.recipes().iter().enumerate() {
            // An all-zero vector has no direction; leave it unindexed.
            if snapshot.vector(i).iter().all(|x| *x == 0.0) {
                continue;
            }
            index
                .add(r.id.clone(), snapshot.vector(i).to_vec())
                .map_err(|e| Error::Invalid(format!("ruvector: {e}")))?;
            by_id.insert(r.id.clone(), i);
        }
        Ok(RuvectorRanker {
            snapshot_id: snapshot.id().to_string(),
            index,
            by_id,
        })
    }
}

impl Ranker for RuvectorRanker {
    /// Panics-free: a snapshot other than the one indexed yields no results.
    fn rank(&self, q: &Fingerprint, snapshot: &Snapshot, k: usize) -> Vec<Scored> {
        if snapshot.id() != self.snapshot_id || k == 0 {
            return vec![];
        }
        let qv = featurize(q);
        let Ok(found) = self.index.search_ef(&qv, k, Some((k * 8).max(256))) else {
            return vec![];
        };
        let hits = found
            .iter()
            .filter_map(|s| self.by_id.get(&s.id).copied())
            .map(|i| {
                let dot: f32 = snapshot.vector(i).iter().zip(&qv).map(|(a, b)| a * b).sum();
                (dot, i)
            })
            .collect();
        finish(snapshot, hits, k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::*;

    #[test]
    fn hnsw_finds_the_exact_match() {
        let recs: Vec<Recipe> = (0..50)
            .map(|i| {
                Recipe {
                    id: String::new(),
                    parent: None,
                    proposer: "p".into(),
                    descriptor: format!("r{i}"),
                    fingerprint: Fingerprint {
                        source_tags: vec![format!("s{i}")],
                        target_triple: "x86_64".into(),
                        ..Default::default()
                    },
                    template: EditTemplate::default(),
                    baseline_hash: "b".into(),
                    candidate_hash: format!("c{i}"),
                    evaluation_version: "e".into(),
                    evidence: vec![],
                    failures: vec![],
                    retention: Retention::Standard,
                    status: Status::Proposed,
                    tenant: "t".into(),
                    visibility: Visibility::Shared,
                }
                .sealed()
            })
            .collect();
        let snap = Snapshot::from_recipes("t", recs.clone());
        let ranker = RuvectorRanker::build(&snap).unwrap();
        let hits = ranker.rank(&recs[7].fingerprint, &snap, 5);
        // HNSW is approximate (and entropy-seeded), so recall is measured in
        // tests/latency.rs, not asserted here. Invariants that must always hold:
        assert!(!hits.is_empty() && hits.len() <= 5);
        assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
        assert!(hits
            .iter()
            .all(|h| snap.recipes().iter().any(|r| r.id == h.id)));
        let other = Snapshot::from_recipes("t", recs[..10].to_vec());
        assert!(ranker.rank(&recs[7].fingerprint, &other, 5).is_empty());
    }
}
