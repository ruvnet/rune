//! Reachability and the pure cleanup plan. Roots are: every approved plan
//! (history, its parent chain and every retained plan file, so rollback plans
//! count), the receipts of those plans, live job leases and retention-pinned
//! objects. Only owned GENERATED objects can ever be candidates; source files
//! are not in the manifest, and file age is not an input anywhere.

use crate::leases;
use crate::liveness::{Liveness, ProcLiveness};
use crate::manifest::{Entry, Kind, Manifest, Retention};
use crate::paths::{is_hex64, secure_dir, validate_key};
use crate::{Error, Result};
use rune_core::Store;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Reach {
    pub plans: BTreeSet<String>,
    /// artifact digest -> plan whose receipt names it
    pub receipt_digests: BTreeMap<String, String>,
    /// object key -> job holding a live lease
    pub leased: BTreeMap<String, String>,
    pub stale_jobs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub key: String,
    pub kind: Kind,
    pub plan_id: String,
    pub seq: u64,
    pub size: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CleanupPlan {
    pub workspace: PathBuf,
    pub candidates: Vec<Candidate>,
    /// Owned objects kept, with why.
    pub retained: Vec<(String, String)>,
    /// Manifest entries refused outright (invalid key etc.); never touched.
    pub rejected: Vec<(String, String)>,
    pub stale_leases: Vec<String>,
}

fn plan_ids(workspace: &Path) -> Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    if let Some(rune) = secure_dir(workspace, &[], false)? {
        let hist = rune.join("history");
        if let Ok(s) = fs::read_to_string(&hist) {
            ids.extend(s.lines().filter(|l| !l.is_empty()).map(str::to_owned));
        }
    }
    if let Some(dir) = secure_dir(workspace, &["plans"], false)? {
        for e in fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))? {
            let name = e.map_err(|e| Error::io(&dir, e))?.file_name();
            if let Some(id) = name.to_string_lossy().strip_suffix(".json") {
                if is_hex64(id) {
                    ids.insert(id.to_owned());
                }
            }
        }
    }
    Ok(ids)
}

/// Compute the root set. Fails closed: an unreadable plan, receipt or lease
/// aborts the computation instead of silently shrinking the root set.
pub fn compute_reach(workspace: &Path, live: &dyn Liveness) -> Result<Reach> {
    let store = Store::open(workspace);
    let mut reach = Reach::default();
    let mut queue: Vec<String> = plan_ids(workspace)?.into_iter().collect();
    while let Some(id) = queue.pop() {
        if !reach.plans.insert(id.clone()) {
            continue;
        }
        let plan = store.load(&id)?;
        queue.extend(plan.parent);
        if let Some(r) = store.receipt(&id)? {
            let d = r.artifact_digest.trim_start_matches("sha256:").to_owned();
            reach.receipt_digests.insert(d, id);
        }
    }
    for l in leases::list(workspace)? {
        if live.is_alive(l.pid, l.pid_start) {
            for k in l.objects {
                reach.leased.insert(k, l.job.clone());
            }
        } else {
            reach.stale_jobs.push(l.job);
        }
    }
    Ok(reach)
}

/// Why an object is reachable, or `None` if it is not.
pub fn retained_because(key: &str, e: &Entry, reach: &Reach) -> Option<String> {
    if e.retention == Retention::Pinned {
        return Some("pinned by retention policy".into());
    }
    if let Some(job) = reach.leased.get(key) {
        return Some(format!("held by live lease {job}"));
    }
    if let Some(plan) = reach.receipt_digests.get(key) {
        return Some(format!("artifact of receipt for plan {plan}"));
    }
    if e.retention == Retention::WhilePlanReachable && reach.plans.contains(&e.plan_id) {
        return Some(format!("retained while plan {} is a root", e.plan_id));
    }
    None
}

/// Pure: reads the workspace, changes nothing.
pub fn plan_cleanup(workspace: &Path) -> Result<CleanupPlan> {
    plan_cleanup_with(workspace, &ProcLiveness)
}

pub fn plan_cleanup_with(workspace: &Path, live: &dyn Liveness) -> Result<CleanupPlan> {
    let reach = compute_reach(workspace, live)?;
    let m = Manifest::load(workspace)?;
    let mut plan = CleanupPlan {
        workspace: workspace.to_path_buf(),
        stale_leases: reach.stale_jobs.clone(),
        ..CleanupPlan::default()
    };
    for (key, e) in &m.entries {
        if let Err(err) = validate_key(key) {
            plan.rejected.push((key.clone(), err.to_string()));
            continue;
        }
        match retained_because(key, e, &reach) {
            Some(why) => plan.retained.push((key.clone(), why)),
            None => plan.candidates.push(Candidate {
                key: key.clone(),
                kind: e.kind,
                plan_id: e.plan_id.clone(),
                seq: e.seq,
                size: e.size,
                reason: "unreachable from plans, receipts, leases and retention".into(),
            }),
        }
    }
    plan.candidates.sort_by_key(|c| c.seq);
    Ok(plan)
}

/// Keys currently safe to move: recomputed at execution time so a stale or
/// tampered plan can neither widen the set nor ignore a new lease.
pub(crate) fn unreachable_now(
    workspace: &Path,
    m: &Manifest,
    live: &dyn Liveness,
) -> Result<BTreeSet<String>> {
    let reach = compute_reach(workspace, live)?;
    Ok(m.entries
        .iter()
        .filter(|(k, e)| validate_key(k).is_ok() && retained_because(k, e, &reach).is_none())
        .map(|(k, _)| k.clone())
        .collect())
}
