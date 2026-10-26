//! Baseline kernel: f32_sqdist. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn sqdist(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sqdist: length mismatch");
    let mut acc: f32 = 0.0;
    for (&x, &y) in a.iter().zip(b) {
        acc += { let d = x - y; d * d };
    }
    acc
}
