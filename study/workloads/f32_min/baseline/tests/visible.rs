//! Visible tests: random data, lengths that are multiples of 8.
fn next(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

fn val(variant: u32, i: usize, n: usize, s: &mut u64) -> f32 {
    let v: f32 = match variant {
        1 if i + 1 == n => -1000.0,
        2 if i == 0 => -1000.0,
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
    let (mut v, mut s) = (f64::INFINITY, 0.0f64);
    for i in 0..a.len() {
        let x = a[i] as f64;
        let t: f64 = x;
        v = v.min(t);
        s += t.abs();
    }
    (v, s)
}

#[test]
fn multiples_of_eight_match_reference() {
    for variant in [0u32] {
        for n in [8usize, 16, 64, 256, 1024, 4096] {
        let a = make(variant, n, 1000 + n as u64);
        let got = kernel::min(&a) as f64;
        let (v, _) = reference(&a);
        assert_eq!(got, v, "variant {variant} n {n}");
        }
    }
}
