//! `rune build` / `rune verify`. Model-free by construction: no network, no
//! ledger, no credentials. A successful build registers its artifact with the
//! ownership manifest so `rune cleanup` can reason about it.

use super::{Budget, Failure};
use rune_cargo as cargo;
use rune_core::{tree_digest, Error, Plan, Receipt, Store};
use rune_gc::{leases, put_object, Kind};
use std::path::Path;

fn observe(ws: &Path) -> Result<(String, String, String), Error> {
    Ok((
        tree_digest(ws)?,
        cargo::lock_digest(ws)?,
        cargo::toolchain_id()?,
    ))
}

pub(super) fn build(
    ws: &Path,
    store: &Store,
    freeze: bool,
    budget: &Budget,
) -> Result<(), Failure> {
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
        artifact_digest: art.digest.clone(),
        offline: true,
        model_calls: 0,
    };
    register(ws, store, &receipt, &art.path)?;
    println!(
        "built {} sha256:{}",
        receipt.artifact, receipt.artifact_digest
    );
    Ok(())
}

/// Own the artifact (content-addressed under `.rune/objects`), hold a lease on
/// it until the receipt that makes it reachable is written, then release.
fn register(ws: &Path, store: &Store, receipt: &Receipt, artifact: &Path) -> Result<(), Failure> {
    let job = format!("build-{}", std::process::id());
    leases::acquire(ws, &job, std::slice::from_ref(&receipt.artifact_digest))?;
    let result = (|| -> Result<(), Failure> {
        let bytes = std::fs::read(artifact)?;
        let key = put_object(ws, Kind::Artifact, &receipt.plan_id, None, &bytes)?;
        if key != receipt.artifact_digest {
            return Err(Failure::Other(
                "artifact changed between build and registration".into(),
            ));
        }
        store.record(receipt)?;
        Ok(())
    })();
    let released = leases::release(ws, &job);
    result?;
    released?;
    Ok(())
}

pub(super) fn verify(ws: &Path, store: &Store) -> Result<(), Failure> {
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
