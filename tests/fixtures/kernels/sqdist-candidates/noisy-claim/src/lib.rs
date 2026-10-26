//! Candidate: only the last eighth of the vector goes through the 8-lane path;
//! the first seven eighths stay on the original dependent-add chain. Correct,
//! and faster, but by a margin (about 7% on the dev machine) short of the 10% threshold: the claimed
//! gain is too marginal for a 95% CI lower bound to clear 1.10.

pub fn sq_euclid(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sq_euclid: length mismatch");
    let split = ((a.len() * 7 / 8) / 8) * 8;
    let (ha, ta) = a.split_at(split);
    let (hb, tb) = b.split_at(split);
    let mut s = 0.0f32;
    for (x, y) in ha.iter().zip(hb) {
        let d = x - y;
        s += d * d;
    }
    let mut acc = [0.0f32; 8];
    let (ca, cb) = (ta.chunks_exact(8), tb.chunks_exact(8));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..8 {
            let d = x[l] - y[l];
            acc[l] += d * d;
        }
    }
    s += ((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]));
    for (x, y) in ra.iter().zip(rb) {
        let d = x - y;
        s += d * d;
    }
    s
}
