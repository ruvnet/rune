//! Markdown rendering helpers for the study report.

use crate::analyze::{Pair, RecipeInfo, K};
use crate::gen::Manifest;
use crate::recipes::ALL;
use rune_memory::control::ControlStudy;
use std::collections::HashMap;

fn code(p: &Pair) -> String {
    let c = match p.first.outcome.as_str() {
        "accept" => match &p.rerun {
            Some(r) if !r.accepted() => "c",
            _ => "A",
        },
        "insufficient_improvement" => "i",
        "guard_regression" => "g",
        "holdout_failed" => "H",
        "visible_tests_failed" => "V",
        "build_failed" => "B",
        "new_unsafe" => "U",
        "new_dependency" | "new_capability" => "P",
        "timeout" => "T",
        _ => "M",
    };
    c.into()
}

pub fn matrix_table(m: &Manifest, pairs: &[Pair]) -> String {
    let by: HashMap<(&str, &str), &Pair> = pairs
        .iter()
        .map(|p| ((p.workload.as_str(), p.recipe.as_str()), p))
        .collect();
    let mut s = String::from("| workload | split |");
    for r in ALL {
        s += &format!(" {} |", r.id());
    }
    s += "\n|---|---|";
    for _ in ALL {
        s += "---|";
    }
    s += "\n";
    for w in &m.workloads {
        s += &format!(
            "| {} | {} |",
            w.name,
            if w.training { "train" } else { "unseen" }
        );
        for r in ALL {
            match by.get(&(w.name.as_str(), r.id())) {
                Some(p) => s += &format!(" {} {:.2} |", code(p), p.first.ci95_low),
                None => s += " . |",
            }
        }
        s += "\n";
    }
    s += "\nCell = outcome code and first-run speedup CI lower bound. A accept; c accept in the first run but not confirmed by the re-run; i insufficient improvement; g guard regression; H holdout failed; V visible tests failed; B build failed; U new unsafe; P dependency/capability; T timeout; M missing evidence; . recipe not applicable (not evaluated).\n";
    s
}

pub fn stats_md(
    m: &Manifest,
    pairs: &[Pair],
    study: &ControlStudy,
    infos: &[RecipeInfo],
) -> String {
    let mut s = String::from("## Descriptive statistics\n\n| recipe | applicable | accepted (train) | accepted (unseen) | unseen applicable | origin (training workload) |\n|---|---|---|---|---|---|\n");
    let train: HashMap<&str, bool> = m
        .workloads
        .iter()
        .map(|w| (w.name.as_str(), w.training))
        .collect();
    for r in ALL {
        let ps: Vec<&Pair> = pairs.iter().filter(|p| p.recipe == r.id()).collect();
        let cnt = |t: bool, a: bool| {
            ps.iter()
                .filter(|p| train[p.workload.as_str()] == t && (!a || p.final_accept()))
                .count()
        };
        let info = infos.iter().find(|i| i.id == r.id()).unwrap();
        s += &format!(
            "| {} | {} | {}/{} | {}/{} | {} | {} |\n",
            r.id(),
            ps.len(),
            cnt(true, true),
            cnt(true, false),
            cnt(false, true),
            cnt(false, false),
            cnt(false, false),
            info.origin
                .clone()
                .unwrap_or_else(|| "none (not in library)".into())
        );
    }
    let test: Vec<usize> = (0..study.workloads.len())
        .filter(|&w| !study.workloads[w].training)
        .collect();
    let any = test
        .iter()
        .filter(|&&w| study.outcomes.iter().any(|row| row[w]))
        .count();
    let k = K.min(study.recipes.len());
    let per_w: Vec<usize> = test
        .iter()
        .map(|&w| study.outcomes.iter().filter(|row| row[w]).count())
        .collect();
    s += &format!(
        "\nLibrary of {} recipes; {} of {} unseen workloads have at least one library recipe accepted (the ceiling for any proposer with this library). Mean number of accepting library recipes per unseen workload: {:.2} of {}. Budget k = {k}.\n",
        study.recipes.len(), any, test.len(),
        per_w.iter().sum::<usize>() as f64 / test.len().max(1) as f64, study.recipes.len()
    );
    s
}
