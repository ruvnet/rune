use clap::Args;
use rune_cargo as cargo;
use rune_core::{tree_digest, Error, Plan, Policy, Receipt, Store};
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
    fn policy(&self) -> Policy {
        Policy {
            max_usd: self.max_usd,
            max_tokens: self.max_tokens,
            max_seconds: self.max_seconds,
            max_memory_mb: self.max_memory_mb,
        }
    }
}

pub enum Op {
    Build { freeze: bool, budget: Budget },
    Improve { budget: Budget },
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
        Op::Build { freeze, budget } => build(ws, &store, freeze, &budget),
        Op::Improve { budget } => improve(&budget),
        Op::Verify => verify(ws, &store),
        Op::Explain => explain(&store),
        Op::Rollback => {
            let id = store.rollback()?;
            println!("rolled back; current plan {id}");
            Ok(())
        }
    }
}

fn observe(ws: &Path) -> Result<(String, String, String), Error> {
    Ok((
        tree_digest(ws)?,
        cargo::lock_digest(ws)?,
        cargo::toolchain_id()?,
    ))
}

fn build(ws: &Path, store: &Store, freeze: bool, budget: &Budget) -> Result<(), Failure> {
    let (src, lock, tc) = observe(ws)?;
    if freeze {
        let parent = store.current_id()?;
        let plan = Plan::new(
            src.clone(),
            lock.clone(),
            tc.clone(),
            cargo::host_target(&tc),
            budget.policy(),
            parent,
        );
        let id = store.approve(&plan)?;
        println!("approved plan {id}");
    }
    let plan = store.current().map_err(|e| match e {
        Error::NoPlan(_) => Failure::Other("no approved plan; run `rune build --freeze`".into()),
        other => other.into(),
    })?;
    plan.check_fresh(&src, &lock, &tc)?;
    let art = cargo::build(ws, &ws.join("target/rune"), None)?;
    let receipt = Receipt {
        plan_id: plan.id(),
        artifact: art
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        artifact_digest: art.digest,
        offline: true,
        model_calls: 0,
    };
    store.record(&receipt)?;
    println!(
        "built {} sha256:{}",
        receipt.artifact, receipt.artifact_digest
    );
    Ok(())
}

fn verify(ws: &Path, store: &Store) -> Result<(), Failure> {
    let plan = store.current()?;
    let id = plan.id();
    let (src, lock, tc) = observe(ws)?;
    plan.check_fresh(&src, &lock, &tc)?;
    let receipt = store
        .receipt(&id)?
        .ok_or_else(|| Failure::Other("no receipt; run `rune build` first".into()))?;
    let fresh =
        std::env::temp_dir().join(format!("rune-verify-{}-{}", std::process::id(), &id[..12]));
    let art = cargo::build(ws, &fresh, None);
    let _ = std::fs::remove_dir_all(&fresh);
    let art = art?;
    if art.digest == receipt.artifact_digest {
        println!("verified: artifact reproduces sha256:{}", art.digest);
        Ok(())
    } else {
        Err(Failure::Other(format!(
            "NOT reproducible: recorded {} but rebuilt {}",
            receipt.artifact_digest, art.digest
        )))
    }
}

fn explain(store: &Store) -> Result<(), Failure> {
    let mut next = store.current_id()?;
    if next.is_none() {
        return Err(Failure::Other("no approved plan".into()));
    }
    while let Some(id) = next {
        let plan = store.load(&id)?;
        println!("plan {id}\n  target     {}\n  source     {}\n  lockfile   {}\n  budget     ${} / {} tokens / {}s / {}MB",
            plan.target, plan.source_digest, plan.lock_digest,
            plan.policy.max_usd, plan.policy.max_tokens, plan.policy.max_seconds, plan.policy.max_memory_mb);
        match store.receipt(&id)? {
            Some(r) => println!(
                "  evidence   {} sha256:{} (offline={}, model calls={})",
                r.artifact, r.artifact_digest, r.offline, r.model_calls
            ),
            None => println!("  evidence   none"),
        }
        next = plan.parent;
    }
    Ok(())
}

fn improve(budget: &Budget) -> Result<(), Failure> {
    budget.policy().validate()?;
    if std::env::var_os("OPENROUTER_API_KEY").is_none_or(|v| v.is_empty()) {
        return Err(Failure::MissingCredentials(
            "OPENROUTER_API_KEY is not set; no request was made".into(),
        ));
    }
    Err(Failure::Other(
        "improve: proposal flow is Phase 2 (ADR 008) and not implemented".into(),
    ))
}
