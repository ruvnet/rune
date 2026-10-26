//! `rune package` and `rune verify-artifact`: the evidence-directory artifact
//! envelope (ADR 006). It is an evidence directory, not an RVF container. A
//! signature proves provenance and integrity, not correctness.

use super::common::{descriptor, evidence_name, perf_line, short, ReportFile};
use super::{target::TargetArgs, Failure};
use clap::Args as ClapArgs;
use rune_cargo as cargo;
use rune_core::target::Evidence;
use rune_core::{tree_digest, Store};
use rune_target::cross::{cross_build, CrossOptions};
use rune_target::envelope::{write_envelope, EnvelopeInputs, TestSummary};
use rune_target::exec::{executor_for, run_and_classify};
use rune_target::gate::{check_loadable, LoadStatus};
use rune_target::host::Host;
use rune_target::sign::load_verifying_key;
use rune_target::verify::{verify_envelope, SignatureStatus, VerifyOptions};
use std::path::{Path, PathBuf};

const NOT_CORRECTNESS: &str =
    "note: a signature proves provenance and integrity, not correctness; this is an evidence directory, not RVF";

#[derive(ClapArgs)]
pub struct Args {
    #[command(flatten)]
    t: TargetArgs,
    /// Ed25519 signing key FILE (32 raw bytes or 64 hex chars). Rune never generates keys.
    #[arg(long)]
    sign_key: Option<PathBuf>,
    /// Contract the artifact was evaluated under (default: the latest recorded report for this source)
    #[arg(long)]
    contract: Option<PathBuf>,
    /// Do not execute the artifact: evidence stays compile_only
    #[arg(long)]
    no_run: bool,
}

#[derive(ClapArgs)]
pub struct VerifyArgs {
    /// Artifact evidence directory
    dir: PathBuf,
    /// Public key FILE you already trust (32 raw bytes or 64 hex chars)
    #[arg(long)]
    trusted_pubkey: Option<PathBuf>,
    /// Fail when the artifact is unsigned
    #[arg(long)]
    require_signature: bool,
}

fn contract_digest(ws: &Path, plan_source: &str, from: &Option<PathBuf>) -> String {
    if let Some(p) = from {
        return super::common::load_contract(p)
            .map(|c| c.digest())
            .unwrap_or_default();
    }
    let latest = std::fs::read_to_string(ws.join(".rune/eval/latest")).unwrap_or_default();
    let name = latest.trim();
    if name.is_empty() || name.contains('/') {
        return String::new();
    }
    ReportFile::load(&ws.join(".rune/eval/reports").join(name))
        .ok()
        .filter(|r| r.report.candidate_digest == plan_source)
        .map(|r| r.report.contract_digest)
        .unwrap_or_default()
}

pub(super) fn package(ws: &Path, store: &Store, a: &Args) -> Result<(), Failure> {
    let plan = store.current()?;
    let id = plan.id();
    let desc = descriptor(&a.t.target, &a.t.cpu_features)?;
    if desc.runtime == "unavailable" {
        return Err(Failure::Other(format!(
            "target {} cannot be packaged: specification required",
            desc.triple
        )));
    }
    if plan.target != desc.triple {
        return Err(Failure::Other(format!(
            "the current plan targets {} but --target is {}; approve a plan for that target first (`rune promote --target`)",
            plan.target, desc.triple
        )));
    }
    plan.check_fresh(
        &tree_digest(ws)?,
        &cargo::lock_digest(ws)?,
        &cargo::toolchain_id()?,
    )?;
    let dir = ws.join("target").join(format!("rune-{}", desc.triple));
    let opts = CrossOptions {
        rustflags: vec![],
        bin: a.t.bin.clone(),
    };
    let art = cross_build(ws, &dir, &desc, &opts)?;
    let (evidence, tests) = if a.no_run || executor_for(&desc).availability().is_err() {
        let why = if a.no_run { "--no-run" } else { "no executor" };
        let ev = if a.no_run {
            Evidence::CompileOnly
        } else {
            Evidence::Unvalidated
        };
        (
            ev,
            TestSummary {
                command: format!("not executed ({why})"),
                passed: 0,
                failed: 0,
                ignored: 1,
            },
        )
    } else {
        let out = run_and_classify(&desc, &Host::detect(), &art.path, &[])?;
        let (passed, failed) = match &out.output {
            Some(o) if o.status.success() => (1, 0),
            Some(_) => (0, 1),
            None => (0, 0),
        };
        (
            out.evidence,
            TestSummary {
                command: format!("smoke run on {}", desc.triple),
                passed,
                failed,
                ignored: u32::from(out.output.is_none()),
            },
        )
    };
    let name = art
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".into());
    let out_dir = ws.join(".rune/artifacts").join(&id);
    let inputs = EnvelopeInputs {
        plan: &plan,
        target: &desc,
        evidence,
        tests,
        contract_digest: contract_digest(ws, &plan.source_digest, &a.contract),
        executables: vec![(name, art.path.clone())],
        files: vec![],
    };
    let m = write_envelope(&out_dir, &inputs, a.sign_key.as_deref())?;
    println!(
        "packaged plan {} -> {}\n  target {} evidence {} ({})\n  {}\n{NOT_CORRECTNESS}",
        short(&id),
        out_dir.display(),
        m.target.triple,
        evidence_name(m.evidence),
        perf_line(m.evidence),
        if a.sign_key.is_some() {
            "signed"
        } else {
            "unsigned"
        },
    );
    Ok(())
}

pub(super) fn verify_artifact(a: &VerifyArgs) -> Result<(), Failure> {
    let trusted = a
        .trusted_pubkey
        .as_deref()
        .map(load_verifying_key)
        .transpose()?;
    let rep = verify_envelope(
        &a.dir,
        &VerifyOptions {
            trusted_key: trusted,
            require_signature: a.require_signature,
        },
    )?;
    let sig = match rep.signature {
        SignatureStatus::Absent => "absent (unsigned)",
        SignatureStatus::Trusted => "valid, made by the trusted key",
        SignatureStatus::SelfAsserted => {
            "valid against its embedded key only (integrity, not identity)"
        }
    };
    let m = &rep.manifest;
    println!(
        "envelope ok: {} files match their digests\n  plan {} target {} recorded evidence {}\n  signature: {sig}",
        rep.files_checked,
        short(&m.plan_id),
        m.target.triple,
        evidence_name(m.evidence)
    );
    println!("{NOT_CORRECTNESS}");
    match check_loadable(m, &Host::detect()) {
        Ok(LoadStatus::Loadable { evidence }) => {
            println!(
                "load gate: loadable here; running it would be {} evidence\n{}",
                evidence_name(evidence),
                perf_line(evidence)
            );
            Ok(())
        }
        Ok(LoadStatus::Unvalidated { reason }) => {
            println!(
                "load gate: unvalidated ({reason})\n{}",
                perf_line(Evidence::Unvalidated)
            );
            Ok(())
        }
        Err(e) => Err(Failure::Other(format!("load gate BLOCKED: {e}"))),
    }
}
