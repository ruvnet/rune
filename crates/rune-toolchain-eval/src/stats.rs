//! Paired-measurement statistics (ADR 005): per-pair speedup, a t-interval on
//! log-speedups, and a bootstrap cross-check. No statistics dependency.
//!
//! Preregistered rules (also stated in every report):
//! * improvement: accept only if the 95% CI lower bound of the speedup
//!   (baseline_ns / candidate_ns, geometric mean) is strictly greater than
//!   `1 + min_improvement`.
//! * guard (non-inferiority): a guard bench passes only if the 95% CI lower
//!   bound of its speed ratio is >= `1 - tolerance`. A noisy guard whose CI is
//!   too wide to show non-inferiority therefore fails: the burden is on the
//!   candidate, and missing evidence rejects.

use crate::tdist::t975;
use serde::{Deserialize, Serialize};

pub const MIN_SAMPLES: u32 = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub n: usize,
    /// exp(mean(ln speedup)): the geometric mean, the natural centre of a ratio.
    pub geo_mean: f64,
    pub arith_mean: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
    pub sd_log: f64,
}

/// Per-pair speedup `baseline / candidate`. Pairs with non-positive or
/// non-finite timings are an error: bad evidence is never silently dropped.
pub fn paired_speedups(baseline: &[f64], candidate: &[f64]) -> Result<Vec<f64>, String> {
    if baseline.len() != candidate.len() {
        return Err(format!(
            "unpaired samples: {} baseline vs {} candidate",
            baseline.len(),
            candidate.len()
        ));
    }
    baseline
        .iter()
        .zip(candidate)
        .map(|(&b, &c)| {
            if b.is_finite() && c.is_finite() && b > 0.0 && c > 0.0 {
                Ok(b / c)
            } else {
                Err(format!("invalid timing pair ({b}, {c})"))
            }
        })
        .collect()
}

/// t-interval on log-speedups.
pub fn summarize(speedups: &[f64]) -> Result<Summary, String> {
    let n = speedups.len();
    if n < 2 {
        return Err("need at least 2 pairs".into());
    }
    let logs: Vec<f64> = speedups.iter().map(|s| s.ln()).collect();
    let mean = logs.iter().sum::<f64>() / n as f64;
    let var = logs.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    let sd = var.sqrt();
    let half = t975(n - 1) * sd / (n as f64).sqrt();
    Ok(Summary {
        n,
        geo_mean: mean.exp(),
        arith_mean: speedups.iter().sum::<f64>() / n as f64,
        ci95_low: (mean - half).exp(),
        ci95_high: (mean + half).exp(),
        sd_log: sd,
    })
}

/// Deterministic xorshift64* generator for the bootstrap (no rand dependency).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Percentile bootstrap CI (95%) of the geometric mean speedup.
pub fn bootstrap_ci(speedups: &[f64], iters: usize, seed: u64) -> (f64, f64) {
    let logs: Vec<f64> = speedups.iter().map(|s| s.ln()).collect();
    let n = logs.len();
    let mut rng = Rng(seed | 1);
    let mut means: Vec<f64> = (0..iters)
        .map(|_| {
            let mut s = 0.0;
            for _ in 0..n {
                s += logs[(rng.next() % n as u64) as usize];
            }
            s / n as f64
        })
        .collect();
    means.sort_by(|a, b| a.total_cmp(b));
    let lo = means[((iters as f64) * 0.025) as usize];
    let hi = means[(((iters as f64) * 0.975) as usize).min(iters - 1)];
    (lo.exp(), hi.exp())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    Pass,
    Fail(String),
}

/// Improvement gate: CI lower bound must exceed `1 + min_improvement`.
pub fn improvement_gate(s: &Summary, min_improvement: f64) -> GateOutcome {
    let need = 1.0 + min_improvement;
    if s.ci95_low > need {
        GateOutcome::Pass
    } else {
        GateOutcome::Fail(format!(
            "speedup 95% CI lower bound {:.4} does not exceed {:.4} (geo-mean {:.4}, CI [{:.4}, {:.4}], n={})",
            s.ci95_low, need, s.geo_mean, s.ci95_low, s.ci95_high, s.n
        ))
    }
}

