//! Visible tests: lengths that are multiples of 8, f64 reference, rel. tol 1e-4.
fn lcg(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
}

fn reference(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum()
}

fn close(got: f32, want: f64) -> bool {
    (got as f64 - want).abs() <= 1e-4 * want.abs() + 1.2e-38
}

#[test]
fn multiples_of_eight_match_reference() {
    let mut s = 42u64;
    for n in [8usize, 16, 64, 256, 1024, 4096] {
        let a: Vec<f32> = (0..n).map(|_| lcg(&mut s)).collect();
        let b: Vec<f32> = (0..n).map(|_| lcg(&mut s)).collect();
        assert!(close(sqdist::sq_euclid(&a, &b), reference(&a, &b)), "n={n}");
    }
}

#[test]
fn identical_vectors_are_zero() {
    let a = vec![0.5f32; 64];
    assert_eq!(sqdist::sq_euclid(&a, &a), 0.0);
}
