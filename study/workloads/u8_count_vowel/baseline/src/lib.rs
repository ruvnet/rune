//! Baseline kernel: u8_count_vowel. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn count_vowel(a: &[u8]) -> u64 {
    let mut acc: u64 = 0u64;
    for i in 0..a.len() {
        let x = a[i];
        if matches!(x, b'a' | b'e' | b'i' | b'o' | b'u' | b'A' | b'E' | b'I' | b'O' | b'U') {
            acc += 1;
        }
    }
    acc
}
