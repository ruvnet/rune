//! Baseline kernel: u8_hist. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn hist(a: &[u8]) -> [u64; 256] {
    let mut h = [0u64; 256];
    for &x in a {
        h[x as usize] += 1;
    }
    h
}
