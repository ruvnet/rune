//! Baseline kernel: u8_find_first. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub const NEEDLE: u8 = 0xA5;

pub fn find_first(a: &[u8]) -> Option<usize> {
    for i in 0..a.len() {
        if a[i] == NEEDLE {
            return Some(i);
        }
    }
    None
}
