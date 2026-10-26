//! Baseline kernel: squared Euclidean distance over f32 (scalar reference).
//!
//! Frozen semantics: sum of (a[i]-b[i])^2 accumulated in f32; mismatched
//! lengths panic; empty inputs give 0.0.

pub fn sq_euclid(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sq_euclid: length mismatch");
    let mut s = 0.0f32;
    for (x, y) in a.iter().zip(b) {
        let d = x - y;
        s += d * d;
    }
    s
}
