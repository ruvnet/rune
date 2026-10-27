pub fn sort(a: &[u16]) -> Vec<u16> {
    if a.is_empty() { return vec![]; }
    let lo = *a.iter().min().unwrap(); let hi = *a.iter().max().unwrap();
    let mut counts = vec![0usize; (hi as usize) - (lo as usize) + 1];
    for &x in a { counts[(x-lo) as usize] += 1; }
    let mut out = Vec::with_capacity(a.len());
    for (i, &n) in counts.iter().enumerate() { for _ in 0..n { out.push(lo + i as u16); } }
    out
}
