use std::{hint::black_box, time::Instant};
fn gen(n: usize, range: u32, mut s: u64, near: bool) -> Vec<u16> {
 let mut a: Vec<u16> = (0..n).map(|_| { s ^= s << 13; s ^= s >> 7; s ^= s << 17; (s % range as u64) as u16 }).collect();
 if near { a.sort_unstable(); if n > 3 { a.swap(n/3, n/3+1); } } a
}
fn main() {
 let n = std::env::args().nth(1).unwrap().parse::<usize>().unwrap();
 let a = gen(n,32768,5772,false);
 let batch = |iters: u64| { let t=Instant::now(); for _ in 0..iters { black_box(kernel::sort(black_box(&a))); }
 t.elapsed().as_nanos() as f64 / iters as f64 };
 let mut iters=1u64; while iters < (1<<24) && batch(iters)* (iters as f64) < 200000.0 { iters*=2; }
 let mut xs: Vec<f64> = (0..5).map(|_|batch(iters)).collect(); xs.sort_by(f64::total_cmp);
 println!("{{\"ns_per_op\":{:.4}}}",xs[2]);
}
