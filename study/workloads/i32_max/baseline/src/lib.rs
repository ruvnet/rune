//! Baseline kernel: i32_max. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn max(a: &[i32]) -> i32 {
    let mut acc: i32 = i32::MIN;
    for i in 0..a.len() {
        let x = a[i];
        let t = x;
        if t > acc {
            acc = t;
        }
    }
    acc
}
