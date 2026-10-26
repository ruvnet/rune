//! Quarantine-first deletion. `quarantine` moves unreachable generated
//! objects aside (reversible with `restore`); `purge` deletes only quarantined
//! objects older than 7 days by an injected clock, after re-checking
//! reachability. Nothing here accepts a path: every path is derived from a
//! validated key (and, for candidate workspaces, a verified ownership marker).

use crate::gc::{compute_reach, retained_because, unreachable_now, CleanupPlan};
use crate::liveness::ProcLiveness;
use crate::manifest::{Entry, Kind, Manifest, QEntry};
use crate::objects::{candidate_key, CANDIDATE_MARKER};
use crate::paths::{secure_child, secure_dir, validate_key};
use crate::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const QUARANTINE_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Report only. The default.
    #[default]
    DryRun,
    Apply,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub mode: Mode,
    pub batch: Option<u64>,
    /// Keys moved / deleted / restored (or that would be, in dry-run).
    pub done: Vec<String>,
    pub restored: Vec<String>,
    pub skipped: Vec<(String, String)>,
}

fn verify_candidate(key: &str, e: &Entry) -> Result<(PathBuf, String)> {
    let p = PathBuf::from(
        e.path
            .as_deref()
            .ok_or_else(|| Error::Refused("no path".into()))?,
    );
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Escape(p.display().to_string()))?
        .to_owned();
    let parent = p
        .parent()
        .ok_or_else(|| Error::Escape(p.display().to_string()))?;
    let real_parent = fs::canonicalize(parent).map_err(|e| Error::io(parent, e))?;
    if real_parent != parent || candidate_key(&p) != key {
        return Err(Error::Escape(p.display().to_string()));
    }
    Ok((p, name))
}

fn check_marker(dir: &Path, key: &str) -> Result<()> {
    match crate::paths::is_symlink(dir)? {
        Some(false) if dir.is_dir() => {}
        _ => return Err(Error::Escape(dir.display().to_string())),
    }
    let marker = dir.join(CANDIDATE_MARKER);
    let ok = crate::paths::is_symlink(&marker)? == Some(false)
        && fs::read_to_string(&marker)
            .map(|s| s == key)
            .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(Error::Refused(format!(
            "{} lacks ownership marker",
            dir.display()
        )))
    }
}

/// `(original, held)` for an owned entry; verifies containment on both.
fn locate(ws: &Path, key: &str, e: &Entry, batch: u64, create: bool) -> Result<(PathBuf, PathBuf)> {
    validate_key(key)?;
    if e.kind == Kind::CandidateWorkspace {
        let (p, name) = verify_candidate(key, e)?;
        let held = p.with_file_name(format!(".{name}.rune-q{batch}"));
        return Ok((p, held));
    }
    let objects = secure_dir(ws, &["objects"], false)?
        .ok_or_else(|| Error::NotFound("objects directory".into()))?;
    let orig = secure_child(&objects, key)?;
    let qdir = secure_dir(ws, &["quarantine", &batch.to_string()], create)?;
    let held = match qdir {
        Some(d) => secure_child(&d, key)?,
        None => return Ok((orig, PathBuf::new())),
    };
    Ok((orig, held))
}

fn rename(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).map_err(|e| Error::io(from, e))
}

fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

/// Move unreachable generated objects of `plan` into `.rune/quarantine/<seq>/`.
/// Reachability is recomputed now; the plan can only narrow the set.
pub fn quarantine(plan: &CleanupPlan, now_unix: u64, mode: Mode) -> Result<Report> {
    let ws = plan.workspace.as_path();
    let mut m = Manifest::load(ws)?;
    let safe = unreachable_now(ws, &m, &ProcLiveness)?;
    let mut rep = Report {
        mode,
        ..Report::default()
    };
    let batch = m.alloc_seq();
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for c in &plan.candidates {
        let Some(entry) = m.entries.get(&c.key).cloned() else {
            rep.skipped.push((c.key.clone(), "not owned".into()));
            continue;
        };
        if !safe.contains(&c.key) {
            rep.skipped.push((c.key.clone(), "reachable now".into()));
            continue;
        }
        let create = mode == Mode::Apply;
        let step = locate(ws, &c.key, &entry, batch, create).and_then(|(from, to)| {
            if !exists(&from) {
                return Err(Error::NotFound(from.display().to_string()));
            }
            if entry.kind == Kind::CandidateWorkspace {
                check_marker(&from, &c.key)?;
            }
            if mode == Mode::Apply {
                if exists(&to) {
                    return Err(Error::Refused(format!("{} already exists", to.display())));
                }
                rename(&from, &to)?;
                moved.push((from, to));
            }
            Ok(())
        });
        match step {
            Ok(()) => {
                rep.done.push(c.key.clone());
                if mode == Mode::Apply {
                    m.entries.remove(&c.key);
                    m.quarantine.push(QEntry {
                        batch,
                        key: c.key.clone(),
                        entry,
                        quarantined_at: now_unix,
                    });
                }
            }
            Err(e) => rep.skipped.push((c.key.clone(), e.to_string())),
        }
    }
    if mode == Mode::Apply && !moved.is_empty() {
        rep.batch = Some(batch);
        if let Err(e) = m.commit(ws) {
            for (from, to) in moved.iter().rev() {
                let _ = fs::rename(to, from); // stale writer: undo every move
            }
            return Err(e);
        }
    }
    Ok(rep)
}

