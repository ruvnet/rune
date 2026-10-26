//! Prints one JSON line {"ns_per_op": median over batches} for an input length
//! given as the first argument (default 1024).
#![allow(unused_mut)]
use std::hint::black_box;
use std::time::Instant;

fn gen(n: usize, mut s: u64) -> Vec<u8> {
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 56) as u8
        })
        .collect()
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let mut a = gen(n, 0x9E37_79B9_7F4A_7C15);
    let batch = |iters: u64| {
        let t = Instant::now();
        for _ in 0..iters {
            black_box(kernel::max(black_box(&a)));
        }
        t.elapsed().as_nanos() as f64 / iters as f64
    };
    let mut iters = 1u64;
    while iters < (1 << 28) && batch(iters) * (iters as f64) < 2_000_000.0 {
        iters *= 2;
    }
    let mut per: Vec<f64> = (0..15).map(|_| batch(iters)).collect();
    per.sort_by(|x, y| x.total_cmp(y));
    println!("{{\"ns_per_op\": {:.4}}}", per[per.len() / 2]);
}
