//! Baseline kernel: f64_dot. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn dot(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len(), "dot: length mismatch");
    let mut acc: f64 = 0.0;
    for (&x, &y) in a.iter().zip(b) {
        acc += x * y;
    }
    acc
}
