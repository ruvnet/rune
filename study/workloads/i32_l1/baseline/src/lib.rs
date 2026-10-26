//! Baseline kernel: i32_l1. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn l1(a: &[i32], b: &[i32]) -> u64 {
    assert_eq!(a.len(), b.len(), "l1: length mismatch");
    let mut acc: u64 = 0u64;
    for (&x, &y) in a.iter().zip(b) {
        acc = acc.wrapping_add((x as i64 - y as i64).unsigned_abs());
    }
    acc
}
