//! Timing harness for recipe selection (ADR 008 target: p95 < 10 ms).
//! Run: cargo test -p rune-toolchain-memory --release --test latency -- --ignored --nocapture

use rune_memory::*;
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn fingerprint(rng: &mut Rng) -> Fingerprint {
    let triples = [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "wasm32-wasip1",
        "riscv64gc-unknown-linux-gnu",
    ];
    let cpus = ["avx2", "avx512f", "neon", "sse4.2", "bmi2", "sve"];
    let tags = |p: &str, n: usize, rng: &mut Rng| -> Vec<String> {
        (0..1 + rng.pick(3))
            .map(|_| format!("{p}{}", rng.pick(n)))
            .collect()
    };
    Fingerprint {
        source_tags: tags("src", 60, rng),
        target_triple: triples[rng.pick(triples.len())].into(),
        cpu_features: (0..1 + rng.pick(3))
            .map(|_| cpus[rng.pick(cpus.len())].to_string())
            .collect(),
        compiler_id: format!("cc{}", rng.pick(5)),
        abi: format!("abi{}", rng.pick(3)),
        workload_tags: tags("wl", 40, rng),
        policy_digest: format!("pol{}", rng.pick(4)),
    }
}

#[test]
#[ignore = "timing harness; run with --release --ignored"]
fn selection_latency_over_10k_recipes() {
    let mut rng = Rng(0xC0FFEE);
    let recipes: Vec<Recipe> = (0..10_000)
        .map(|i| {
            Recipe {
                id: String::new(),
                parent: None,
                proposer: "synthetic".into(),
                descriptor: format!("recipe {i}"),
                fingerprint: fingerprint(&mut rng),
                template: EditTemplate::default(),
                baseline_hash: "b".into(),
                candidate_hash: format!("c{i}"),
                evaluation_version: "ev".into(),
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
    let t = Instant::now();
    let snap = Snapshot::from_recipes("t", recipes);
    println!("snapshot build (one-off, 10000 recipes): {:?}", t.elapsed());
    let queries: Vec<Fingerprint> = (0..2000).map(|_| fingerprint(&mut rng)).collect();
    for q in queries.iter().take(50) {
        std::hint::black_box(HashedFeatureRanker.rank(q, &snap, 10)); // warmup
    }
    let mut us: Vec<f64> = queries
        .iter()
        .map(|q| {
            let t = Instant::now();
            std::hint::black_box(HashedFeatureRanker.rank(q, &snap, 10));
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    us.sort_by(f64::total_cmp);
    let pct = |p: f64| us[((us.len() as f64 * p).ceil() as usize - 1).min(us.len() - 1)] / 1000.0;
    println!(
        "recipes=10000 queries={} k=10  p50={:.3} ms  p95={:.3} ms  p99={:.3} ms  max={:.3} ms",
        us.len(),
        pct(0.50),
        pct(0.95),
        pct(0.99),
        us[us.len() - 1] / 1000.0
    );
}

#[cfg(feature = "ruvector")]
#[test]
#[ignore = "timing/recall harness; run with --release --features ruvector --ignored"]
fn ruvector_recall_and_latency() {
    use rune_memory::rank_ruvector::RuvectorRanker;
    let mut rng = Rng(0xC0FFEE);
    let recipes: Vec<Recipe> = (0..10_000)
        .map(|i| {
            Recipe {
                id: String::new(),
                parent: None,
                proposer: "synthetic".into(),
                descriptor: format!("recipe {i}"),
                fingerprint: fingerprint(&mut rng),
                template: EditTemplate::default(),
                baseline_hash: "b".into(),
                candidate_hash: format!("c{i}"),
                evaluation_version: "ev".into(),
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
    let snap = Snapshot::from_recipes("t", recipes);
    let t = Instant::now();
    let hnsw = RuvectorRanker::build(&snap).unwrap();
    println!("hnsw build: {:?}", t.elapsed());
    let queries: Vec<Fingerprint> = (0..1000).map(|_| fingerprint(&mut rng)).collect();
    let (mut found, mut total, mut us) = (0usize, 0usize, vec![]);
    for q in &queries {
        let exact = HashedFeatureRanker.rank(q, &snap, 10);
        let t = Instant::now();
        let approx = hnsw.rank(q, &snap, 10);
        us.push(t.elapsed().as_secs_f64() * 1e3);
        // Count by score so exact ties (hash collisions) are not penalised.
        let floor = exact.last().map_or(0.0, |s| s.score) - 1e-4;
        total += exact.len();
        found += approx
            .iter()
            .filter(|s| s.score >= floor)
            .count()
            .min(exact.len());
    }
    us.sort_by(f64::total_cmp);
    println!(
        "hnsw recall@10 (score-based) = {:.3}  p50={:.3} ms p95={:.3} ms p99={:.3} ms",
        found as f64 / total as f64,
        us[us.len() / 2],
        us[us.len() * 95 / 100],
        us[us.len() * 99 / 100]
    );
}
