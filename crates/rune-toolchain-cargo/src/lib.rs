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
/// Selects a workspace executable (or library) deterministically. Cargo message
/// completion order is not stable across clean and cached workspace builds.
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
    let canonical = std::fs::canonicalize(workspace).map_err(|e| Error::io(workspace, e))?;
    let path = artifact_from_messages(&String::from_utf8_lossy(&out.stdout), Some(&canonical))
        .ok_or_else(|| Error::Policy("cargo reported no build artifact".into()))?;
    let digest = file_digest(&path)?;
    Ok(Artifact { path, digest })
}

fn artifact_from_messages(stdout: &str, workspace: Option<&Path>) -> Option<PathBuf> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["reason"] == "compiler-artifact")
        .filter(|v| {
            workspace.is_none_or(|root| {
                v["manifest_path"]
                    .as_str()
                    .is_some_and(|p| Path::new(p).starts_with(root))
            })
        })
        .filter(|v| {
            !v["target"]["kind"]
                .as_array()
                .is_some_and(|ks| ks.iter().any(|k| k.as_str() == Some("custom-build")))
        })
        .filter_map(|v| {
            let executable = v["executable"].as_str().map(PathBuf::from);
            let is_executable = executable.is_some();
            executable
                .or_else(|| {
                    v["filenames"]
                        .as_array()?
                        .first()?
                        .as_str()
                        .map(PathBuf::from)
                })
                .map(|p| (is_executable, p))
        })
        .max()
        .map(|(_, p)| p)
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
        assert_eq!(
            artifact_from_messages(s, None),
            Some(PathBuf::from("/t/bin"))
        );
    }

    #[test]
    fn selected_artifact_does_not_depend_on_build_completion_order() {
        let a =
            r#"{"reason":"compiler-artifact","executable":"/t/alpha","filenames":["/t/alpha"]}"#;
        let z = r#"{"reason":"compiler-artifact","executable":"/t/zeta","filenames":["/t/zeta"]}"#;
        assert_eq!(
            artifact_from_messages(&format!("{a}\n{z}"), None),
            artifact_from_messages(&format!("{z}\n{a}"), None)
        );
    }

    #[test]
    fn dependencies_and_build_scripts_cannot_be_the_workspace_product() {
        let s = r#"{"reason":"compiler-artifact","manifest_path":"/ws/member/Cargo.toml","target":{"kind":["bin"]},"executable":"/t/app","filenames":[]}
{"reason":"compiler-artifact","manifest_path":"/cache/dep/Cargo.toml","target":{"kind":["bin"]},"executable":"/t/zzz-dependency","filenames":[]}
{"reason":"compiler-artifact","manifest_path":"/ws/member/Cargo.toml","target":{"kind":["custom-build"]},"executable":"/t/zzz-build-script","filenames":[]}"#;
        assert_eq!(
            artifact_from_messages(s, Some(Path::new("/ws"))),
            Some(PathBuf::from("/t/app"))
        );
    }
}
