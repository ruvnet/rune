//! Measurement environment: machine load and CPU choice for pinning.

use serde::{Deserialize, Serialize};
use std::fs;
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Load {
    pub loadavg_1m: f64,
    pub cpus: usize,
}

pub fn load() -> Load {
    let l: f64 = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|x| x.parse().ok()))
        .unwrap_or(-1.0);
    Load {
        loadavg_1m: l,
        cpus: std::thread::available_parallelism().map_or(1, |n| n.get()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pinning {
    pub core: usize,
    /// Idle fraction of that core over the 250 ms before measurement.
    pub idle_fraction: f64,
}

fn allowed_cpus() -> Vec<usize> {
    let list = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("Cpus_allowed_list:")
                    .map(|v| v.trim().to_string())
            })
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for part in list.split(',') {
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
                    out.extend(a..=b);
                }
            }
            None => out.extend(part.trim().parse::<usize>()),
        }
    }
    out
}

fn cpu_times() -> Vec<(usize, u64, u64)> {
    let s = fs::read_to_string("/proc/stat").unwrap_or_default();
    s.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let name = it.next()?;
            let n: usize = name.strip_prefix("cpu")?.parse().ok()?;
            let v: Vec<u64> = it.filter_map(|x| x.parse().ok()).collect();
            if v.len() < 5 {
                return None;
            }
            Some((n, v[3] + v[4], v.iter().sum()))
        })
        .collect()
}

/// The allowed core that was idlest over a short window, or None if /proc is
/// unreadable. Pinning both sides to one quiet core cancels core-to-core
/// variation; it does not remove interference from other processes.
pub fn quietest_core() -> Option<Pinning> {
    let allowed = allowed_cpus();
    let a = cpu_times();
    sleep(Duration::from_millis(250));
    let b = cpu_times();
    a.iter()
        .filter(|(n, _, _)| allowed.contains(n))
        .filter_map(|(n, idle0, tot0)| {
            let (_, idle1, tot1) = b.iter().find(|(m, _, _)| m == n)?;
            let dt = tot1.saturating_sub(*tot0);
            (dt > 0).then(|| Pinning {
                core: *n,
                idle_fraction: idle1.saturating_sub(*idle0) as f64 / dt as f64,
            })
        })
        .max_by(|x, y| {
            x.idle_fraction
                .total_cmp(&y.idle_fraction)
                .then(x.core.cmp(&y.core))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_load_and_core() {
        assert!(load().cpus >= 1);
        let p = quietest_core().expect("proc readable");
        assert!((0.0..=1.0).contains(&p.idle_fraction));
    }
}
