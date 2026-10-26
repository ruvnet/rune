//! Ownership manifest `.rune/ownership.json`: object key -> owner. Source
//! files are never entered here, so they can never be collected. Writers use
//! a version counter with an exclusive lock file and an atomic rename; a
//! writer holding a stale version fails with `Error::Conflict`.

use crate::liveness::{start_time, Liveness, ProcLiveness};
use crate::paths::secure_dir;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Artifact,
    CandidateWorkspace,
    Receipt,
    Evidence,
}

/// Retention class. `Pinned` is never collected; `WhilePlanReachable` lives as
/// long as its owning plan is a root; `Transient` is held only by receipts or
/// leases that reference it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Retention {
    Transient,
    WhilePlanReachable,
    Pinned,
}

impl Kind {
    pub fn default_retention(self) -> Retention {
        match self {
            Kind::Receipt | Kind::Evidence => Retention::WhilePlanReachable,
            Kind::Artifact | Kind::CandidateWorkspace => Retention::Transient,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    pub plan_id: String,
    /// Creation sequence number; the only ordering used by logic.
    pub seq: u64,
    pub size: u64,
    pub retention: Retention,
    /// Informational wall-clock stamp, never read by cleanup logic.
    pub created_unix: u64,
    /// Candidate workspaces only: canonical directory.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QEntry {
    pub batch: u64,
    pub key: String,
    pub entry: Entry,
    pub quarantined_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u64,
    pub next_seq: u64,
    pub entries: BTreeMap<String, Entry>,
    #[serde(default)]
    pub quarantine: Vec<QEntry>,
}

fn manifest_path(rune: &Path) -> PathBuf {
    rune.join("ownership.json")
}

fn read_disk(rune: &Path) -> Result<Manifest> {
    let p = manifest_path(rune);
    match fs::read(&p) {
        Ok(b) => Ok(serde_json::from_slice(&b)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
        Err(e) => Err(Error::io(p, e)),
    }
}

struct LockGuard(PathBuf);
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn lock(rune: &Path) -> Result<LockGuard> {
    let path = rune.join("ownership.lock");
    for attempt in 0..2 {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                let pid = std::process::id();
                let _ = writeln!(f, "{pid} {}", start_time(pid).unwrap_or(0));
                return Ok(LockGuard(path));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let text = fs::read_to_string(&path).unwrap_or_default();
                let mut it = text.split_whitespace().map(|t| t.parse::<u64>().ok());
                let pid = it.next().flatten().unwrap_or(0) as u32;
                let start = it.next().flatten().unwrap_or(0);
                if attempt == 0 && pid != 0 && !ProcLiveness.is_alive(pid, start) {
                    let _ = fs::remove_file(&path); // holder died: reclaim once
                    continue;
                }
                return Err(Error::Locked(pid));
            }
            Err(e) => return Err(Error::io(path, e)),
        }
    }
    Err(Error::Locked(0))
}

impl Manifest {
    pub fn load(workspace: &Path) -> Result<Manifest> {
        match secure_dir(workspace, &[], false)? {
            Some(rune) => read_disk(&rune),
            None => Ok(Manifest::default()),
        }
    }

    /// Compare-and-swap: succeeds only if the on-disk version still equals the
    /// version this handle was loaded at.
    pub fn commit(&mut self, workspace: &Path) -> Result<()> {
        let rune = secure_dir(workspace, &[], true)?.expect("created");
        let _guard = lock(&rune)?;
        let found = read_disk(&rune)?.version;
        if found != self.version {
            return Err(Error::Conflict {
                expected: self.version,
                found,
            });
        }
        let mut next = self.clone();
        next.version += 1;
        let tmp = rune.join(format!("ownership.json.tmp-{}", std::process::id()));
        fs::write(&tmp, serde_json::to_vec_pretty(&next)?).map_err(|e| Error::io(&tmp, e))?;
        fs::rename(&tmp, manifest_path(&rune)).map_err(|e| Error::io(&tmp, e))?;
        *self = next;
        Ok(())
    }

    /// Load, mutate, commit; retries when another writer wins the race.
    pub fn update<T>(workspace: &Path, mut f: impl FnMut(&mut Manifest) -> Result<T>) -> Result<T> {
        let mut last = None;
        for _ in 0..8 {
            let mut m = Manifest::load(workspace)?;
            let out = f(&mut m)?;
            match m.commit(workspace) {
                Ok(()) => return Ok(out),
                Err(e @ (Error::Conflict { .. } | Error::Locked(_))) => last = Some(e),
                Err(e) => return Err(e),
            }
        }
        Err(last.expect("loop ran"))
    }

    pub fn alloc_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }
}
