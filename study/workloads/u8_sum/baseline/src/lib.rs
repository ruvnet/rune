//! Baseline kernel: u8_sum. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn sum(a: &[u8]) -> u64 {
    let mut acc: u64 = 0u64;
    for i in 0..a.len() {
        let x = a[i];
        acc = acc.wrapping_add(x as u64);
    }
    acc
}