/// Guard (non-inferiority) gate: CI lower bound of speed ratio >= `1 - tolerance`.
pub fn guard_gate(s: &Summary, tolerance: f64) -> GateOutcome {
    let floor = 1.0 - tolerance;
    if s.ci95_low >= floor {
        GateOutcome::Pass
    } else {
        GateOutcome::Fail(format!(
            "speed-ratio 95% CI lower bound {:.4} is below the non-inferiority floor {:.4} (geo-mean {:.4}, CI [{:.4}, {:.4}], n={})",
            s.ci95_low, floor, s.geo_mean, s.ci95_low, s.ci95_high, s.n
        ))
    }
}

pub fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    match s.len() {
        0 => f64::NAN,
        n if n % 2 == 1 => s[n / 2],
        n => (s[n / 2 - 1] + s[n / 2]) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-noise in [-1, 1).
    fn noise(rng: &mut Rng) -> f64 {
        (rng.next() >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }

    fn synth(n: usize, true_speedup: f64, rel_noise: f64, seed: u64) -> Vec<f64> {
        let mut rng = Rng(seed);
        (0..n)
            .map(|_| {
                let b = 1000.0 * (1.0 + rel_noise * noise(&mut rng));
                let c = 1000.0 / true_speedup * (1.0 + rel_noise * noise(&mut rng));
                b / c
            })
            .collect()
    }

    #[test]
    fn pairs_validate() {
        assert!(paired_speedups(&[1.0], &[1.0, 2.0]).is_err());
        assert!(paired_speedups(&[0.0], &[1.0]).is_err());
        assert!(paired_speedups(&[f64::NAN], &[1.0]).is_err());
        assert_eq!(paired_speedups(&[4.0], &[2.0]).unwrap(), vec![2.0]);
    }

    #[test]
    fn t_and_bootstrap_agree() {
        for (sp, noise_lvl) in [(1.0, 0.05), (1.3, 0.05), (2.0, 0.1), (5.0, 0.03)] {
            let x = synth(40, sp, noise_lvl, 0x1234_5678_9abc);
            let s = summarize(&x).unwrap();
            let (blo, bhi) = bootstrap_ci(&x, 20_000, 99);
            assert!(
                (blo / s.ci95_low - 1.0).abs() < 0.02,
                "lower: t {} vs boot {blo}",
                s.ci95_low
            );
            assert!(
                (bhi / s.ci95_high - 1.0).abs() < 0.02,
                "upper: t {} vs boot {bhi}",
                s.ci95_high
            );
            assert!(s.ci95_low < s.geo_mean && s.geo_mean < s.ci95_high);
        }
    }

    #[test]
    fn ci_covers_truth_and_gate_decides() {
        let clear = summarize(&synth(40, 2.0, 0.05, 7)).unwrap();
        assert_eq!(improvement_gate(&clear, 0.10), GateOutcome::Pass);
        let none = summarize(&synth(40, 1.0, 0.05, 8)).unwrap();
        assert!(matches!(
            improvement_gate(&none, 0.10),
            GateOutcome::Fail(_)
        ));
        // True speedup right at the threshold: the CI must straddle it and reject.
        let edge = summarize(&synth(40, 1.10, 0.08, 9)).unwrap();
        assert!(edge.ci95_low < 1.10 && edge.ci95_high > 1.10, "{edge:?}");
        assert!(matches!(
            improvement_gate(&edge, 0.10),
            GateOutcome::Fail(_)
        ));
    }

    #[test]
    fn guard_requires_non_inferiority() {
        let same = summarize(&synth(40, 1.0, 0.01, 3)).unwrap();
        assert_eq!(guard_gate(&same, 0.05), GateOutcome::Pass);
        let slower = summarize(&synth(40, 0.8, 0.01, 4)).unwrap();
        assert!(matches!(guard_gate(&slower, 0.05), GateOutcome::Fail(_)));
        let noisy = summarize(&synth(40, 1.0, 0.5, 5)).unwrap();
        assert!(matches!(guard_gate(&noisy, 0.05), GateOutcome::Fail(_)));
    }

    #[test]
    fn median_works() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
    }
}
