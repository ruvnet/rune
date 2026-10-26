//! Baseline kernel: u8_hamming. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn hamming(a: &[u8], b: &[u8]) -> u64 {
    assert_eq!(a.len(), b.len(), "hamming: length mismatch");
    let mut acc: u64 = 0u64;
    for i in 0..a.len() {
        let (x, y) = (a[i], b[i]);
        acc = acc.wrapping_add((x ^ y).count_ones() as u64);
    }
    acc
}
