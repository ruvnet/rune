//! Paired, interleaved benchmark measurement.

use crate::report::MetricResult;
use crate::stats::{self, GateOutcome};
use rune_core::contract::{GuardBench, Reject};

#[derive(Debug, Clone)]
pub enum MetricKind {
    Improvement { min_improvement: f64 },
    Guard { tolerance: f64 },
}

#[derive(Debug, Clone)]
pub struct Metric {
    pub name: String,
    pub cmd: Vec<String>,
    pub kind: MetricKind,
}

pub fn metrics_for(primary: &[String], min_improvement: f64, guards: &[GuardBench]) -> Vec<Metric> {
    let mut v = vec![Metric {
        name: "primary".into(),
        cmd: primary.to_vec(),
        kind: MetricKind::Improvement { min_improvement },
    }];
    v.extend(guards.iter().map(|g| Metric {
        name: g.name.clone(),
        cmd: g.bench_cmd.clone(),
        kind: MetricKind::Guard {
            tolerance: g.tolerance,
        },
    }));
    v
}

/// Parse the last non-empty stdout line as `{"ns_per_op": <positive f64>}`.
pub fn parse_ns(stdout: &str) -> Result<f64, String> {
    let line = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or("bench printed nothing")?;
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("bench output not JSON ({e}): {line}"))?;
    let ns = v
        .get("ns_per_op")
        .and_then(|x| x.as_f64())
        .ok_or_else(|| format!("no numeric ns_per_op in: {line}"))?;
    if ns.is_finite() && ns > 0.0 {
        Ok(ns)
    } else {
        Err(format!("ns_per_op not positive and finite: {ns}"))
    }
}

/// Run `run(is_candidate)` for warmup + sample pairs, alternating which side
/// goes first on every pair, and summarise.
pub fn measure(
    metric: &Metric,
    warmup: usize,
    samples: usize,
    mut run: impl FnMut(bool) -> Result<f64, Reject>,
) -> Result<MetricResult, Reject> {
    let (mut base, mut cand) = (Vec::new(), Vec::new());
    for p in 0..warmup + samples {
        let cand_first = p % 2 == 1;
        let (a, b) = if cand_first {
            let c = run(true)?;
            let b = run(false)?;
            (b, c)
        } else {
            let b = run(false)?;
            let c = run(true)?;
            (b, c)
        };
        if p >= warmup {
            base.push(a);
            cand.push(b);
        }
    }
    let bad = |e: String| Reject::MissingEvidence(format!("{}: {e}", metric.name));
    let sp = stats::paired_speedups(&base, &cand).map_err(bad)?;
    let s = stats::summarize(&sp).map_err(bad)?;
    let (blo, bhi) = stats::bootstrap_ci(&sp, 10_000, 0x5EED_0005);
    let (outcome, role, threshold) = match &metric.kind {
        MetricKind::Improvement { min_improvement } => (
            stats::improvement_gate(&s, *min_improvement),
            "improvement",
            format!("CI lower bound > {:.4}", 1.0 + min_improvement),
        ),
        MetricKind::Guard { tolerance } => (
            stats::guard_gate(&s, *tolerance),
            "guard",
            format!("CI lower bound >= {:.4}", 1.0 - tolerance),
        ),
    };
    let passed = outcome == GateOutcome::Pass;
    Ok(MetricResult {
        name: metric.name.clone(),
        role: role.into(),
        pairs: s.n,
        warmup_pairs: warmup,
        geo_mean_speedup: s.geo_mean,
        arith_mean_speedup: s.arith_mean,
        ci95_low: s.ci95_low,
        ci95_high: s.ci95_high,
        bootstrap_ci95_low: blo,
        bootstrap_ci95_high: bhi,
        baseline_median_ns: stats::median(&base),
        candidate_median_ns: stats::median(&cand),
        threshold,
        passed,
        baseline_ns: base,
        candidate_ns: cand,
    })
}

pub fn gate_detail(metric: &Metric, m: &MetricResult) -> String {
    let s = stats::Summary {
        n: m.pairs,
        geo_mean: m.geo_mean_speedup,
        arith_mean: m.arith_mean_speedup,
        ci95_low: m.ci95_low,
        ci95_high: m.ci95_high,
        sd_log: 0.0,
    };
    let out = match &metric.kind {
        MetricKind::Improvement { min_improvement } => {
            stats::improvement_gate(&s, *min_improvement)
        }
        MetricKind::Guard { tolerance } => stats::guard_gate(&s, *tolerance),
    };
    match out {
        GateOutcome::Pass => "pass".into(),
        GateOutcome::Fail(d) => format!("{}: {d}", metric.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bench_lines() {
        assert_eq!(parse_ns("warming\n{\"ns_per_op\": 12.5}\n").unwrap(), 12.5);
        assert!(parse_ns("").is_err());
        assert!(parse_ns("{\"ns_per_op\": 0}").is_err());
        assert!(parse_ns("{\"ns_per_op\": \"x\"}").is_err());
        assert!(parse_ns("nope").is_err());
    }

    #[test]
    fn interleaves_and_alternates_order() {
        let m = Metric {
            name: "t".into(),
            cmd: vec![],
            kind: MetricKind::Improvement {
                min_improvement: 0.1,
            },
        };
        let mut calls = Vec::new();
        let r = measure(&m, 2, 30, |cand| {
            calls.push(cand);
            Ok(if cand { 50.0 } else { 100.0 })
        })
        .ok()
        .unwrap();
        assert_eq!(r.pairs, 30);
        assert_eq!(calls.len(), 64);
        // pair 0: base,cand ; pair 1: cand,base ; ...
        assert_eq!(&calls[..4], &[false, true, true, false]);
        assert!((r.geo_mean_speedup - 2.0).abs() < 1e-9);
        assert!(r.passed);
    }
}
