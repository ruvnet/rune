//! Best-effort, REPORTED containment for candidate commands (ADR 007).
//!
//! Layers are real OS mechanisms applied only when the host supports them:
//! `timeout -k` (wall time), `prlimit` (address space, CPU seconds), `unshare
//! -rn` (empty network namespace), `taskset` (CPU pinning, for measurement) and
//! a scrubbed environment. None of this is a security sandbox; the report lists
//! exactly which layers were applied and which were unavailable.

use rune_core::Error;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub timeout: bool,
    pub prlimit: bool,
    pub netns: bool,
    pub taskset: bool,
    /// Why a layer is unavailable.
    pub notes: Vec<String>,
}

fn quiet(prog: &str, args: &[&str]) -> Option<std::process::Output> {
    Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()
}

fn ok(prog: &str, args: &[&str]) -> bool {
    quiet(prog, args).is_some_and(|o| o.status.success())
}

impl Capabilities {
    /// Probe once per process by actually running each mechanism.
    pub fn probe() -> &'static Capabilities {
        static CAPS: OnceLock<Capabilities> = OnceLock::new();
        CAPS.get_or_init(|| {
            let mut c = Capabilities {
                timeout: ok("timeout", &["-k", "1", "5", "true"]),
                prlimit: ok("prlimit", &["--as=4000000000", "--cpu=10", "true"]),
                taskset: ok("taskset", &["-c", "0", "true"]),
                ..Default::default()
            };
            // The namespace must really be empty, not merely creatable.
            c.netns = quiet("unshare", &["-rn", "cat", "/proc/net/dev"]).is_some_and(|o| {
                o.status.success()
                    && String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .skip(2)
                        .all(|l| l.trim_start().starts_with("lo:"))
            });
            for (flag, name, why) in [
                (c.timeout, "timeout", "`timeout` not runnable"),
                (c.prlimit, "prlimit", "`prlimit` not runnable"),
                (
                    c.netns,
                    "netns",
                    "`unshare -rn` unavailable (user namespaces disabled?)",
                ),
                (c.taskset, "taskset", "`taskset` not runnable"),
            ] {
                if !flag {
                    c.notes.push(format!("{name}: {why}"));
                }
            }
            c
        })
    }
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub wall_secs: u64,
    pub mem_mb: u64,
    pub cpu_secs: u64,
    pub deny_net: bool,
    pub pin_core: Option<usize>,
}

/// The only environment a candidate command sees.
#[derive(Debug, Clone)]
pub struct SandboxEnv {
    pub home: PathBuf,
    pub tmpdir: PathBuf,
    pub cargo_home: PathBuf,
    pub rustup_home: Option<PathBuf>,
    pub path: String,
}

