//! Cargo compatibility adapter (ADR 001). Rune reuses rustc and Cargo; this
//! crate only observes inputs and runs `cargo build --locked --offline`.

use rune_core::{file_digest, Error};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Identity of the active toolchain: the full `rustc -vV` text.
pub fn toolchain_id() -> Result<String, Error> {
    let out = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|e| Error::io("rustc", e))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The host triple from `rustc -vV`, used as the default target.
pub fn host_target(toolchain: &str) -> String {
    toolchain
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .unwrap_or("unknown")
        .to_owned()
}

pub fn lock_digest(workspace: &Path) -> Result<String, Error> {
    file_digest(&workspace.join("Cargo.lock"))
}

pub struct Artifact {
    pub path: PathBuf,
    pub digest: String,
}

/// Build the workspace with a locked dependency graph and no network.
/// Returns the last executable (or library) artifact Cargo reports.
pub fn build(workspace: &Path, target_dir: &Path, target: Option<&str>) -> Result<Artifact, Error> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace)
        .args([
            "build",
            "--release",
            "--locked",
            "--offline",
            "--message-format=json-render-diagnostics",
        ])
        .arg("--target-dir")
        .arg(target_dir)
        .env("CARGO_TERM_COLOR", "never")
        .stderr(Stdio::inherit());
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().map_err(|e| Error::io("cargo", e))?;
    if !out.status.success() {
        return Err(Error::Policy(format!(
            "cargo build failed with {}",
            out.status
        )));
    }
    let path = artifact_from_messages(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| Error::Policy("cargo reported no build artifact".into()))?;
    let digest = file_digest(&path)?;
    Ok(Artifact { path, digest })
}

fn artifact_from_messages(stdout: &str) -> Option<PathBuf> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["reason"] == "compiler-artifact")
        .filter_map(|v| {
            v["executable"].as_str().map(PathBuf::from).or_else(|| {
                v["filenames"]
                    .as_array()?
                    .first()?
                    .as_str()
                    .map(PathBuf::from)
            })
        })
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_target_parsed() {
        assert_eq!(
            host_target("rustc 1\nhost: x86_64-unknown-linux-gnu\nrelease: 1"),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(host_target("garbage"), "unknown");
    }

    #[test]
    fn picks_executable_over_filenames() {
        let s = r#"{"reason":"compiler-artifact","executable":"/t/bin","filenames":["/t/lib"]}
{"reason":"build-finished","success":true}"#;
        assert_eq!(artifact_from_messages(s), Some(PathBuf::from("/t/bin")));
    }
}
