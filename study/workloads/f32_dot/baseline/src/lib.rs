//! Baseline kernel: f32_dot. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "dot: length mismatch");
    let mut acc: f32 = 0.0;
    for i in 0..a.len() {
        let (x, y) = (a[i], b[i]);
        acc += x * y;
    }
    acc
}
