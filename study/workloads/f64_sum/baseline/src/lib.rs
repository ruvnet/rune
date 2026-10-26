//! Baseline kernel: f64_sum. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn sum(a: &[f64]) -> f64 {
    let mut acc: f64 = 0.0;
    for &x in a {
        acc += x;
    }
    acc
}
