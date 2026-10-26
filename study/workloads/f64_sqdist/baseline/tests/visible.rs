//! Visible tests: random data, lengths that are multiples of 8.
fn next(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

fn val(variant: u32, i: usize, n: usize, s: &mut u64) -> f64 {
    let v: f64 = match variant {
        1 if i + 1 == n => 1000.0,
        2 if i == 0 => 1000.0,
        3 => 0.75,
        4 => [0.0, -0.0, 1.0e150, -1.0e150, f64::from_bits(1), 1.0, -1.0][i % 7],
        _ => (((next(s) >> 40) as f64) / (1u64 << 24) as f64) * 2.0 - 1.0,
    };
    v
}

fn make(variant: u32, n: usize, seed: u64) -> Vec<f64> {
    let mut s = seed | 1;
    (0..n).map(|i| val(variant, i, n, &mut s)).collect()
}

fn reference(a: &[f64], b: &[f64]) -> (f64, f64) {
    let (mut v, mut s) = (0.0f64, 0.0f64);
    for i in 0..a.len() {
        let (x, y) = (a[i] as f64, b[i] as f64);
        let t: f64 = { let d = x - y; d * d };
        v += t;
        s += t.abs();
    }
    (v, s)
}

#[test]
fn multiples_of_eight_match_reference() {
    for variant in [0u32] {
        for n in [8usize, 16, 64, 256, 1024, 4096] {
        let a = make(variant, n, 1000 + n as u64);
        let b = make(variant ^ 8, n, 77_000 + n as u64);
        let got = kernel::sqdist(&a, &b) as f64;
        let (v, s) = reference(&a, &b);
        assert!(got.is_finite() && (got - v).abs() <= 1e-9 * s + 1e-300, "variant {variant} n {n}: got {got}, want {v} (scale {s})");
        }
    }
}
