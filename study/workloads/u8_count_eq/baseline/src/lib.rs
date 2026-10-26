//! Baseline kernel: u8_count_eq. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn count_eq(a: &[u8], b: &[u8]) -> u64 {
    assert_eq!(a.len(), b.len(), "count_eq: length mismatch");
    let mut acc: u64 = 0u64;
    for (&x, &y) in a.iter().zip(b) {
        if x == y {
            acc += 1;
        }
    }
    acc
}
