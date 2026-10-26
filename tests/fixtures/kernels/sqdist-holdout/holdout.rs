//! HIDDEN holdout (ADR 005). Never shown to the proposer. Frozen semantics:
//! f32 accumulation, relative tolerance 1e-4 (+1.2e-38 absolute floor) against
//! an f64 reference; mismatched lengths PANIC; empty inputs give exactly 0.0.
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

fn pair(n: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut s = seed;
    ((0..n).map(|_| lcg(&mut s)).collect(), (0..n).map(|_| lcg(&mut s)).collect())
}

#[test]
fn odd_and_tail_lengths() {
    for n in [0usize, 1, 2, 3, 7, 9, 15, 17, 1023, 1025] {
        let (a, b) = pair(n, 1000 + n as u64);
        let got = sqdist::sq_euclid(&a, &b);
        assert!(close(got, reference(&a, &b)), "n={n}: got {got}, want {}", reference(&a, &b));
    }
}

#[test]
fn empty_is_exactly_zero() {
    assert_eq!(sqdist::sq_euclid(&[], &[]), 0.0);
}

#[test]
fn last_element_alone_matters() {
    for n in [1usize, 7, 9, 1023] {
        let a = vec![0.0f32; n];
        let mut b = a.clone();
        b[n - 1] = 3.0;
        assert_eq!(sqdist::sq_euclid(&a, &b), 9.0, "n={n}");
    }
}

#[test]
#[should_panic(expected = "length mismatch")]
fn mismatched_lengths_panic() {
    sqdist::sq_euclid(&[1.0, 2.0, 3.0], &[1.0, 2.0]);
}

#[test]
fn subnormals_within_tolerance() {
    let a: Vec<f32> = (0..13).map(|i| f32::from_bits(1 + i)).collect();
    let b = vec![0.0f32; 13];
    assert!(close(sqdist::sq_euclid(&a, &b), reference(&a, &b)));
}

#[test]
fn large_magnitudes_within_tolerance() {
    for n in [9usize, 1023] {
        let a: Vec<f32> = (0..n).map(|i| 1.0e15 * (1.0 + (i % 5) as f32)).collect();
        let b: Vec<f32> = (0..n).map(|i| -1.0e15 * (1.0 + (i % 3) as f32)).collect();
        let got = sqdist::sq_euclid(&a, &b);
        assert!(got.is_finite() && close(got, reference(&a, &b)), "n={n}: {got}");
    }
}
