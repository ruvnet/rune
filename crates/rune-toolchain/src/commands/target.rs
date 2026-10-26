//! `rune target`: hardware descriptors, cross builds, and evidence-classified runs (ADR 006).

use super::common::{descriptor, evidence_name, perf_line};
use super::Failure;
use clap::{Args as ClapArgs, Subcommand};
use rune_core::target::{Evidence, TargetDescriptor};
use rune_target::cross::{cross_build, CrossArtifact, CrossOptions};
use rune_target::descriptors::RVM_UNVALIDATED_REASON;
use rune_target::exec::{executor_for, run_and_classify};
use rune_target::host::Host;
use std::path::{Path, PathBuf};

const NAMES: [&str; 4] = ["x86_64", "aarch64", "wasm32", "rvm"];

#[derive(Subcommand)]
pub enum Cmd {
    /// List the target descriptors and whether an executor is available here
    List,
    /// Cross-build the workspace for a target (evidence: compile_only)
    Build(TargetArgs),
    /// Build, run and classify the evidence (native, emulated, compile_only, unvalidated)
    Run {
        #[command(flatten)]
        t: TargetArgs,
        /// Arguments passed to the built program (after `--`)
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(ClapArgs, Clone)]
pub struct TargetArgs {
    /// x86_64, aarch64 (qemu), wasm32 (node-wasi), rvm, or a full triple
    #[arg(long)]
    pub target: String,
    /// Required CPU features of the target (native and aarch64 only)
    #[arg(long = "cpu-feature")]
    pub cpu_features: Vec<String>,
    /// Binary to build when the workspace has several
    #[arg(long)]
    pub bin: Option<String>,
}

pub(super) fn run(ws: &Path, cmd: Cmd) -> Result<(), Failure> {
    match cmd {
        Cmd::List => list(),
        Cmd::Build(t) => {
            let (desc, art) = build(ws, &t)?;
            println!(
                "built {} for {} sha256:{}\nevidence: {}\n{}",
                art.path.display(),
                desc.triple,
                art.digest,
                evidence_name(Evidence::CompileOnly),
                perf_line(Evidence::CompileOnly)
            );
            Ok(())
        }
        Cmd::Run { t, args } => run_target(ws, &t, &args),
    }
}

fn list() -> Result<(), Failure> {
    let host = Host::detect();
    println!("host {} ({})", host.triple, host.abi);
    for name in NAMES {
        let d = descriptor(name, &[])?;
        let exec = executor_for(&d);
        let (status, class) = match exec.availability() {
            Ok(()) => ("available".to_owned(), exec.evidence()),
            Err(why) => (format!("unavailable ({why})"), Evidence::Unvalidated),
        };
        println!(
            "{name:<8} triple {} runtime {} abi {} executor {status}\n         runs classify as: {}; {}",
            d.triple,
            d.runtime,
            d.abi,
            evidence_name(class),
            perf_line(class)
        );
    }
    Ok(())
}

fn build(ws: &Path, t: &TargetArgs) -> Result<(TargetDescriptor, CrossArtifact), Failure> {
    let desc = descriptor(&t.target, &t.cpu_features)?;
    if desc.runtime == "unavailable" {
        return Err(Failure::Other(format!(
            "target {} cannot be built: {RVM_UNVALIDATED_REASON}",
            desc.triple
        )));
    }
    let dir: PathBuf = ws.join("target").join(format!("rune-{}", desc.triple));
    let opts = CrossOptions {
        rustflags: vec![],
        bin: t.bin.clone(),
    };
    let art = cross_build(ws, &dir, &desc, &opts)?;
    Ok((desc, art))
}

fn run_target(ws: &Path, t: &TargetArgs, args: &[String]) -> Result<(), Failure> {
    let desc = descriptor(&t.target, &t.cpu_features)?;
    let host = Host::detect();
    println!("target {} (runtime {})", desc.triple, desc.runtime);
    // No executor means nothing is built or run: unvalidated, with the reason.
    if let Err(why) = executor_for(&desc).availability() {
        println!(
            "evidence: {}\nnote: {why}\n{}",
            evidence_name(Evidence::Unvalidated),
            perf_line(Evidence::Unvalidated)
        );
        return Ok(());
    }
    let (_, art) = build(ws, t)?;
    println!("built {} sha256:{}", art.path.display(), art.digest);
    let out = run_and_classify(&desc, &host, &art.path, args)?;
    println!("evidence: {}", evidence_name(out.evidence));
    if let Some(n) = &out.note {
        println!("note: {n}");
    }
    let mut failed = None;
    if let Some(o) = &out.output {
        print!("{}", String::from_utf8_lossy(&o.stdout));
        eprint!("{}", String::from_utf8_lossy(&o.stderr));
        if !o.status.success() {
            failed = Some(o.status.to_string());
        }
    }
    println!("{}", perf_line(out.evidence));
    match failed {
        Some(s) => Err(Failure::Other(format!("program exited with {s}"))),
        None => Ok(()),
    }
}
