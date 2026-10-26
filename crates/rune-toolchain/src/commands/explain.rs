//! `rune explain`: the approved plan lineage plus every kind of evidence the
//! workspace holds. Each section is skipped quietly when its data is absent,
//! and nothing here writes.

use super::common::{evidence_name, perf_line, short, ReportFile};
use super::Failure;
use rune_core::contract::Verdict;
use rune_core::{Ledger, Policy, Store};
use rune_target::envelope::{Manifest, MANIFEST_FILE};
use rune_target::verify::{verify_envelope, SignatureStatus, VerifyOptions};
use std::fs;
use std::path::Path;

pub(super) fn explain(ws: &Path, store: &Store) -> Result<(), Failure> {
    let mut next = store.current_id()?;
    if next.is_none() {
        return Err(Failure::Other("no approved plan".into()));
    }
    let policy = store.current()?.policy;
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
        approval(ws, &id);
        envelope(ws, &id);
        next = plan.parent;
    }
    ledger(ws, &policy)?;
    latest_report(ws);
    memory_approvals(ws);
    Ok(())
}

fn approval(ws: &Path, plan_id: &str) {
    let path = ws.join(format!(".rune/promotions/{plan_id}.json"));
    let Ok(bytes) = fs::read(path) else {
        println!("  approval   none recorded for this plan");
        return;
    };
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let key = v["approval_key"].as_str().unwrap_or("?");
    println!(
        "  approval   key {} (candidate {}, evaluator {})",
        short(key),
        short(v["candidate_digest"].as_str().unwrap_or("?")),
        v["inputs"]["evaluator_version"].as_str().unwrap_or("?")
    );
}

fn envelope(ws: &Path, plan_id: &str) {
    let dir = ws.join(format!(".rune/artifacts/{plan_id}"));
    if !dir.join(MANIFEST_FILE).exists() {
        println!("  target     evidence class: none (not packaged)\n  envelope   not packaged");
        return;
    }
    match verify_envelope(&dir, &VerifyOptions::default()) {
        Ok(rep) => {
            let m: &Manifest = &rep.manifest;
            let sig = match rep.signature {
                SignatureStatus::Absent => "unsigned",
                SignatureStatus::Trusted => "signed (trusted key)",
                SignatureStatus::SelfAsserted => "signed (self-asserted key; integrity only)",
            };
            println!(
                "  target     {} evidence class: {} ({})",
                m.target.triple,
                evidence_name(m.evidence),
                perf_line(m.evidence)
            );
            println!(
                "  envelope   verified, {} files, {sig}: {}",
                rep.files_checked,
                dir.display()
            );
        }
        Err(e) => println!("  envelope   FAILED verification: {e}"),
    }
}

fn ledger(ws: &Path, policy: &Policy) -> Result<(), Failure> {
    let Ok(rd) = fs::read_dir(ws.join(".rune/ledger")) else {
        return Ok(());
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".jsonl")
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    let (mut spent, mut tokens) = (0u64, 0u64);
    for name in &names {
        let Ok(l) = Ledger::open(ws, name, policy) else {
            continue;
        };
        let t = l.totals()?;
        spent += t.spent_usd_micros;
        tokens += t.spent_tokens;
        println!(
            "ledger campaign {name}: spent ${:.6} / {} tokens, open reservations ${:.6}, overrun ${:.6}",
            t.spent_usd_micros as f64 / 1e6,
            t.spent_tokens,
            t.open_reserved_usd_micros as f64 / 1e6,
            t.overrun_usd_micros as f64 / 1e6
        );
    }
    if !names.is_empty() {
        println!(
            "ledger total: ${:.6} / {tokens} tokens across {} campaign(s)",
            spent as f64 / 1e6,
            names.len()
        );
    }
    Ok(())
}

fn latest_report(ws: &Path) {
    let dir = ws.join(".rune/eval");
    let Ok(name) = fs::read_to_string(dir.join("latest")) else {
        println!("eval report: none");
        return;
    };
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return;
    }
    match ReportFile::load(&dir.join("reports").join(name)) {
        Ok(r) => {
            let r = r.report;
            let verdict = match &r.verdict {
                Verdict::Accept => "accept".to_owned(),
                Verdict::Reject(why) => format!("reject ({why:?})"),
            };
            println!(
                "eval report: candidate {} speedup {:.3}x (95% CI {:.3}..{:.3}, {} samples), verdict {verdict}",
                short(&r.candidate_digest),
                r.speedup_mean,
                r.speedup_ci95_low,
                r.speedup_ci95_high,
                r.samples
            );
        }
        Err(Failure::Other(e)) => println!("eval report: unreadable ({e})"),
        Err(_) => {}
    }
}

fn memory_approvals(ws: &Path) {
    if let Ok(text) = fs::read_to_string(ws.join(".rune/memory/approvals.jsonl")) {
        let n = text.lines().filter(|l| !l.is_empty()).count();
        println!("memory approvals: {n} stored key record(s)");
    }
}