impl SandboxEnv {
    /// `home` is a per-run directory; `cargo_home` is read from the caller
    /// (an empty dir works for dependency-free fixtures).
    pub fn new(home: &Path, cargo_home: &Path) -> Self {
        let rustup_home = std::env::var_os("RUSTUP_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".rustup")))
            .filter(|p| p.is_dir());
        SandboxEnv {
            home: home.to_path_buf(),
            tmpdir: home.join("tmp"),
            cargo_home: cargo_home.to_path_buf(),
            rustup_home,
            path: std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub wall: Duration,
    /// Isolation layers actually applied to this run.
    pub layers: Vec<String>,
}

impl RunResult {
    pub fn success(&self) -> bool {
        !self.timed_out && self.code == Some(0)
    }
}

/// Run `cmd` in `cwd` under the requested limits. Wall-time enforcement fails
/// closed: without `timeout` nothing runs. Other missing layers are skipped
/// and simply absent from `layers`.
pub fn run_limited(
    cmd: &[String],
    cwd: &Path,
    limits: &Limits,
    env: &SandboxEnv,
) -> Result<RunResult, Error> {
    if cmd.is_empty() {
        return Err(Error::Policy("empty command".into()));
    }
    let caps = Capabilities::probe();
    if !caps.timeout {
        return Err(Error::Policy(
            "wall-time layer unavailable: refusing to run unbounded".into(),
        ));
    }
    let mut layers = vec!["timeout".to_string(), "env-scrub".to_string()];
    let mut argv: Vec<String> = vec![
        "timeout".into(),
        "-k".into(),
        "5".into(),
        limits.wall_secs.to_string(),
    ];
    if let (Some(core), true) = (limits.pin_core, caps.taskset) {
        argv.extend(["taskset".into(), "-c".into(), core.to_string()]);
        layers.push(format!("cpu-pin(core {core})"));
    }
    if caps.prlimit {
        argv.push("prlimit".into());
        argv.push(format!("--as={}", limits.mem_mb.saturating_mul(1 << 20)));
        argv.push(format!("--cpu={}", limits.cpu_secs));
        layers.push("rlimit-as".into());
        layers.push("rlimit-cpu".into());
    }
    if limits.deny_net && caps.netns {
        argv.extend(["unshare".into(), "-rn".into()]);
        layers.push("netns".into());
    }
    argv.extend(cmd.iter().cloned());

    let _ = std::fs::create_dir_all(&env.tmpdir);
    let mut c = Command::new(&argv[0]);
    c.args(&argv[1..])
        .current_dir(cwd)
        .env_clear()
        .env("PATH", &env.path)
        .env("HOME", &env.home)
        .env("TMPDIR", &env.tmpdir)
        .env("CARGO_HOME", &env.cargo_home)
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_TERM_COLOR", "never")
        .stdin(Stdio::null());
    if let Some(r) = &env.rustup_home {
        c.env("RUSTUP_HOME", r);
    }
    let start = Instant::now();
    let out = c.output().map_err(|e| Error::io(cwd, e))?;
    let code = out.status.code();
    Ok(RunResult {
        code,
        timed_out: matches!(code, Some(124) | Some(137)),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        wall: start.elapsed(),
        layers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(d: &Path) -> SandboxEnv {
        SandboxEnv::new(d, &d.join("cargo-home"))
    }

    fn limits() -> Limits {
        Limits {
            wall_secs: 10,
            mem_mb: 2048,
            cpu_secs: 10,
            deny_net: true,
            pin_core: None,
        }
    }

    #[test]
    fn env_is_scrubbed() {
        std::env::set_var("RUNE_TEST_SECRET", "hunter2");
        let d = tempfile::tempdir().unwrap();
        let r = run_limited(&["env".into()], d.path(), &limits(), &env(d.path())).unwrap();
        assert!(r.success());
        assert!(!r.stdout.contains("hunter2"));
        assert!(r.stdout.contains(&format!("HOME={}", d.path().display())));
        assert!(!r.stdout.contains("SSH_AUTH_SOCK"));
    }

    #[test]
    fn wall_time_kills() {
        let d = tempfile::tempdir().unwrap();
        let mut l = limits();
        l.wall_secs = 1;
        let r = run_limited(&["sleep".into(), "30".into()], d.path(), &l, &env(d.path())).unwrap();
        assert!(r.timed_out, "{r:?}");
        assert!(r.wall < Duration::from_secs(10));
    }

    #[test]
    fn layers_match_capabilities() {
        let d = tempfile::tempdir().unwrap();
        let r = run_limited(&["true".into()], d.path(), &limits(), &env(d.path())).unwrap();
        let caps = Capabilities::probe();
        assert_eq!(r.layers.iter().any(|l| l == "netns"), caps.netns);
        assert_eq!(r.layers.iter().any(|l| l == "rlimit-as"), caps.prlimit);
        assert!(r.layers.iter().any(|l| l == "timeout"));
    }

    #[test]
    fn network_is_denied() {
        if !Capabilities::probe().netns {
            eprintln!("netns unavailable here; skipping");
            return;
        }
        // Listener outside the namespace; the sandboxed probe must not reach it.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let d = tempfile::tempdir().unwrap();
        let sh = format!("exec 3<>/dev/tcp/127.0.0.1/{port}");
        let cmd = ["bash".to_string(), "-c".to_string(), sh];
        let denied = run_limited(&cmd, d.path(), &limits(), &env(d.path())).unwrap();
        assert!(!denied.success(), "connect must fail inside netns");
        let mut open = limits();
        open.deny_net = false;
        let allowed = run_limited(&cmd, d.path(), &open, &env(d.path())).unwrap();
        assert!(
            allowed.success(),
            "control: same probe works with network allowed"
        );
    }
}
