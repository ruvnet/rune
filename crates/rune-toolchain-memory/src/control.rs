//! Equal-budget random control study (ADR 004).
//!
//! Learning may be claimed to help search only if retrieval-ranked proposals
//! beat uniformly random proposals drawn from the same library, with the same
//! evaluation budget `k`, on workloads the library never saw.
//!
//! Inputs: an outcome matrix `outcomes[recipe][workload]` (accepted by the
//! independent evaluator), a query fingerprint per workload, and for each
//! recipe the training workload it originated from. The library is the set of
//! recipes whose origin is a TRAINING workload; evaluation uses only
//! non-training workloads.
//!
//! Statistics: hit@k rates carry Wilson 95% intervals over `n` unseen
//! workloads (for the random arm the successes are rate x n, since each
//! workload's random hit rate is averaged over `trials` seeded draws). The
//! p-value is a one-sided exact (n <= 20) or Monte-Carlo sign-flip test on
//! the per-workload paired difference (retrieval hit - random hit rate).
//! Mean evaluations-to-first-accept counts a miss as `k` (censored at budget).

use crate::rank::Ranker;
use crate::recipe::{EditTemplate, Fingerprint, Recipe, Retention, Status, Visibility};
use crate::store::Snapshot;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Fewer unseen workloads than this can never support a claim.
pub const MIN_WORKLOADS: usize = 20;
const Z95: f64 = 1.959_963_984_540_054;
const MC_DRAWS: u32 = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkloadSpec {
    pub name: String,
    pub training: bool,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeSpec {
    pub id: String,
    /// Name of the workload this recipe was learned on.
    pub origin: String,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlStudy {
    /// Evaluation budget per workload, identical for both arms.
    pub k: usize,
    /// Random draws per workload.
    pub trials: u32,
    pub seed: u64,
    pub workloads: Vec<WorkloadSpec>,
    pub recipes: Vec<RecipeSpec>,
    /// `outcomes[recipe][workload]`, in the order of the two lists above.
    pub outcomes: Vec<Vec<bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Retrieval's hit-rate interval is above random's and p < 0.05.
    Better,
    /// Enough data, but the intervals overlap or the test is not significant.
    NoDifference,
    /// Too few unseen workloads or too small a library to say anything.
    Insufficient,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlReport {
    pub n_workloads: usize,
    pub n_recipes: usize,
    /// Recipes dropped because their origin is not a training workload.
    pub n_excluded_recipes: usize,
    pub k: usize,
    pub trials: u32,
    pub seed: u64,
    pub retrieval_hit_rate: f64,
    pub retrieval_wilson95: (f64, f64),
    pub random_hit_rate: f64,
    pub random_wilson95: (f64, f64),
    pub retrieval_mean_evals_to_first_accept: f64,
    pub random_mean_evals_to_first_accept: f64,
    pub p_value: f64,
    pub verdict: Verdict,
}

/// Wilson score interval for `successes` out of `n` trials.
pub fn wilson(successes: f64, n: f64) -> (f64, f64) {
    if n <= 0.0 {
        return (0.0, 1.0);
    }
    let p = successes / n;
    let z2 = Z95 * Z95;
    let denom = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denom;
    let half = Z95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

impl ControlStudy {
    pub fn from_json(text: &str) -> Result<ControlStudy, Error> {
        let s: ControlStudy = serde_json::from_str(text)?;
        s.check()?;
        Ok(s)
    }

    fn check(&self) -> Result<(), Error> {
        let bad = |m: &str| Err(Error::Invalid(format!("control study: {m}")));
        if self.k == 0 || self.trials == 0 {
            return bad("k and trials must be positive");
        }
        if self.outcomes.len() != self.recipes.len()
            || self
                .outcomes
                .iter()
                .any(|row| row.len() != self.workloads.len())
        {
            return bad("outcomes must be recipes x workloads");
        }
        let names: HashSet<&str> = self.workloads.iter().map(|w| w.name.as_str()).collect();
        let ids: HashSet<&str> = self.recipes.iter().map(|r| r.id.as_str()).collect();
        if names.len() != self.workloads.len() || ids.len() != self.recipes.len() {
            return bad("duplicate workload name or recipe id");
        }
        if self
            .recipes
            .iter()
            .any(|r| !names.contains(r.origin.as_str()))
        {
            return bad("recipe origin is not a known workload");
        }
        Ok(())
    }

    fn library_snapshot(&self, train: &HashSet<&str>) -> (Snapshot, Vec<usize>) {
        let lib: Vec<usize> = (0..self.recipes.len())
            .filter(|&i| train.contains(self.recipes[i].origin.as_str()))
            .collect();
        let recs = lib
            .iter()
            .map(|&i| Recipe {
                id: self.recipes[i].id.clone(),
                parent: None,
                proposer: "control".into(),
                descriptor: String::new(),
                fingerprint: self.recipes[i].fingerprint.clone(),
                template: EditTemplate::default(),
                baseline_hash: String::new(),
                candidate_hash: self.recipes[i].id.clone(),
                evaluation_version: String::new(),
                evidence: vec![],
                failures: vec![],
                retention: Retention::Ephemeral,
                status: Status::Proposed,
                tenant: "control".into(),
                visibility: Visibility::Shared,
            })
            .collect();
        (Snapshot::from_recipes("control", recs), lib)
    }

    /// Run the study with `ranker` as the retrieval arm.
    pub fn run(&self, ranker: &dyn Ranker) -> Result<ControlReport, Error> {
        self.check()?;
        let train: HashSet<&str> = self
            .workloads
            .iter()
            .filter(|w| w.training)
            .map(|w| w.name.as_str())
            .collect();
        let (snap, lib) = self.library_snapshot(&train);
        let by_id: HashMap<&str, usize> = lib
            .iter()
            .map(|&i| (self.recipes[i].id.as_str(), i))
            .collect();
        let test: Vec<usize> = (0..self.workloads.len())
            .filter(|&w| !self.workloads[w].training)
            .collect();
        let k = self.k.min(lib.len());
        let n = test.len();
        let mut rng = SplitMix(self.seed);
        let mut scratch: Vec<usize> = lib.clone();

        let (mut ret_hits, mut ret_evals) = (0u32, 0.0f64);
        let (mut rand_rate_sum, mut rand_evals) = (0.0f64, 0.0f64);
        let mut diffs = Vec::with_capacity(n);
        for &w in &test {
            // Retrieval arm: top-k by rank, evaluated in rank order.
            let ranked = ranker.rank(&self.workloads[w].fingerprint, &snap, k);
            let pos = ranked.iter().position(|s| {
                by_id
                    .get(s.id.as_str())
                    .is_some_and(|&i| self.outcomes[i][w])
            });
            let hit = u32::from(pos.is_some());
            ret_hits += hit;
            ret_evals += pos.map_or(k, |p| p + 1) as f64;
            // Random arm: `trials` uniform draws of k distinct library recipes.
            let (mut rhits, mut revals) = (0u32, 0.0f64);
            for _ in 0..self.trials {
                let mut first = None;
                for j in 0..k {
                    let pick = j + rng.below(scratch.len() - j);
                    scratch.swap(j, pick);
                    if first.is_none() && self.outcomes[scratch[j]][w] {
                        first = Some(j + 1);
                    }
                }
                rhits += u32::from(first.is_some());
                revals += first.unwrap_or(k) as f64;
            }
            let rate = f64::from(rhits) / f64::from(self.trials);
            rand_rate_sum += rate;
            rand_evals += revals / f64::from(self.trials);
            diffs.push(f64::from(hit) - rate);
        }

        let nf = n.max(1) as f64;
        let ret_rate = f64::from(ret_hits) / nf;
        let rand_rate = rand_rate_sum / nf;
        let ret_ci = wilson(f64::from(ret_hits), n as f64);
        let rand_ci = wilson(rand_rate_sum, n as f64);
        let p = sign_flip_p(&diffs, &mut rng);
        let verdict = if n < MIN_WORKLOADS || lib.len() <= k {
            Verdict::Insufficient
        } else if ret_ci.0 > rand_ci.1 && p < 0.05 && ret_rate > rand_rate {
            Verdict::Better
        } else {
            Verdict::NoDifference
        };
        Ok(ControlReport {
            n_workloads: n,
            n_recipes: lib.len(),
            n_excluded_recipes: self.recipes.len() - lib.len(),
            k,
            trials: self.trials,
            seed: self.seed,
            retrieval_hit_rate: ret_rate,
            retrieval_wilson95: ret_ci,
            random_hit_rate: rand_rate,
            random_wilson95: rand_ci,
            retrieval_mean_evals_to_first_accept: ret_evals / nf,
            random_mean_evals_to_first_accept: rand_evals / nf,
            p_value: p,
            verdict,
        })
    }
}

/// One-sided sign-flip test of "mean paired difference > 0".
fn sign_flip_p(d: &[f64], rng: &mut SplitMix) -> f64 {
    let n = d.len();
    if n == 0 {
        return 1.0;
    }
    let obs: f64 = d.iter().sum();
    let ge = |t: f64| t >= obs - 1e-9;
    if n <= 20 {
        let total = 1u32 << n;
        let count = (0..total)
            .filter(|mask| {
                let t: f64 = d
                    .iter()
                    .enumerate()
                    .map(|(i, x)| if mask >> i & 1 == 1 { -x } else { *x })
                    .sum();
                ge(t)
            })
            .count();
        count as f64 / f64::from(total)
    } else {
        let mut count = 0u32;
        for _ in 0..MC_DRAWS {
            let t: f64 = d
                .iter()
                .map(|x| if rng.next() & 1 == 1 { -x } else { *x })
                .sum();
            count += u32::from(ge(t));
        }
        f64::from(count + 1) / f64::from(MC_DRAWS + 1)
    }
}

impl ControlReport {
    pub fn render_markdown(&self) -> String {
        let verdict = match self.verdict {
            Verdict::Better => "retrieval is better than the equal-budget random control",
            Verdict::NoDifference => {
                "no demonstrated difference: do NOT claim learning improves search"
            }
            Verdict::Insufficient => "insufficient data: do NOT claim learning improves search",
        };
        let pct = |x: f64| format!("{:.1}%", 100.0 * x);
        let ci = |c: (f64, f64)| format!("[{}, {}]", pct(c.0), pct(c.1));
        format!(
            "## Equal-budget random control (ADR 004)\n\n\
             {} unseen workloads, {} library recipes (training workloads only; {} recipes excluded), \
             budget k={} evaluations per workload, {} random trials per workload, seed {}.\n\n\
             | arm | hit@k | Wilson 95% | mean evals to first accept |\n\
             |---|---|---|---|\n\
             | retrieval top-k | {} | {} | {:.2} |\n\
             | uniform random k | {} | {} | {:.2} |\n\n\
             Paired sign-flip p-value (one-sided): {:.4}\n\n\
             Verdict: **{:?}**: {}.\n\n\
             Evaluations-to-first-accept counts a miss as k. The random arm's interval treats its \
             per-workload mean hit rate x n as successes.\n",
            self.n_workloads,
            self.n_recipes,
            self.n_excluded_recipes,
            self.k,
            self.trials,
            self.seed,
            pct(self.retrieval_hit_rate),
            ci(self.retrieval_wilson95),
            self.retrieval_mean_evals_to_first_accept,
            pct(self.random_hit_rate),
            ci(self.random_wilson95),
            self.random_mean_evals_to_first_accept,
            self.p_value,
            self.verdict,
            verdict
        )
    }
}
