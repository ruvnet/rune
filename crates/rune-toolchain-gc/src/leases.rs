//! Active job leases `.rune/leases/<job>.json`. A lease holds its objects
//! reachable while its owner process is alive. Staleness is decided by pid
//! liveness (/proc, with start time to defeat pid reuse), never by file age.

use crate::liveness::{start_time, Liveness};
use crate::paths::{secure_child, secure_dir, validate_key, validate_name};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lease {
    pub job: String,
    pub pid: u32,
    pub pid_start: u64,
    pub heartbeat_seq: u64,
    pub objects: Vec<String>,
}

fn write_lease(workspace: &Path, lease: &Lease) -> Result<()> {
    let dir = secure_dir(workspace, &["leases"], true)?.expect("created");
    let path = secure_child(&dir, &format!("{}.json", lease.job))?;
    let tmp = secure_child(&dir, &format!("{}.json.tmp", lease.job))?;
    fs::write(&tmp, serde_json::to_vec_pretty(lease)?).map_err(|e| Error::io(&tmp, e))?;
    fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))
}

/// Take a lease for the current process.
pub fn acquire(workspace: &Path, job: &str, objects: &[String]) -> Result<Lease> {
    acquire_for(workspace, job, std::process::id(), objects)
}

/// Take a lease on behalf of `pid` (a worker process we spawned).
pub fn acquire_for(workspace: &Path, job: &str, pid: u32, objects: &[String]) -> Result<Lease> {
    validate_name(job)?;
    objects.iter().try_for_each(|k| validate_key(k))?;
    let lease = Lease {
        job: job.to_owned(),
        pid,
        pid_start: start_time(pid).unwrap_or(0),
        heartbeat_seq: 0,
        objects: objects.to_vec(),
    };
    write_lease(workspace, &lease)?;
    Ok(lease)
}

pub fn heartbeat(workspace: &Path, job: &str) -> Result<Lease> {
    let mut lease = read(workspace, job)?;
    lease.heartbeat_seq += 1;
    write_lease(workspace, &lease)?;
    Ok(lease)
}

pub fn read(workspace: &Path, job: &str) -> Result<Lease> {
    validate_name(job)?;
    let dir = secure_dir(workspace, &["leases"], false)?
        .ok_or_else(|| Error::NotFound(format!("lease {job}")))?;
    let path = secure_child(&dir, &format!("{job}.json"))?;
    Ok(serde_json::from_slice(
        &fs::read(&path).map_err(|e| Error::io(&path, e))?,
    )?)
}

pub fn release(workspace: &Path, job: &str) -> Result<()> {
    validate_name(job)?;
    if let Some(dir) = secure_dir(workspace, &["leases"], false)? {
        let path = secure_child(&dir, &format!("{job}.json"))?;
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::io(path, e)),
        }
    }
    Ok(())
}

/// All leases on disk. A malformed lease is an error (fail closed): we cannot
/// know what it was protecting.
pub fn list(workspace: &Path) -> Result<Vec<Lease>> {
    let Some(dir) = secure_dir(workspace, &["leases"], false)? else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for e in fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))? {
        let e = e.map_err(|e| Error::io(&dir, e))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(job) = name.strip_suffix(".json") else {
            continue;
        };
        if validate_name(job).is_err() {
            continue;
        }
        let lease = read(workspace, job)?;
        if lease.job != job {
            return Err(Error::Refused(format!(
                "lease {job} names job {}",
                lease.job
            )));
        }
        out.push(lease);
    }
    out.sort_by(|a, b| a.job.cmp(&b.job));
    Ok(out)
}

/// Remove leases whose owner is dead according to `live`. Returns their jobs.
pub fn reclaim_stale(workspace: &Path, live: &dyn Liveness) -> Result<Vec<String>> {
    let mut reclaimed = Vec::new();
    for l in list(workspace)? {
        if !live.is_alive(l.pid, l.pid_start) {
            release(workspace, &l.job)?;
            reclaimed.push(l.job);
        }
    }
    Ok(reclaimed)
}
