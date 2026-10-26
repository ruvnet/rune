//! `rune` command line. Exit codes: 0 ok, 1 failure, 2 missing credentials.

mod commands;
mod propose;

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "rune", version, about = "The learning toolchain for Rust")]
struct Cli {
    /// Workspace root (contains Cargo.toml and Cargo.lock)
    #[arg(long, short = 'C', default_value = ".", global = true)]
    workspace: PathBuf,
    #[command(subcommand)]
    cmd: commands::Cmd,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match commands::run(&cli.workspace, cli.cmd) {
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
