mod build;
mod cleanup;
mod common;
mod edit;
mod eval;
mod explain;
mod improve;
mod package;
mod promote;
mod recipes;
mod target;

use clap::{Args, Subcommand};
use rune_core::{Error, Policy, Store};
use std::path::Path;

#[derive(Args, Clone)]
pub struct Budget {
    /// Maximum reasoning spend in USD
    #[arg(long, default_value_t = 2.0)]
    pub max_usd: f64,
    #[arg(long, default_value_t = 200_000)]
    pub max_tokens: u64,
    #[arg(long, default_value_t = 600)]
    pub max_seconds: u64,
    #[arg(long, default_value_t = 64)]
    pub max_memory_mb: u64,
}

impl Budget {
    pub(crate) fn policy(&self) -> Policy {
        Policy {
            max_usd: self.max_usd,
            max_tokens: self.max_tokens,
            max_seconds: self.max_seconds,
            max_memory_mb: self.max_memory_mb,
        }
    }
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Replay the approved plan with no model call; --freeze approves a new plan first
    Build {
        #[arg(long)]
        freeze: bool,
        #[command(flatten)]
        budget: Budget,
    },
    /// Explore candidate improvements within an explicit budget (never applies code)
    Improve {
        #[command(flatten)]
        budget: Budget,
        /// Campaign id for the cost ledger (default: derived from the approved plan id)
        #[arg(long)]
        campaign: Option<String>,
        /// Compare ledger totals with provider-reported totals instead of requesting
        #[arg(long)]
        reconcile: bool,
    },
    /// Rebuild independently and compare against the recorded artifact digest
    Verify,
    /// Show the approved plan, its lineage, ledger, evaluation, approvals and artifact status
    Explain,
    /// Restore the previous approved plan
    Rollback,
    /// Apply one confined, hash-checked edit (exact replace, or Agent Booster with --booster)
    Edit(Box<edit::Args>),
    /// Run Rune's independent evaluator; exit 0 on accept, 1 on reject
    Eval(Box<eval::Args>),
    /// Evidence memory: recipes are ranked for search, never approved by similarity
    Recipes {
        #[command(subcommand)]
        cmd: recipes::Cmd,
    },
    /// Approve a new plan bound to the exact tested artifact and policy
    Promote(Box<promote::Args>),
    /// Hardware targets: list descriptors, cross-build, run and classify evidence
    Target {
        #[command(subcommand)]
        cmd: target::Cmd,
    },
    /// Write the signed-or-unsigned evidence-directory artifact envelope for the current plan
    Package(Box<package::Args>),
    /// Verify an evidence-directory artifact and run the load gate
    VerifyArtifact(Box<package::VerifyArgs>),
    /// Quarantine-first cleanup of unreachable generated objects (dry run by default)
    Cleanup(Box<cleanup::Args>),
}

pub enum Failure {
    MissingCredentials(String),
    Other(String),
}

impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        Failure::Other(e.to_string())
    }
}

macro_rules! failure_from {
    ($($t:ty),*) => {$(
        impl From<$t> for Failure {
            fn from(e: $t) -> Self {
                Failure::Other(e.to_string())
            }
        }
    )*};
}
failure_from!(
    rune_memory::Error,
    rune_target::Error,
    rune_gc::Error,
    std::io::Error,
    serde_json::Error
);

pub fn run(ws: &Path, cmd: Cmd) -> Result<(), Failure> {
    // Cargo runs with this directory as cwd. Keep derived store/target paths
    // absolute so a relative -C is never interpreted a second time by Cargo.
    let canonical = std::fs::canonicalize(ws)
        .map_err(|e| Failure::Other(format!("workspace {}: {e}", ws.display())))?;
    let ws = canonical.as_path();
    let store = Store::open(ws);
    match cmd {
        Cmd::Build { freeze, budget } => build::build(ws, &store, freeze, &budget),
        Cmd::Improve {
            budget,
            campaign,
            reconcile,
        } => improve::improve(ws, &store, &budget, campaign.as_deref(), reconcile),
        Cmd::Verify => build::verify(ws, &store),
        Cmd::Explain => explain::explain(ws, &store),
        Cmd::Rollback => {
            let id = store.rollback()?;
            println!("rolled back; current plan {id}");
            Ok(())
        }
        Cmd::Edit(a) => edit::run(ws, &a),
        Cmd::Eval(a) => eval::run(ws, &store, &a),
        Cmd::Recipes { cmd } => recipes::run(ws, cmd),
        Cmd::Promote(a) => promote::run(ws, &store, &a),
        Cmd::Target { cmd } => target::run(ws, cmd),
        Cmd::Package(a) => package::package(ws, &store, &a),
        Cmd::VerifyArtifact(a) => package::verify_artifact(&a),
        Cmd::Cleanup(a) => cleanup::run(ws, &a),
    }
}
