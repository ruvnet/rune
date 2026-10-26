//! Baseline kernel: i32_min. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn min(a: &[i32]) -> i32 {
    let mut acc: i32 = i32::MAX;
    for i in 0..a.len() {
        let x = a[i];
        let t = x;
        if t < acc {
            acc = t;
        }
    }
    acc
}
