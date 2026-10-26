//! Baseline kernel: f64_maxabs. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn maxabs(a: &[f64]) -> f64 {
    let mut acc: f64 = 0.0;
    for i in 0..a.len() {
        let x = a[i];
        let t = x.abs();
        if t > acc {
            acc = t;
        }
    }
    acc
}
