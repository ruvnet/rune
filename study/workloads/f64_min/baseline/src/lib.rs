//! Baseline kernel: f64_min. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn min(a: &[f64]) -> f64 {
    let mut acc: f64 = f64::INFINITY;
    for i in 0..a.len() {
        let x = a[i];
        let t = x;
        if t < acc {
            acc = t;
        }
    }
    acc
}
