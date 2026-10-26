//! Baseline kernel: f64_sqdist. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn sqdist(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len(), "sqdist: length mismatch");
    let mut acc: f64 = 0.0;
    for i in 0..a.len() {
        let (x, y) = (a[i], b[i]);
        acc += { let d = x - y; d * d };
    }
    acc
}
