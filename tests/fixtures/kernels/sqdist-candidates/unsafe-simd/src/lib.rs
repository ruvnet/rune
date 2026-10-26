//! Candidate: faster via SSE intrinsics, which requires new `unsafe`.

#[cfg(target_arch = "x86_64")]
pub fn sq_euclid(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    assert_eq!(a.len(), b.len(), "sq_euclid: length mismatch");
    let n = a.len();
    let mut i = 0;
    let mut s;
    unsafe {
        let (mut v0, mut v1) = (_mm_setzero_ps(), _mm_setzero_ps());
        while i + 8 <= n {
            let d0 = _mm_sub_ps(_mm_loadu_ps(a.as_ptr().add(i)), _mm_loadu_ps(b.as_ptr().add(i)));
            let d1 = _mm_sub_ps(_mm_loadu_ps(a.as_ptr().add(i + 4)), _mm_loadu_ps(b.as_ptr().add(i + 4)));
            v0 = _mm_add_ps(v0, _mm_mul_ps(d0, d0));
            v1 = _mm_add_ps(v1, _mm_mul_ps(d1, d1));
            i += 8;
        }
        let mut lanes = [0.0f32; 4];
        _mm_storeu_ps(lanes.as_mut_ptr(), _mm_add_ps(v0, v1));
        s = (lanes[0] + lanes[2]) + (lanes[1] + lanes[3]);
    }
    for j in i..n {
        let d = a[j] - b[j];
        s += d * d;
    }
    s
}

#[cfg(not(target_arch = "x86_64"))]
pub fn sq_euclid(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sq_euclid: length mismatch");
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
