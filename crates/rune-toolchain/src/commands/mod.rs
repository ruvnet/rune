mod build;
mod improve;

use clap::Args;
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

pub enum Op {
    Build {
        freeze: bool,
        budget: Budget,
    },
    Improve {
        budget: Budget,
        campaign: Option<String>,
        reconcile: bool,
    },
    Verify,
    Explain,
    Rollback,
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

pub fn run(ws: &Path, op: Op) -> Result<(), Failure> {
    let store = Store::open(ws);
    match op {
        Op::Build { freeze, budget } => build::build(ws, &store, freeze, &budget),
        Op::Improve {
            budget,
            campaign,
            reconcile,
        } => improve::improve(ws, &store, &budget, campaign.as_deref(), reconcile),
        Op::Verify => build::verify(ws, &store),
        Op::Explain => build::explain(&store),
        Op::Rollback => {
            let id = store.rollback()?;
            println!("rolled back; current plan {id}");
            Ok(())
        }
    }
}
