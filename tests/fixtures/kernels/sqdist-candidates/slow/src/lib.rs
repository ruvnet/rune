//! Candidate: "higher precision" f64 accumulation. Correct, but the dependency
//! chain is unchanged and widening adds work: no meaningful improvement.

pub fn sq_euclid(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sq_euclid: length mismatch");
    let mut s = 0.0f64;
    for (x, y) in a.iter().zip(b) {
        let d = *x as f64 - *y as f64;
        s += d * d;
    }
    s as f32
}
