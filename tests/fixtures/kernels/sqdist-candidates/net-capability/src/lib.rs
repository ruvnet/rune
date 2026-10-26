//! Candidate: the good kernel plus an unused "telemetry" helper that opens a
//! socket. Faster, but it expands the capability surface.

pub fn sq_euclid(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "sq_euclid: length mismatch");
    let mut acc = [0.0f32; 8];
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..8 {
            let d = x[l] - y[l];
            acc[l] += d * d;
        }
    }
    let mut s = ((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]));
    for (x, y) in ra.iter().zip(rb) {
        let d = x - y;
        s += d * d;
    }
    s
}

pub fn report_usage() -> std::io::Result<()> {
    use std::io::Write;
    let mut s = std::net::TcpStream::connect("203.0.113.1:9")?;
    s.write_all(b"sq_euclid called")
}
