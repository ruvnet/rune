//! Property-style loop: deterministic seeded generator, random object graphs,
//! and the invariant that nothing reachable from a root is ever quarantined
//! or purged.

mod common;
use common::*;
use rune_gc::manifest::{Kind, Manifest, Retention};
use rune_gc::{leases, plan_cleanup, purge, put_object, quarantine, Mode};
use std::collections::BTreeSet;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn reachable_objects_are_never_quarantined_or_purged() {
    for seed in 1..=30u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let d = workspace();
        let ws = d.path();
        let ghost = "e".repeat(64);
        let n_plans = rng.below(4) as usize;
        let n_objs = 5 + rng.below(12) as usize;

        // plan ids are content-derived, so create plans first with placeholder receipts
        let mut plan_ids: Vec<String> = Vec::new();
        let mut plan_art: Vec<String> = Vec::new();
        for i in 0..n_plans {
            let art = put_object(
                ws,
                Kind::Artifact,
                &ghost,
                None,
                format!("art-{seed}-{i}").as_bytes(),
            )
            .unwrap();
            let parent = if i > 0 && rng.below(2) == 0 {
                plan_ids.last().cloned()
            } else {
                None
            };
            plan_ids.push(approve(ws, &format!("s{seed}p{i}"), parent, &art));
            plan_art.push(art);
        }
        let mut expected: BTreeSet<String> = plan_art.iter().cloned().collect();
        let mut all: BTreeSet<String> = expected.clone();
        let mut leased = Vec::new();
        for i in 0..n_objs {
            let kind = match rng.below(3) {
                0 => Kind::Artifact,
                1 => Kind::Evidence,
                _ => Kind::Receipt,
            };
            let retention = match rng.below(3) {
                0 => Retention::Transient,
                1 => Retention::WhilePlanReachable,
                _ => Retention::Pinned,
            };
            let owner_real = !plan_ids.is_empty() && rng.below(2) == 0;
            let owner = if owner_real {
                plan_ids[rng.below(plan_ids.len() as u64) as usize].clone()
            } else {
                ghost.clone()
            };
            let k = put_object(
                ws,
                kind,
                &owner,
                Some(retention),
                format!("o-{seed}-{i}").as_bytes(),
            )
            .unwrap();
            all.insert(k.clone());
            if retention == Retention::Pinned
                || (retention == Retention::WhilePlanReachable && owner_real)
            {
                expected.insert(k.clone());
            }
            if rng.below(4) == 0 {
                leased.push(k.clone());
                expected.insert(k);
            }
        }
        if !leased.is_empty() {
            leases::acquire(ws, "j", &leased).unwrap();
        }

        let plan = plan_cleanup(ws).unwrap();
        let cand: BTreeSet<_> = plan.candidates.iter().map(|c| c.key.clone()).collect();
        assert!(
            cand.is_disjoint(&expected),
            "seed {seed}: planned a reachable object"
        );
        assert_eq!(
            cand.len() + expected.len(),
            all.len(),
            "seed {seed}: classification must be exact"
        );

        let r = quarantine(&plan, 0, Mode::Apply).unwrap();
        let moved: BTreeSet<_> = r.done.iter().cloned().collect();
        assert!(moved.is_disjoint(&expected), "seed {seed}");
        purge(ws, 8 * DAY, Mode::Apply).unwrap();
        let m = Manifest::load(ws).unwrap();
        for k in &expected {
            assert!(m.entries.contains_key(k), "seed {seed}: {k} lost ownership");
            assert!(
                rune_gc::object_path(ws, k).unwrap().exists(),
                "seed {seed}: {k} deleted"
            );
        }
        for k in &cand {
            assert!(
                !rune_gc::object_path(ws, k).unwrap().exists(),
                "seed {seed}: unreachable {k} kept"
            );
        }
    }
}
