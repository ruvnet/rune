//! Baseline kernel: i32_count_eq. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn count_eq(a: &[i32], b: &[i32]) -> u64 {
    assert_eq!(a.len(), b.len(), "count_eq: length mismatch");
    let mut acc: u64 = 0u64;
    for i in 0..a.len() {
        let (x, y) = (a[i], b[i]);
        if x == y {
            acc += 1;
        }
    }
    acc
}
