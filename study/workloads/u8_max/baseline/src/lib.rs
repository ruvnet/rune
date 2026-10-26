//! Baseline kernel: u8_max. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn max(a: &[u8]) -> u8 {
    let mut acc: u8 = 0u8;
    for &x in a {
        let t = x;
        if t > acc {
            acc = t;
        }
    }
    acc
}
