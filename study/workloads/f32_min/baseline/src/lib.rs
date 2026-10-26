//! Baseline kernel: f32_min. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn min(a: &[f32]) -> f32 {
    let mut acc: f32 = f32::INFINITY;
    for &x in a {
        let t = x;
        if t < acc {
            acc = t;
        }
    }
    acc
}
