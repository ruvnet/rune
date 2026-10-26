//! What the loading machine offers: triple, ABI and CPU features.

use std::collections::BTreeSet;

/// Parse CPU feature names from `/proc/cpuinfo` text: the `flags` line on x86
/// and the `Features` line on arm. Pure so it can be tested with fixtures.
pub fn parse_cpuinfo(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key == "flags" || key == "Features" {
            out.extend(rest.split_whitespace().map(normalize_feature));
        }
    }
    out
}

/// Alias arm's `asimd` to `neon` so descriptors can use the familiar name.
fn normalize_feature(f: &str) -> String {
    match f {
        "asimd" => "neon".to_owned(),
        other => other.to_owned(),
    }
}

/// CPU features of this machine. Empty when `/proc/cpuinfo` is unreadable, so
/// a feature requirement then fails closed.
pub fn detect_host_features() -> BTreeSet<String> {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|t| parse_cpuinfo(&t))
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub triple: String,
    pub abi: String,
    pub features: BTreeSet<String>,
}

impl Host {
    pub fn detect() -> Self {
        Host {
            triple: host_triple(),
            abi: host_abi().to_owned(),
            features: detect_host_features(),
        }
    }
}

/// Triple of the machine this binary runs on (Linux gnu/musl; other OSes are
/// reported as `<arch>-unknown-<os>`).
pub fn host_triple() -> String {
    let arch = std::env::consts::ARCH;
    if cfg!(target_os = "linux") {
        let env = if cfg!(target_env = "musl") {
            "musl"
        } else {
            "gnu"
        };
        format!("{arch}-unknown-linux-{env}")
    } else {
        format!("{arch}-unknown-{}", std::env::consts::OS)
    }
}

pub fn host_abi() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "sysv-x86_64",
        "aarch64" => "aapcs64",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_x86_flags_and_arm_features() {
        let x = parse_cpuinfo("processor : 0\nflags\t\t: fpu sse2 avx2\n");
        assert!(x.contains("avx2") && x.contains("sse2"));
        let a = parse_cpuinfo("Features\t: fp asimd aes\n");
        assert!(a.contains("neon") && !a.contains("asimd"));
    }

    #[test]
    fn unreadable_or_empty_is_empty() {
        assert!(parse_cpuinfo("").is_empty());
    }

    #[test]
    fn detects_something_on_linux() {
        if cfg!(target_os = "linux") {
            assert!(!Host::detect().features.is_empty());
        }
    }
}
