//! Baseline kernel: u8_count_digit. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn count_digit(a: &[u8]) -> u64 {
    let mut acc: u64 = 0u64;
    for i in 0..a.len() {
        let x = a[i];
        if matches!(x, b'0'..=b'9') {
            acc += 1;
        }
    }
    acc
}
