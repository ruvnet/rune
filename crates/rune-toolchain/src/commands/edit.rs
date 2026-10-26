//! `rune edit`: one confined, hash-checked edit through an `EditApplier`.

use super::Failure;
use clap::Args as ClapArgs;
use rune_core::edit::{EditApplier, EditRequest};
use rune_edit::{AgentBoosterSidecar, ExactReplace};
use std::path::{Path, PathBuf};

#[derive(ClapArgs)]
pub struct Args {
    /// File to edit, relative to the workspace (absolute paths, `..` and symlink escapes are refused)
    #[arg(long)]
    path: PathBuf,
    /// SHA-256 of the file as you saw it; a mismatch refuses the edit
    #[arg(long)]
    expected_sha256: String,
    /// Exact text to find (must occur once); with --booster, the edit snippet
    #[arg(long, allow_hyphen_values = true)]
    find: String,
    /// Replacement text (unused with --booster)
    #[arg(long, allow_hyphen_values = true, required_unless_present = "booster")]
    replace: Option<String>,
    /// Apply with the Agent Booster Node sidecar instead of exact replace
    #[arg(long)]
    booster: bool,
    /// node_modules directory holding agent-booster (default: <workspace>/node_modules)
    #[arg(long)]
    node_modules: Option<PathBuf>,
    /// Language hint for the booster (default: from the file extension)
    #[arg(long)]
    language: Option<String>,
    /// Refuse files, and results, larger than this
    #[arg(long, default_value_t = 1_048_576)]
    max_bytes: u64,
}

fn language_of(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust".into(),
        Some("ts") => "typescript".into(),
        Some("js") => "javascript".into(),
        Some("py") => "python".into(),
        Some(other) => other.into(),
        None => "text".into(),
    }
}

pub(super) fn run(ws: &Path, a: &Args) -> Result<(), Failure> {
    let req = EditRequest {
        root: ws.to_path_buf(),
        path: a.path.clone(),
        expected_sha256: a.expected_sha256.clone(),
        find: a.find.clone(),
        replace: a.replace.clone().unwrap_or_default(),
        language: a.language.clone().unwrap_or_else(|| language_of(&a.path)),
        max_bytes: a.max_bytes,
    };
    let outcome = if a.booster {
        let nm = a
            .node_modules
            .clone()
            .unwrap_or_else(|| ws.join("node_modules"));
        AgentBoosterSidecar::new(nm).apply(&req)?
    } else {
        ExactReplace.apply(&req)?
    };
    println!(
        "edited {} with {}\n  before sha256:{}\n  after  sha256:{}",
        a.path.display(),
        outcome.applier,
        outcome.before_sha256,
        outcome.after_sha256
    );
    if let Some(c) = outcome.confidence {
        println!("  confidence {c:.3} (placement evidence, not proof)");
    }
    Ok(())
}
