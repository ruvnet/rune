//! Baseline kernel: f64_l1. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn l1(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len(), "l1: length mismatch");
    let mut acc: f64 = 0.0;
    for (&x, &y) in a.iter().zip(b) {
        acc += (x - y).abs();
    }
    acc
}
