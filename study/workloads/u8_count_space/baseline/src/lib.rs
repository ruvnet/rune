//! Baseline kernel: u8_count_space. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn count_space(a: &[u8]) -> u64 {
    let mut acc: u64 = 0u64;
    for &x in a {
        if matches!(x, b' ' | b'\t' | b'\n' | b'\r') {
            acc += 1;
        }
    }
    acc
}
