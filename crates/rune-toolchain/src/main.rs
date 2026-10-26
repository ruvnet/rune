//! `rune` command line. Exit codes: 0 ok, 1 failure, 2 missing credentials.

mod commands;
mod propose;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "rune", version, about = "The learning toolchain for Rust")]
struct Cli {
    /// Workspace root (contains Cargo.toml and Cargo.lock)
    #[arg(long, short = 'C', default_value = ".", global = true)]
    workspace: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Replay the approved plan with no model call; --freeze approves a new plan first
    Build {
        #[arg(long)]
        freeze: bool,
        #[command(flatten)]
        budget: commands::Budget,
    },
    /// Explore candidate improvements within an explicit budget (never applies code)
    Improve {
        #[command(flatten)]
        budget: commands::Budget,
    },
    /// Rebuild independently and compare against the recorded artifact digest
    Verify,
    /// Show the approved plan, its lineage and evidence
    Explain,
    /// Restore the previous approved plan
    Rollback,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match commands::run(
        &cli.workspace,
        match cli.cmd {
            Cmd::Build { freeze, budget } => commands::Op::Build { freeze, budget },
            Cmd::Improve { budget } => commands::Op::Improve { budget },
            Cmd::Verify => commands::Op::Verify,
            Cmd::Explain => commands::Op::Explain,
            Cmd::Rollback => commands::Op::Rollback,
        },
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(commands::Failure::MissingCredentials(m)) => {
            eprintln!("rune: {m}");
            ExitCode::from(2)
        }
        Err(commands::Failure::Other(m)) => {
            eprintln!("rune: {m}");
            ExitCode::FAILURE
        }
    }
}
