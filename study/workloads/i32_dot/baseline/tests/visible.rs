//! Visible tests: random data, lengths that are multiples of 8.
fn next(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

fn val(variant: u32, i: usize, n: usize, s: &mut u64) -> i32 {
    let v: i32 = match variant {
        1 if i + 1 == n => 1_000_000,
        2 if i == 0 => 1_000_000,
        3 => 7,
        4 => [0, -1, 1, i32::MIN, i32::MAX, 1 << 30, -(1 << 30)][i % 7],
        _ => ((next(s) >> 33) as i32) - (1 << 30),
    };
    v
}

fn make(variant: u32, n: usize, seed: u64) -> Vec<i32> {
    let mut s = seed | 1;
    (0..n).map(|i| val(variant, i, n, &mut s)).collect()
}

fn reference(a: &[i32], b: &[i32]) -> i64 {
    let mut v: i64 = 0i64;
    for i in 0..a.len() {
        let (x, y) = (a[i], b[i]);
        let t = (x as i64) * (y as i64);
        v = v.wrapping_add(t);
    }
    v
}

#[test]
fn multiples_of_eight_match_reference() {
    for variant in [0u32] {
        for n in [8usize, 16, 64, 256, 1024, 4096] {
        let a = make(variant, n, 1000 + n as u64);
        let b = make(variant ^ 8, n, 77_000 + n as u64);
        let got = kernel::dot(&a, &b);
        assert_eq!(got, reference(&a, &b), "variant {variant} n {n}");
        }
    }
}
