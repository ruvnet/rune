//! Frozen snapshots and the JSON-lines store under `.rune/memory/`.
//!
//! Files: `recipes.jsonl`, `evidence.jsonl` (validated), `queue.jsonl`
//! (unvalidated, never read by snapshots), `approvals.jsonl`.

use crate::approval::{ApprovalBook, ApprovalRecord, KeyInputs};
use crate::ingest::Evidence;
use crate::rank::{featurize, DIM};
use crate::recipe::{EvidenceRef, Failure, Recipe, Status, Visibility};
use crate::Error;
use rune_core::contract::{EvalContract, Verdict};
use rune_core::hex_sha256;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Immutable view of the memory for one search campaign. It owns its data, so
/// later writes to the store cannot change it. The id is a content address.
#[derive(Debug, Clone)]
pub struct Snapshot {
    id: String,
    tenant: String,
    recipes: Vec<Recipe>,
    vectors: Vec<f32>,
}

impl Snapshot {
    /// Build a snapshot for `tenant`: shared records plus the tenant's own
    /// private ones; other tenants' private records are never included.
    /// Records are sorted by id and de-duplicated (first wins).
    pub fn from_recipes(tenant: &str, recipes: Vec<Recipe>) -> Snapshot {
        let mut recipes: Vec<Recipe> = recipes
            .into_iter()
            .filter(|r| r.visibility == Visibility::Shared || r.tenant == tenant)
            .collect();
        recipes.sort_by(|a, b| a.id.cmp(&b.id));
        recipes.dedup_by(|a, b| a.id == b.id);
        let mut vectors = Vec::with_capacity(recipes.len() * DIM);
        for r in &recipes {
            vectors.extend(featurize(&r.fingerprint));
        }
        let body = serde_json::to_vec(&(tenant, &recipes)).expect("snapshot serializes");
        Snapshot {
            id: hex_sha256(&body),
            tenant: tenant.to_string(),
            recipes,
            vectors,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn tenant(&self) -> &str {
        &self.tenant
    }
    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }
    pub fn len(&self) -> usize {
        self.recipes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }
    /// Precomputed L2-normalised feature vector of record `i` (length [`DIM`]).
    pub fn vector(&self, i: usize) -> &[f32] {
        &self.vectors[i * DIM..(i + 1) * DIM]
    }
}

/// Serialise records for public federation; refuses if any is private.
pub fn export_shareable(recipes: &[Recipe]) -> Result<String, Error> {
    let mut out = String::new();
    for r in recipes {
        if r.visibility == Visibility::Private {
            return Err(Error::PrivateExport(r.id.clone()));
        }
        out.push_str(&serde_json::to_string(r)?);
        out.push('\n');
    }
    Ok(out)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DrainReport {
    pub ingested: usize,
    pub rejected: Vec<String>,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// Open (creating) `<workspace>/.rune/memory`. Symlinked store directories
    /// are refused so records cannot be redirected out of the workspace.
    pub fn open(workspace: &Path) -> Result<Store, Error> {
        let rune = workspace.join(".rune");
        let dir = rune.join("memory");
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        for p in [&rune, &dir] {
            let md = fs::symlink_metadata(p).map_err(|e| Error::io(p, e))?;
            if md.file_type().is_symlink() || !md.is_dir() {
                return Err(Error::UnsafePath(p.display().to_string()));
            }
        }
        Ok(Store { dir })
    }

    /// One `write_all` of a complete line to an `O_APPEND` handle, then fsync:
    /// concurrent appenders cannot interleave within a line.
    fn append<T: Serialize>(&self, file: &str, value: &T) -> Result<(), Error> {
        let mut line = serde_json::to_vec(value)?;
        line.push(b'\n');
        let path = self.dir.join(file);
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| Error::io(&path, e))?;
        f.write_all(&line).map_err(|e| Error::io(&path, e))?;
        f.sync_all().map_err(|e| Error::io(&path, e))
    }

    fn read<T: DeserializeOwned>(&self, file: &str) -> Result<Vec<T>, Error> {
        read_lines(&self.dir.join(file))
    }

    /// Add a proposal. Approved status cannot be written here: approval
    /// exists only as a key record ([`Store::record_approval`]).
    pub fn add_recipe(&self, recipe: &Recipe) -> Result<(), Error> {
        recipe.validate()?;
        if recipe.status == Status::Approved {
            return Err(Error::Invalid(
                "recipes enter as proposed or rejected".into(),
            ));
        }
        self.append("recipes.jsonl", recipe)
    }

    /// Queue unvalidated evidence. Snapshots never read the queue.
    pub fn enqueue(&self, ev: &Evidence) -> Result<(), Error> {
        self.append("queue.jsonl", ev)
    }

    /// Validate and append evidence to the store.
    pub fn ingest(&self, ev: &Evidence, contract: &EvalContract) -> Result<(), Error> {
        ev.validate(contract)?;
        let recipes: Vec<Recipe> = self.read("recipes.jsonl")?;
        let r = recipes
            .iter()
            .find(|r| r.id == ev.recipe_id)
            .ok_or_else(|| Error::UnknownRecipe(ev.recipe_id.clone()))?;
        if ev.report.candidate_digest != r.candidate_hash
            || ev.report.baseline_digest != r.baseline_hash
        {
            return Err(Error::EvidenceRejected(
                "report digests do not match the recipe".into(),
            ));
        }
        self.append("evidence.jsonl", ev)
    }

    /// Validate every queued item; valid ones enter the store, the rest are
    /// reported. The queue file is renamed away first so concurrent
    /// enqueues land in a fresh queue.
    pub fn drain_queue(&self, contract: &EvalContract) -> Result<DrainReport, Error> {
        let (queue, draining) = (
            self.dir.join("queue.jsonl"),
            self.dir.join("queue.draining"),
        );
        if queue.exists() && !draining.exists() {
            fs::rename(&queue, &draining).map_err(|e| Error::io(&queue, e))?;
        }
        let mut rep = DrainReport::default();
        if !draining.exists() {
            return Ok(rep);
        }
        for ev in read_lines::<Evidence>(&draining)? {
            match self.ingest(&ev, contract) {
                Ok(()) => rep.ingested += 1,
                Err(e) => rep.rejected.push(format!("{}: {e}", ev.recipe_id)),
            }
        }
        fs::remove_file(&draining).map_err(|e| Error::io(&draining, e))?;
        Ok(rep)
    }

    /// Store an approval key. Requires validated Accept evidence for this
    /// exact candidate under exactly `inputs`; a recipe never inherits
    /// approval from another target, compiler, policy, contract or evaluator.
    pub fn record_approval(&self, recipe_id: &str, inputs: &KeyInputs) -> Result<String, Error> {
        let recipes: Vec<Recipe> = self.read("recipes.jsonl")?;
        let r = recipes
            .iter()
            .find(|r| r.id == recipe_id)
            .ok_or_else(|| Error::UnknownRecipe(recipe_id.into()))?;
        let evs: Vec<Evidence> = self.read("evidence.jsonl")?;
        let backed = evs.iter().any(|e| {
            e.recipe_id == recipe_id
                && e.report.verdict == Verdict::Accept
                && e.report.candidate_digest == r.candidate_hash
                && e.report.contract_digest == inputs.contract_digest
                && e.target_digest == inputs.target_digest
                && e.compiler_id == inputs.compiler_id
                && e.policy_digest == inputs.policy_digest
                && e.evaluator_version == inputs.evaluator_version
        });
        if !backed {
            return Err(Error::ApprovalRefused(
                "no accepted evidence under exactly these inputs".into(),
            ));
        }
        let key = inputs.key_for(&r.candidate_hash);
        self.append(
            "approvals.jsonl",
            &ApprovalRecord {
                key: key.clone(),
                recipe_id: recipe_id.into(),
            },
        )?;
        Ok(key)
    }

    pub fn approvals(&self) -> Result<ApprovalBook, Error> {
        Ok(ApprovalBook::from_records(
            self.read::<ApprovalRecord>("approvals.jsonl")?,
        ))
    }

    /// Freeze the current contents for `tenant`, folding validated evidence
    /// into each recipe (accepts add evidence refs; rejects add failures with
    /// their scope and mark a recipe with no accept as rejected).
    pub fn freeze(&self, tenant: &str) -> Result<Snapshot, Error> {
        let mut recipes: Vec<Recipe> = self.read("recipes.jsonl")?;
        let evs: Vec<Evidence> = self.read("evidence.jsonl")?;
        for r in &mut recipes {
            let (mut accepted, mut rejected) = (false, false);
            for e in evs.iter().filter(|e| e.recipe_id == r.id) {
                let rep = &e.report;
                r.evidence.push(EvidenceRef {
                    sample_path: e.sample_path.clone(),
                    ci95_low: rep.speedup_ci95_low,
                    ci95_high: rep.speedup_ci95_high,
                    samples: rep.samples,
                    cost_micro_usd: e.cost_micro_usd,
                });
                match &rep.verdict {
                    Verdict::Accept => accepted = true,
                    Verdict::Reject(why) => {
                        rejected = true;
                        r.failures.push(Failure {
                            reason: format!("{why:?}"),
                            scope: e.scope.clone(),
                        });
                    }
                }
            }
            if rejected && !accepted {
                r.status = Status::Rejected;
            }
        }
        Ok(Snapshot::from_recipes(tenant, recipes))
    }
}

fn read_lines<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, Error> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(Error::io(path, e)),
    };
    // A final line without '\n' is a torn write: ignore it.
    let complete = match text.rfind('\n') {
        Some(i) => &text[..i],
        None => return Ok(vec![]),
    };
    complete
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).map_err(Error::from))
        .collect()
}
