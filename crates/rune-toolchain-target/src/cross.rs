//! Locked, offline Cargo builds for a target descriptor.
//!
//! The ambient `RUSTFLAGS` of a developer shell (for example mold `link-arg`s)
//! break cross and wasm links, so rustflags are always set explicitly per
//! build and nothing is inherited.

use crate::Error;
use rune_core::file_digest;
use rune_core::target::{Evidence, TargetDescriptor};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct CrossOptions {
    /// Explicit rustflags for this build. Empty by default (deterministic).
    pub rustflags: Vec<String>,
    /// `--bin` selector; the last reported executable is used when `None`.
    pub bin: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CrossArtifact {
    pub path: PathBuf,
    pub digest: String,
    pub triple: String,
    /// A build proves only that it compiled and linked.
    pub evidence: Evidence,
}

/// `CARGO_TARGET_<TRIPLE>_LINKER` for a triple.
pub fn linker_env_name(triple: &str) -> String {
    let t: String = triple
        .chars()
        .map(|c| {
            if c == '-' || c == '.' {
                '_'
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect();
    format!("CARGO_TARGET_{t}_LINKER")
}

/// Build the `cargo` invocation (without running it); split out so the
/// environment handling is unit-testable.
pub fn build_command(
    project: &Path,
    target_dir: &Path,
    target: &TargetDescriptor,
    opts: &CrossOptions,
) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(project)
        .args([
            "build",
            "--release",
            "--locked",
            "--offline",
            "--message-format=json-render-diagnostics",
        ])
        .arg("--target-dir")
        .arg(target_dir)
        .args(["--target", &target.triple])
        .env("CARGO_TERM_COLOR", "never")
        // Both spellings set: whichever cargo prefers, ambient flags lose.
        .env("RUSTFLAGS", opts.rustflags.join(" "))
        .env("CARGO_ENCODED_RUSTFLAGS", opts.rustflags.join("\u{1f}"))
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .stderr(Stdio::piped());
    if let Some(bin) = &opts.bin {
        cmd.args(["--bin", bin]);
    }
    let wasm = target.triple.starts_with("wasm");
    if !wasm && !target.linker.is_empty() && target.linker != "rust-lld" {
        cmd.env(linker_env_name(&target.triple), &target.linker);
    }
    cmd
}

/// Cross-build `project` for `target`; the result is `Evidence::CompileOnly`.
pub fn cross_build(
    project: &Path,
    target_dir: &Path,
    target: &TargetDescriptor,
    opts: &CrossOptions,
) -> Result<CrossArtifact, Error> {
    let out = build_command(project, target_dir, target, opts)
        .output()
        .map_err(|e| Error::io("cargo", e))?;
    if !out.status.success() {
        return Err(Error::Build(format!(
            "cargo build --target {} failed ({}): {}",
            target.triple,
            out.status,
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }
    let path = executable_from_messages(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| Error::Build("cargo reported no executable artifact".into()))?;
    let digest = file_digest(&path)?;
    Ok(CrossArtifact {
        path,
        digest,
        triple: target.triple.clone(),
        evidence: Evidence::CompileOnly,
    })
}

fn executable_from_messages(stdout: &str) -> Option<PathBuf> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["reason"] == "compiler-artifact")
        .filter_map(|v| v["executable"].as_str().map(PathBuf::from))
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptors::*;

    #[test]
    fn linker_env_name_is_cargos() {
        assert_eq!(
            linker_env_name("aarch64-unknown-linux-gnu"),
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"
        );
    }

    fn envs(cmd: &Command) -> Vec<(String, Option<String>)> {
        cmd.get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn rustflags_are_explicit_and_linker_is_per_target() {
        let cmd = build_command(
            Path::new("."),
            Path::new("/t"),
            &aarch64_linux_gnu(&[]),
            &CrossOptions::default(),
        );
        let e = envs(&cmd);
        assert!(e.contains(&("RUSTFLAGS".into(), Some(String::new()))));
        assert!(e.contains(&("CARGO_ENCODED_RUSTFLAGS".into(), Some(String::new()))));
        assert!(e.contains(&(
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER".into(),
            Some("aarch64-linux-gnu-gcc".into())
        )));
        let w = envs(&build_command(
            Path::new("."),
            Path::new("/t"),
            &wasm32_wasip1(),
            &CrossOptions::default(),
        ));
        assert!(!w.iter().any(|(k, _)| k.ends_with("_LINKER")));
    }
}
