//! `rune-study analyze`: assemble the outcome matrix from the evaluator's
//! reports and run the ADR 004 `ControlStudy` with `HashedFeatureRanker`.

use crate::gen::{io, write, Manifest, WorkloadEntry, R};
use crate::recipes::ALL;
use crate::render::{matrix_table, stats_md};
use crate::run::read_manifest;
use rune_core::contract::Verdict;
use rune_memory::control::{ControlReport, ControlStudy, RecipeSpec, WorkloadSpec};
use rune_memory::{Fingerprint, HashedFeatureRanker};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// First-run Accepts whose CI lower bound is within 2% of the 1.10 threshold
/// are re-run once; the final outcome needs both runs to Accept.
pub const BORDER_LO: f64 = 1.08;
pub const BORDER_HI: f64 = 1.12;
pub const K: usize = 3;
pub const TRIALS: u32 = 10_000;
pub const SEED: u64 = 0x5255_4E45_5354_5544;

#[derive(Deserialize, Clone, Debug)]
pub struct Rep {
    pub samples: u32,
    pub speedup_mean: f64,
    pub speedup_ci95_low: f64,
    pub speedup_ci95_high: f64,
    pub verdict: Verdict,
    #[serde(default)]
    pub evidence: serde_json::Value,
}

pub fn load_rep(p: &Path) -> R<Rep> {
    let b = io(fs::read(p), p)?;
    serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", p.display()))
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Summary {
    pub outcome: String,
    pub detail: String,
    pub samples: u32,
    pub geo_mean_speedup: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
    pub pinned_core: Option<u64>,
    pub load_before: Option<f64>,
    pub load_after: Option<f64>,
    pub guards: Vec<(String, f64)>,
}

impl Summary {
    pub fn accepted(&self) -> bool {
        self.outcome == "accept"
    }

    pub fn from_rep(r: &Rep) -> Summary {
        let (outcome, detail) = match &r.verdict {
            Verdict::Accept => ("accept".to_string(), String::new()),
            Verdict::Reject(x) => {
                let v = serde_json::to_value(x).unwrap_or_default();
                (
                    v["reason"].as_str().unwrap_or("reject").to_string(),
                    v["detail"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(300)
                        .collect(),
                )
            }
        };
        let e = &r.evidence;
        let guards = e["metrics"]
            .as_array()
            .map(|ms| {
                ms.iter()
                    .filter(|m| m["role"] == "guard")
                    .filter_map(|m| {
                        Some((m["name"].as_str()?.to_string(), m["ci95_low"].as_f64()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Summary {
            outcome,
            detail,
            samples: r.samples,
            geo_mean_speedup: r.speedup_mean,
            ci95_low: r.speedup_ci95_low,
            ci95_high: r.speedup_ci95_high,
            pinned_core: e["pinning"]["core"].as_u64(),
            load_before: e["load_before"]["loadavg_1m"].as_f64(),
            load_after: e["load_after"]["loadavg_1m"].as_f64(),
            guards,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct Pair {
    pub workload: String,
    pub recipe: String,
    pub first: Summary,
    pub rerun: Option<Summary>,
}

impl Pair {
    pub fn borderline(&self) -> bool {
        self.first.accepted() && (BORDER_LO..=BORDER_HI).contains(&self.first.ci95_low)
    }
    pub fn first_accept(&self) -> bool {
        self.first.accepted()
    }
    /// Accept only if the first run accepted and any confirmation run did too.
    pub fn final_accept(&self) -> bool {
        self.first.accepted() && self.rerun.as_ref().is_none_or(Summary::accepted)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct RecipeInfo {
    pub id: String,
    pub descriptor: String,
    pub applicability: String,
    pub origin: Option<String>,
    pub origin_ci95_low: Option<f64>,
}

fn fingerprint(w: &WorkloadEntry, compiler: &str) -> Fingerprint {
    Fingerprint {
        source_tags: w.source_tags.clone(),
        target_triple: "x86_64-unknown-linux-gnu".into(),
        cpu_features: vec!["host-native".into()],
        compiler_id: compiler.into(),
        abi: "sysv64".into(),
        workload_tags: w.workload_tags.clone(),
        policy_digest: "rune-study-policy-v1".into(),
    }
}

/// Build the study input. A recipe enters the library only if it was accepted
/// on at least one TRAINING workload; its origin (and thus fingerprint) is the
/// training workload with the highest first-run CI lower bound, ties by name.
pub fn build_study(
    m: &Manifest,
    pairs: &[Pair],
    use_final: bool,
    compiler: &str,
) -> (ControlStudy, Vec<RecipeInfo>) {
    let acc: HashMap<(&str, &str), bool> = pairs
        .iter()
        .map(|p| {
            let a = if use_final {
                p.final_accept()
            } else {
                p.first_accept()
            };
            ((p.workload.as_str(), p.recipe.as_str()), a)
        })
        .collect();
    let low: HashMap<(&str, &str), f64> = pairs
        .iter()
        .map(|p| ((p.workload.as_str(), p.recipe.as_str()), p.first.ci95_low))
        .collect();
    let workloads: Vec<WorkloadSpec> = m
        .workloads
        .iter()
        .map(|w| WorkloadSpec {
            name: w.name.clone(),
            training: w.training,
            fingerprint: fingerprint(w, compiler),
        })
        .collect();
    let (mut specs, mut rows, mut infos) = (Vec::new(), Vec::new(), Vec::new());
    for r in ALL {
        let origin = m
            .workloads
            .iter()
            .filter(|w| w.training && acc.get(&(w.name.as_str(), r.id())) == Some(&true))
            .max_by(|a, b| {
                let (la, lb) = (
                    low[&(a.name.as_str(), r.id())],
                    low[&(b.name.as_str(), r.id())],
                );
                la.total_cmp(&lb).then_with(|| b.name.cmp(&a.name))
            });
        infos.push(RecipeInfo {
            id: r.id().into(),
            descriptor: r.descriptor().into(),
            applicability: r.applicability().into(),
            origin: origin.map(|w| w.name.clone()),
            origin_ci95_low: origin.map(|w| low[&(w.name.as_str(), r.id())]),
        });
        if let Some(o) = origin {
            specs.push(RecipeSpec {
                id: r.id().into(),
                origin: o.name.clone(),
                fingerprint: fingerprint(o, compiler),
            });
            rows.push(
                m.workloads
                    .iter()
                    .map(|w| acc.get(&(w.name.as_str(), r.id())) == Some(&true))
                    .collect(),
            );
        }
    }
    (
        ControlStudy {
            k: K,
            trials: TRIALS,
            seed: SEED,
            workloads,
            recipes: specs,
            outcomes: rows,
        },
        infos,
    )
}

fn compiler_id() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "rustc-unknown".into())
}

pub fn collect_pairs(study: &Path, m: &Manifest) -> R<Vec<Pair>> {
    let mut pairs = Vec::new();
    let mut missing = Vec::new();
    for w in &m.workloads {
        for c in &w.candidates {
            let d = study.join("reports").join(&w.name);
            let p1 = d.join(format!("{}.json", c.recipe));
            let Ok(first) = load_rep(&p1) else {
                missing.push(format!("{}/{}", w.name, c.recipe));
                continue;
            };
            let rerun = load_rep(&d.join(format!("{}.rerun.json", c.recipe)))
                .ok()
                .map(|r| Summary::from_rep(&r));
            let pair = Pair {
                workload: w.name.clone(),
                recipe: c.recipe.clone(),
                first: Summary::from_rep(&first),
                rerun,
            };
            if pair.borderline() && pair.rerun.is_none() {
                missing.push(format!("{}/{} (confirmation re-run)", w.name, c.recipe));
            }
            pairs.push(pair);
        }
    }
    if missing.is_empty() {
        Ok(pairs)
    } else {
        Err(format!(
            "sweep incomplete: {} missing, e.g. {:?}",
            missing.len(),
            &missing[..missing.len().min(5)]
        ))
    }
}

pub fn analyze(study_dir: &Path) -> R<()> {
    let m = read_manifest(study_dir)?;
    let pairs = collect_pairs(study_dir, &m)?;
    let compiler = compiler_id();
    let ranker = HashedFeatureRanker;
    let (study, infos) = build_study(&m, &pairs, true, &compiler);
    let report: ControlReport = study.run(&ranker).map_err(|e| e.to_string())?;
    let (study_first, _) = build_study(&m, &pairs, false, &compiler);
    let report_first = study_first.run(&ranker).map_err(|e| e.to_string())?;
    fn json<T: Serialize>(v: &T) -> String {
        serde_json::to_string_pretty(v).unwrap_or_default() + "\n"
    }
    write(&study_dir.join("control-study.json"), &json(&study))?;
    write(&study_dir.join("control-report.json"), &json(&report))?;
    write(
        &study_dir.join("outcome-matrix.json"),
        &json(&Matrix {
            compiler: compiler.clone(),
            recipes: infos.clone(),
            pairs: pairs.clone(),
        }),
    )?;
    let mut explore = String::new();
    for k in [1usize, 2] {
        let mut s = study.clone();
        s.k = k;
        let r = s.run(&ranker).map_err(|e| e.to_string())?;
        explore += &format!(
            "| {k} | {:?} | {:.1}% [{:.1}%, {:.1}%] | {:.1}% [{:.1}%, {:.1}%] | {:.4} |\n",
            r.verdict,
            100.0 * r.retrieval_hit_rate,
            100.0 * r.retrieval_wilson95.0,
            100.0 * r.retrieval_wilson95.1,
            100.0 * r.random_hit_rate,
            100.0 * r.random_wilson95.0,
            100.0 * r.random_wilson95.1,
            r.p_value
        );
    }
    let n_borderline = pairs.iter().filter(|p| p.borderline()).count();
    let n_unconfirmed = pairs
        .iter()
        .filter(|p| p.borderline() && !p.final_accept())
        .count();
    let md = format!(
        "{}\n{}\n### Exploratory, post hoc (NOT preregistered; cannot change the verdict)\n\nThe preregistered k = 3 equals the library size, so both arms evaluate the whole library and are identical by construction. Smaller budgets on the same outcomes:\n\n| k | verdict | retrieval hit@k (Wilson 95%) | random hit@k (Wilson 95%) | p |\n|---|---|---|---|---|\n{explore}\n### Sensitivity (not the preregistered analysis)\n\nUsing first-run outcomes without the confirmation re-runs ({n_borderline} borderline Accepts, {n_unconfirmed} not confirmed): verdict {:?}, retrieval {:.1}% vs random {:.1}%, p = {:.4}.\n\n## Outcome matrix\n\n{}\nCompiler: {compiler}. Ranker: HashedFeatureRanker. Evaluations in matrix: {}.\n",
        report.render_markdown(),
        stats_md(&m, &pairs, &study, &infos),
        report_first.verdict,
        100.0 * report_first.retrieval_hit_rate,
        100.0 * report_first.random_hit_rate,
        report_first.p_value,
        matrix_table(&m, &pairs),
        pairs.len() + pairs.iter().filter(|p| p.rerun.is_some()).count()
    );
    write(&study_dir.join("control-report.md"), &md)?;
    println!("{md}");
    Ok(())
}

#[derive(Serialize)]
struct Matrix {
    compiler: String,
    recipes: Vec<RecipeInfo>,
    pairs: Vec<Pair>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen::generate;

    fn fake(w: &str, r: &str, outcome: &str, low: f64) -> Pair {
        Pair {
            workload: w.into(),
            recipe: r.into(),
            first: Summary {
                outcome: outcome.into(),
                detail: String::new(),
                samples: 40,
                geo_mean_speedup: low + 0.05,
                ci95_low: low,
                ci95_high: low + 0.1,
                pinned_core: Some(3),
                load_before: Some(1.0),
                load_after: Some(1.0),
                guards: vec![],
            },
            rerun: None,
        }
    }

    #[test]
    fn matrix_assembly_origin_and_study_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let m = generate(
            &d.path().join("w"),
            &d.path().join("c"),
            &d.path().join("m.json"),
        )
        .unwrap();
        // fake evaluator: every unroll4 candidate accepts, everything else is too slow
        let mut pairs = Vec::new();
        for w in &m.workloads {
            for c in &w.candidates {
                pairs.push(if c.recipe == "unroll4" {
                    fake(&w.name, &c.recipe, "accept", 2.0)
                } else {
                    fake(&w.name, &c.recipe, "insufficient_improvement", 0.9)
                });
            }
        }
        let (study, infos) = build_study(&m, &pairs, true, "rustc-test");
        assert_eq!(
            study.recipes.len(),
            1,
            "only unroll4 was accepted on training"
        );
        let origin = &study.recipes[0].origin;
        assert!(m.workloads.iter().any(|w| &w.name == origin && w.training));
        assert_eq!(infos.iter().filter(|i| i.origin.is_some()).count(), 1);
        let back = ControlStudy::from_json(&serde_json::to_string(&study).unwrap()).unwrap();
        let rep = back.run(&HashedFeatureRanker).unwrap();
        // a one-recipe library cannot support a claim (library must exceed k)
        assert_eq!(rep.verdict, rep_verdict_insufficient());
        assert!(rep.n_workloads >= 20);
    }

    fn rep_verdict_insufficient() -> rune_memory::control::Verdict {
        rune_memory::control::Verdict::Insufficient
    }

    #[test]
    fn confirmation_rule_needs_both_runs() {
        let mut p = fake("w", "r", "accept", 1.09);
        assert!(p.borderline() && p.final_accept());
        p.rerun = Some(fake("w", "r", "insufficient_improvement", 1.05).first);
        assert!(!p.final_accept() && p.first_accept());
        assert!(!fake("w", "r", "accept", 1.5).borderline());
    }
}
