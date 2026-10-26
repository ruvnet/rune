//! Baseline kernel: i32_dot. Frozen semantics are in the workload contract; no unsafe, no dependencies.

pub fn dot(a: &[i32], b: &[i32]) -> i64 {
    assert_eq!(a.len(), b.len(), "dot: length mismatch");
    let mut acc: i64 = 0i64;
    for (&x, &y) in a.iter().zip(b) {
        acc = acc.wrapping_add((x as i64) * (y as i64));
    }
    acc
}
