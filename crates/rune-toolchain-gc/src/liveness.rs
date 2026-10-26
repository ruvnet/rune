//! Process liveness through `/proc` (Linux, no unsafe, no libc). A pid alone
//! is not enough because pids are reused, so leases also record the process
//! start time and a lease is live only if both still match.

use std::fs;

pub trait Liveness {
    fn is_alive(&self, pid: u32, start: u64) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcLiveness;

/// `(state, starttime)` parsed from `/proc/<pid>/stat`; the comm field may
/// contain spaces and parentheses, so parse after the last `)`.
pub fn proc_stat(pid: u32) -> Option<(char, u64)> {
    let s = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &s[s.rfind(')')? + 1..];
    let mut f = rest.split_whitespace();
    let state = f.next()?.chars().next()?;
    // fields after comm: state(3) ... starttime is field 22 => index 19 here.
    let start = f.nth(18)?.parse().ok()?;
    Some((state, start))
}

pub fn start_time(pid: u32) -> Option<u64> {
    proc_stat(pid).map(|(_, s)| s)
}

impl Liveness for ProcLiveness {
    fn is_alive(&self, pid: u32, start: u64) -> bool {
        match proc_stat(pid) {
            Some((state, st)) => state != 'Z' && state != 'X' && st == start,
            None => false,
        }
    }
}
