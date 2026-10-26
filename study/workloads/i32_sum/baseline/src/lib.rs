//! Baseline kernel: i32_sum. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn sum(a: &[i32]) -> i64 {
    let mut acc: i64 = 0i64;
    for &x in a {
        acc = acc.wrapping_add(x as i64);
    }
    acc
}
