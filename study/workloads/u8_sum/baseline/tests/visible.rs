//! Visible tests: random data, lengths that are multiples of 8.
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
        let t = x as u64;
        v = v.wrapping_add(t);
    }
    v
}

#[test]
fn multiples_of_eight_match_reference() {
    for variant in [0u32] {
        for n in [8usize, 16, 64, 256, 1024, 4096] {
        let a = make(variant, n, 1000 + n as u64);
        let got = kernel::sum(&a);
        assert_eq!(got, reference(&a), "variant {variant} n {n}");
        }
    }
}