/// Undo one quarantine batch: bytes return to their original location.
pub fn restore(workspace: &Path, batch: u64) -> Result<Report> {
    let mut m = Manifest::load(workspace)?;
    let mut rep = Report {
        mode: Mode::Apply,
        batch: Some(batch),
        ..Report::default()
    };
    let mut keep = Vec::new();
    for q in std::mem::take(&mut m.quarantine) {
        if q.batch != batch {
            keep.push(q);
            continue;
        }
        match restore_one(workspace, &m, &q) {
            Ok(()) => {
                rep.restored.push(q.key.clone());
                m.entries.insert(q.key.clone(), q.entry.clone());
            }
            Err(e) => {
                rep.skipped.push((q.key.clone(), e.to_string()));
                keep.push(q);
            }
        }
    }
    m.quarantine = keep;
    if rep.restored.is_empty() && rep.skipped.is_empty() {
        return Err(Error::NotFound(format!("quarantine batch {batch}")));
    }
    m.commit(workspace)?;
    Ok(rep)
}

fn restore_one(ws: &Path, m: &Manifest, q: &QEntry) -> Result<()> {
    if m.entries.contains_key(&q.key) {
        return Err(Error::Refused("key was re-created since quarantine".into()));
    }
    let (orig, held) = locate(ws, &q.key, &q.entry, q.batch, false)?;
    if !exists(&held) {
        return Err(Error::NotFound(held.display().to_string()));
    }
    if q.entry.kind == Kind::CandidateWorkspace {
        check_marker(&held, &q.key)?;
    }
    if exists(&orig) {
        return Err(Error::Refused(format!("{} is occupied", orig.display())));
    }
    rename(&held, &orig)
}

/// Delete quarantined objects at least 7 days old (by `now_unix`, injected).
/// Re-checks reachability first: an object that became reachable again is
/// restored, never deleted. Dry-run by default.
pub fn purge(workspace: &Path, now_unix: u64, mode: Mode) -> Result<Report> {
    let mut m = Manifest::load(workspace)?;
    let reach = compute_reach(workspace, &ProcLiveness)?;
    let mut rep = Report {
        mode,
        ..Report::default()
    };
    let mut keep = Vec::new();
    for q in std::mem::take(&mut m.quarantine) {
        let age_ok = now_unix >= q.quarantined_at.saturating_add(QUARANTINE_SECS);
        if !age_ok {
            keep.push(q);
            continue;
        }
        if validate_key(&q.key).is_err() {
            rep.skipped.push((q.key.clone(), "invalid key".into()));
            keep.push(q);
            continue;
        }
        if retained_because(&q.key, &q.entry, &reach).is_some() {
            if mode == Mode::Apply {
                match restore_one(workspace, &m, &q) {
                    Ok(()) => {
                        rep.restored.push(q.key.clone());
                        m.entries.insert(q.key.clone(), q.entry.clone());
                    }
                    Err(e) => {
                        rep.skipped.push((q.key.clone(), e.to_string()));
                        keep.push(q);
                    }
                }
            } else {
                rep.restored.push(q.key.clone());
                keep.push(q);
            }
            continue;
        }
        match delete_held(workspace, &q, mode) {
            Ok(()) => {
                rep.done.push(q.key.clone());
                if mode == Mode::DryRun {
                    keep.push(q);
                }
            }
            Err(e) => {
                rep.skipped.push((q.key.clone(), e.to_string()));
                keep.push(q);
            }
        }
    }
    m.quarantine = keep;
    if mode == Mode::Apply && (!rep.done.is_empty() || !rep.restored.is_empty()) {
        m.commit(workspace)?;
    }
    Ok(rep)
}

fn delete_held(ws: &Path, q: &QEntry, mode: Mode) -> Result<()> {
    let (_, held) = locate(ws, &q.key, &q.entry, q.batch, false)?;
    if held.as_os_str().is_empty() || !exists(&held) {
        return Ok(()); // already gone
    }
    let is_cand = q.entry.kind == Kind::CandidateWorkspace;
    if is_cand {
        check_marker(&held, &q.key)?;
    }
    if mode == Mode::DryRun {
        return Ok(());
    }
    let r = if is_cand {
        fs::remove_dir_all(&held)
    } else {
        fs::remove_file(&held)
    };
    r.map_err(|e| Error::io(&held, e))
}
