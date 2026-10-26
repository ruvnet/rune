//! Ranking only. A score orders candidates for evaluation; it is never an
//! approval, safety claim or promotion decision.

use crate::recipe::{Failure, Fingerprint, Status};
use crate::store::Snapshot;

/// Fixed dimension of the hashed feature space.
pub const DIM: usize = 256;

/// Feature weights per fingerprint field (documented, fixed). Larger means a
/// mismatch on that field costs more cosine similarity.
pub const W_TARGET: f32 = 3.0;
pub const W_SOURCE: f32 = 2.0;
pub const W_WORKLOAD: f32 = 2.0;
pub const W_CPU: f32 = 1.5;
pub const W_COMPILER: f32 = 1.0;
pub const W_ABI: f32 = 1.0;
pub const W_POLICY: f32 = 0.5;

#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub id: String,
    pub score: f32,
    pub status: Status,
    /// Rejected recipes stay visible together with where they failed.
    pub failures: Vec<Failure>,
}

pub trait Ranker {
    /// Up to `k` records of `snapshot`, best first. Ties break by ascending id.
    fn rank(&self, q: &Fingerprint, snapshot: &Snapshot, k: usize) -> Vec<Scored>;
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn add(v: &mut [f32], field: &str, token: &str, w: f32) {
    if token.is_empty() {
        return;
    }
    let h = fnv1a(&format!("{field}\u{1}{token}"));
    let sign = if (h >> 63) == 0 { 1.0 } else { -1.0 };
    v[(h % DIM as u64) as usize] += sign * w;
}

/// Signed hashed bag of `field:token` features, L2-normalised (zero vector
/// stays zero). Deterministic: fixed hash, fixed summation order.
pub fn featurize(fp: &Fingerprint) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM];
    add(&mut v, "target", &fp.target_triple, W_TARGET);
    add(&mut v, "compiler", &fp.compiler_id, W_COMPILER);
    add(&mut v, "abi", &fp.abi, W_ABI);
    add(&mut v, "policy", &fp.policy_digest, W_POLICY);
    for t in &fp.source_tags {
        add(&mut v, "source", t, W_SOURCE);
    }
    for t in &fp.cpu_features {
        add(&mut v, "cpu", t, W_CPU);
    }
    for t in &fp.workload_tags {
        add(&mut v, "workload", t, W_WORKLOAD);
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

/// Deterministic cosine ranker over the snapshot's precomputed feature vectors.
#[derive(Debug, Default, Clone, Copy)]
pub struct HashedFeatureRanker;

pub(crate) fn finish(snapshot: &Snapshot, mut hits: Vec<(f32, usize)>, k: usize) -> Vec<Scored> {
    let cmp = |a: &(f32, usize), b: &(f32, usize)| {
        b.0.total_cmp(&a.0)
            .then_with(|| snapshot.recipes()[a.1].id.cmp(&snapshot.recipes()[b.1].id))
    };
    if k < hits.len() {
        if k == 0 {
            return vec![];
        }
        hits.select_nth_unstable_by(k - 1, cmp);
        hits.truncate(k);
    }
    hits.sort_by(cmp);
    hits.into_iter()
        .map(|(score, i)| {
            let r = &snapshot.recipes()[i];
            Scored {
                id: r.id.clone(),
                score,
                status: r.status,
                failures: r.failures.clone(),
            }
        })
        .collect()
}

impl Ranker for HashedFeatureRanker {
    fn rank(&self, q: &Fingerprint, snapshot: &Snapshot, k: usize) -> Vec<Scored> {
        let qv = featurize(q);
        let hits = (0..snapshot.recipes().len())
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

    fn r(desc: &str, tag: &str, status: Status) -> Recipe {
        Recipe {
            id: String::new(),
            parent: None,
            proposer: "p".into(),
            descriptor: desc.into(),
            fingerprint: Fingerprint {
                source_tags: vec![tag.into()],
                target_triple: "x86_64".into(),
                ..Default::default()
            },
            template: EditTemplate::default(),
            baseline_hash: "b".into(),
            candidate_hash: desc.into(),
            evaluation_version: "e".into(),
            evidence: vec![],
            failures: vec![Failure {
                reason: "slow".into(),
                scope: FailureScope::default(),
            }],
            retention: Retention::Standard,
            status,
            tenant: "t".into(),
            visibility: Visibility::Shared,
        }
        .sealed()
    }

    #[test]
    fn deterministic_order_and_rejected_stay_searchable() {
        let recs = vec![
            r("a", "loop", Status::Proposed),
            r("b", "loop", Status::Rejected),
            r("c", "other", Status::Proposed),
        ];
        let snap = Snapshot::from_recipes("t", recs.clone());
        let q = recs[0].fingerprint.clone();
        let a = HashedFeatureRanker.rank(&q, &snap, 3);
        assert_eq!(a, HashedFeatureRanker.rank(&q, &snap, 3));
        // identical-fingerprint records tie at 1.0 and order by id
        assert!((a[0].score - 1.0).abs() < 1e-5 && (a[1].score - 1.0).abs() < 1e-5);
        assert!(a[0].id < a[1].id);
        assert!(a
            .iter()
            .any(|s| s.status == Status::Rejected && !s.failures.is_empty()));
        assert_eq!(HashedFeatureRanker.rank(&q, &snap, 1).len(), 1);
        assert!(HashedFeatureRanker.rank(&q, &snap, 0).is_empty());
    }
}
