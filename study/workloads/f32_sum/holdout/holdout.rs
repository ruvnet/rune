//! HIDDEN holdout (ADR 005). Never shown to the proposer. Lengths 0 (empty), 1, 7, 9,
//! odd and large; random, spike-at-end, spike-at-start, constant and edge-value data.
//! Float sums: |got - ref| <= tol * sum|terms| (tol 1e-4 f32, 1e-9 f64), ref in f64.
//! Everything else is exact.
fn next(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

fn val(variant: u32, i: usize, n: usize, s: &mut u64) -> f32 {
    let v: f32 = match variant {
        1 if i + 1 == n => 1000.0,
        2 if i == 0 => 1000.0,
        3 => 0.75,
        4 => [0.0, -0.0, 1.0e15, -1.0e15, f32::from_bits(1), 1.0, -1.0][i % 7],
        _ => (((next(s) >> 40) as f32) / (1u64 << 24) as f32) * 2.0 - 1.0,
    };
    v
}

fn make(variant: u32, n: usize, seed: u64) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n).map(|i| val(variant, i, n, &mut s)).collect()
}

fn reference(a: &[f32]) -> (f64, f64) {
    let (mut v, mut s) = (0.0f64, 0.0f64);
    for i in 0..a.len() {
        let x = a[i] as f64;
        let t: f64 = x;
        v += t;
        s += t.abs();
    }
    (v, s)
}

#[test]
fn tail_and_edge_cases_match_reference() {
    for variant in [0u32, 1, 2, 3, 4] {
        for n in [1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 33, 63, 65, 1023, 1025, 2040, 2041, 4099] {
        let a = make(variant, n, 1000 + n as u64);
        let got = kernel::sum(&a) as f64;
        let (v, s) = reference(&a);
        assert!(got.is_finite() && (got - v).abs() <= 1e-4 * s + 1.2e-38, "variant {variant} n {n}: got {got}, want {v} (scale {s})");
        }
    }
}

#[test]
fn empty_matches_reference() {
    {
        let got = kernel::sum(&Vec::<f32>::new()) as f64;
        let (v, s) = reference(&Vec::<f32>::new());
        assert!(got.is_finite() && (got - v).abs() <= 1e-4 * s + 1.2e-38, "empty input: got {got}, want {v} (scale {s})");
    }
}
