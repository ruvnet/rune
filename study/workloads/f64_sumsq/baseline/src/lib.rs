//! Baseline kernel: f64_sumsq. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn sumsq(a: &[f64]) -> f64 {
    let mut acc: f64 = 0.0;
    for &x in a {
        acc += x * x;
    }
    acc
}
