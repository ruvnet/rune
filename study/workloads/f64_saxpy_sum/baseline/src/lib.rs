//! Baseline kernel: f64_saxpy_sum. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn saxpy_sum(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len(), "saxpy_sum: length mismatch");
    let mut acc: f64 = 0.0;
    for (&x, &y) in a.iter().zip(b) {
        acc += 0.5 * x + y;
    }
    acc
}
