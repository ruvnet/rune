//! Baseline kernel: f32_argmin. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn argmin(a: &[f32]) -> Option<usize> {
    if a.is_empty() {
        return None;
    }
    let mut m: f32 = a[0];
    for &x in a {
        if x < m {
            m = x;
        }
    }
    a.iter().position(|&x| x == m)
}
