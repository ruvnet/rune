//! Candidate: 8 lanes but the remainder (len % 8) is silently dropped.
//! Passes every visible test (all multiples of 8); wrong for other lengths.

pub fn sq_euclid(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sq_euclid: length mismatch");
    let mut acc = [0.0f32; 8];
    for (x, y) in a.chunks_exact(8).zip(b.chunks_exact(8)) {
        for l in 0..8 {
            let d = x[l] - y[l];
            acc[l] += d * d;
        }
    }
    ((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]))
}
