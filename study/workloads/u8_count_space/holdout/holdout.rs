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

fn val(variant: u32, i: usize, n: usize, s: &mut u64) -> u8 {
    let v: u8 = match variant {
        1 if i + 1 == n => 0xFF,
        2 if i == 0 => 0xFF,
        3 => 0xFF,
        4 => [0u8, 255, b'0', b'9', b'a', b'U', b' ', b'\n', 0xA5][i % 9],
        _ => (next(s) >> 56) as u8,
    };
    v
}

fn make(variant: u32, n: usize, seed: u64) -> Vec<u8> {
    let mut s = seed | 1;
    (0..n).map(|i| val(variant, i, n, &mut s)).collect()
}

fn reference(a: &[u8]) -> u64 {
    let mut v: u64 = 0u64;
    for i in 0..a.len() {
        let x = a[i];
        if matches!(x, b' ' | b'\t' | b'\n' | b'\r') {
            v += 1;
        }
    }
    v
}

#[test]
fn tail_and_edge_cases_match_reference() {
    for variant in [0u32, 1, 2, 3, 4] {
        for n in [1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 33, 63, 65, 1023, 1025, 2040, 2041, 4099, 65537] {
        let a = make(variant, n, 1000 + n as u64);
        let got = kernel::count_space(&a);
        assert_eq!(got, reference(&a), "variant {variant} n {n}");
        }
    }
}

#[test]
fn empty_matches_reference() {
    {
        let got = kernel::count_space(&Vec::<u8>::new());
        assert_eq!(got, reference(&Vec::<u8>::new()), "empty input");
    }
}
