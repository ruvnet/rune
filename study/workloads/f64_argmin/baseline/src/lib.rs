//! Baseline kernel: f64_argmin. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn argmin(a: &[f64]) -> Option<usize> {
    if a.is_empty() {
        return None;
    }
    let mut m: f64 = a[0];
    for &x in a {
        if x < m {
            m = x;
        }
    }
    a.iter().position(|&x| x == m)
}
