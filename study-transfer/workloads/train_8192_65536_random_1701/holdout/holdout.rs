fn gen(n: usize, range: u32, mut s: u64, near: bool) -> Vec<u16> {
 let mut a: Vec<u16> = (0..n).map(|_| { s ^= s << 13; s ^= s >> 7; s ^= s << 17; (s % range as u64) as u16 }).collect();
 if near { a.sort_unstable(); if n > 3 { a.swap(n/3, n/3+1); } } a
}
#[test] fn agrees() {
 for n in [0,1,3,7,31,129,1025,4099] { for range in [1,64,65536] { for seed in [11,19,43] {
 let a = gen(n,range,seed,false); let mut expected=a.clone(); expected.sort();
 assert_eq!(kernel::sort(&a), expected); } } }
}
