//! Baseline kernel: f32_sum. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn sum(a: &[f32]) -> f32 {
    let mut acc: f32 = 0.0;
    for i in 0..a.len() {
        let x = a[i];
        acc += x;
    }
    acc
}
