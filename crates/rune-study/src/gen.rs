//! `rune-study gen`: write frozen workloads, contracts, holdouts and candidates.

use crate::code::{baseline_lib, CARGO_TOML};
use crate::harness::{bench_rs, holdout_rs, visible_rs};
use crate::recipes::{candidate_lib, ALL};
use crate::spec::{all, training_set, Kernel};
use rune_core::contract::{EvalContract, GuardBench};
use rune_core::{hex_sha256, tree_digest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub type R<T> = Result<T, String>;

pub fn io<T>(r: std::io::Result<T>, p: &Path) -> R<T> {
    r.map_err(|e| format!("{}: {e}", p.display()))
}

pub fn write(p: &Path, text: &str) -> R<()> {
    if let Some(d) = p.parent() {
        io(fs::create_dir_all(d), d)?;
    }
    io(fs::write(p, text), p)
}

fn cargo_run(arg: &str) -> Vec<String> {
    [
        "cargo",
        "run",
        "--release",
        "--offline",
        "--quiet",
        "--bin",
        "bench",
        "--",
        arg,
    ]
    .map(String::from)
    .to_vec()
}

pub fn contract_for(k: &Kernel) -> EvalContract {
    let t = |tail: &[&str]| -> Vec<String> {
        [
            "cargo",
            "test",
            "--release",
            "--offline",
            "--quiet",
            "--test",
        ]
        .iter()
        .chain(tail)
        .map(|s| s.to_string())
        .collect()
    };
    EvalContract {
        name: format!("rune-study/{}", k.name()),
        kernel_dir: ".".into(),
        test_cmd: t(&["visible"]),
        holdout_dir: "holdout".into(),
        holdout_dest: "tests".into(),
        holdout_cmd: t(&["holdout"]),
        bench_cmd: cargo_run("1024"),
        guard_benches: vec![
            GuardBench {
                name: "len7".into(),
                bench_cmd: cargo_run("7"),
                tolerance: 0.1,
            },
            GuardBench {
                name: "len1000003".into(),
                bench_cmd: cargo_run("1000003"),
                tolerance: 0.1,
            },
        ],
        samples: 40,
        warmup: 5,
        min_improvement: 0.1,
        forbid_unsafe: true,
        forbid_new_deps: true,
        forbid_new_capabilities: true,
        max_seconds: 300,
        max_memory_mb: 8192,
    }
}

/// Write one crate tree: shared harness + the given `lib.rs`.
fn write_crate(dir: &Path, k: &Kernel, lib: &str) -> R<()> {
    write(&dir.join("Cargo.toml"), CARGO_TOML)?;
    write(&dir.join("src/lib.rs"), lib)?;
    write(&dir.join("src/bin/bench.rs"), &bench_rs(k))?;
    write(&dir.join("tests/visible.rs"), &visible_rs(k))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadEntry {
    pub name: String,
    pub training: bool,
    pub elem: String,
    pub op: String,
    pub style: String,
    pub source_tags: Vec<String>,
    pub workload_tags: Vec<String>,
    pub contract_struct_digest: String,
    pub contract_file_sha256: String,
    pub baseline_tree_digest: String,
    pub holdout_tree_digest: String,
    pub candidates: Vec<CandidateEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateEntry {
    pub recipe: String,
    pub tree_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub workloads: Vec<WorkloadEntry>,
}

/// Generate everything. `workloads` holds baseline/contract/holdout per
/// workload (committed); `candidates` holds `<workload>/<recipe>/` (regenerable).
pub fn generate(workloads: &Path, candidates: &Path, manifest: &Path) -> R<Manifest> {
    let ks = all();
    let train = training_set(&ks);
    let mut entries = Vec::new();
    for k in &ks {
        let name = k.name();
        let wdir = workloads.join(&name);
        write_crate(&wdir.join("baseline"), k, &baseline_lib(k))?;
        write(&wdir.join("holdout/holdout.rs"), &holdout_rs(k))?;
        let c = contract_for(k);
        let text = serde_json::to_string_pretty(&c).map_err(|e| e.to_string())? + "\n";
        write(&wdir.join("contract.json"), &text)?;
        let mut cands = Vec::new();
        for r in ALL {
            if let Some(lib) = candidate_lib(r, k) {
                let cdir = candidates.join(&name).join(r.id());
                write_crate(&cdir, k, &lib)?;
                cands.push(CandidateEntry {
                    recipe: r.id().into(),
                    tree_digest: tree_digest(&cdir).map_err(|e| e.to_string())?,
                });
            }
        }
        entries.push(WorkloadEntry {
            name: name.clone(),
            training: train.contains(&name),
            elem: k.elem.ty().into(),
            op: k.op.name().into(),
            style: format!("{:?}", k.style).to_lowercase(),
            source_tags: k.source_tags(),
            workload_tags: k.workload_tags(),
            contract_struct_digest: c.digest(),
            contract_file_sha256: hex_sha256(text.as_bytes()),
            baseline_tree_digest: tree_digest(&wdir.join("baseline")).map_err(|e| e.to_string())?,
            holdout_tree_digest: tree_digest(&wdir.join("holdout")).map_err(|e| e.to_string())?,
            candidates: cands,
        });
    }
    let m = Manifest { workloads: entries };
    write(
        manifest,
        &(serde_json::to_string_pretty(&m).map_err(|e| e.to_string())? + "\n"),
    )?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_and_complete() {
        let d = tempfile::tempdir().unwrap();
        let gen = |tag: &str| {
            let root = d.path().join(tag);
            generate(&root.join("w"), &root.join("c"), &root.join("m.json")).unwrap()
        };
        let (a, b) = (gen("one"), gen("two"));
        assert_eq!(a.workloads.len(), 34);
        for (x, y) in a.workloads.iter().zip(&b.workloads) {
            assert_eq!(x.baseline_tree_digest, y.baseline_tree_digest);
            assert_eq!(x.holdout_tree_digest, y.holdout_tree_digest);
            assert_eq!(x.contract_struct_digest, y.contract_struct_digest);
            let (cx, cy): (Vec<_>, Vec<_>) = (
                x.candidates
                    .iter()
                    .map(|c| (&c.recipe, &c.tree_digest))
                    .collect(),
                y.candidates
                    .iter()
                    .map(|c| (&c.recipe, &c.tree_digest))
                    .collect(),
            );
            assert_eq!(cx, cy);
        }
        let unseen = a.workloads.iter().filter(|w| !w.training).count();
        assert!(unseen >= 20, "{unseen}");
        // every candidate keeps the baseline's harness files byte-identical
        let w = &a.workloads[0];
        let base = d.path().join("one/w").join(&w.name).join("baseline");
        let cand = d
            .path()
            .join("one/c")
            .join(&w.name)
            .join(&w.candidates[0].recipe);
        for f in ["src/bin/bench.rs", "tests/visible.rs", "Cargo.toml"] {
            assert_eq!(
                fs::read(base.join(f)).unwrap(),
                fs::read(cand.join(f)).unwrap()
            );
        }
        let src = fs::read_to_string(cand.join("src/lib.rs")).unwrap();
        assert!(!src.contains("unsafe"));
    }
}
