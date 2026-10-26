//! Baseline kernel: f32_maxabs. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn maxabs(a: &[f32]) -> f32 {
    let mut acc: f32 = 0.0;
    for &x in a {
        let t = x.abs();
        if t > acc {
            acc = t;
        }
    }
    acc
}
