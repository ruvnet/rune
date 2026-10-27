pub fn sort(a: &[u16]) -> Vec<u16> {
    let mut v = a.to_vec(); let mut tmp = vec![0u16; a.len()];
    for shift in [0u32, 8] {
        let mut counts = [0usize;256];
        for &x in &v { counts[((x >> shift) & 255) as usize] += 1; }
        let mut pos = [0usize;256]; for i in 1..256 { pos[i] = pos[i-1] + counts[i-1]; }
        for &x in &v { let b = ((x >> shift) & 255) as usize; tmp[pos[b]] = x; pos[b] += 1; }
        std::mem::swap(&mut v, &mut tmp);
    }
    v
}
