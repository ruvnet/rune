//! `rune cleanup`: quarantine-first cleanup of unreachable GENERATED objects
//! (ADR 007). Dry run unless `--apply`. Source files are never in the
//! ownership manifest, so they can never be candidates.

use super::common::now_unix;
use super::Failure;
use clap::Args as ClapArgs;
use rune_gc::{plan_cleanup, purge, quarantine, restore, Manifest, Mode, Report};
use std::path::Path;

#[derive(ClapArgs)]
pub struct Args {
    /// Actually move unreachable generated objects into quarantine (and, with --purge, delete expired ones)
    #[arg(long)]
    apply: bool,
    /// Purge phase: remove only quarantined objects older than 7 days (a dry run unless --apply)
    #[arg(long)]
    purge: bool,
    /// Undo one quarantine batch
    #[arg(long, value_name = "BATCH", conflicts_with_all = ["apply", "purge"])]
    restore: Option<u64>,
}

fn show(label: &str, r: &Report) {
    println!("{label}: {} object(s)", r.done.len());
    for k in &r.done {
        println!("  {k}");
    }
    for k in &r.restored {
        println!("  restored (reachable again): {k}");
    }
    for (k, why) in &r.skipped {
        println!("  skipped {k}: {why}");
    }
}

pub(super) fn run(ws: &Path, a: &Args) -> Result<(), Failure> {
    if let Some(batch) = a.restore {
        let r = restore(ws, batch)?;
        println!("restored batch {batch}: {} object(s)", r.restored.len());
        for k in &r.restored {
            println!("  {k}");
        }
        for (k, why) in &r.skipped {
            println!("  skipped {k}: {why}");
        }
        return Ok(());
    }
    let now = now_unix();
    let mode = if a.apply { Mode::Apply } else { Mode::DryRun };
    let plan = plan_cleanup(ws)?;
    println!(
        "cleanup plan ({}): {} unreachable, {} retained, {} refused",
        if a.apply {
            "apply"
        } else {
            "dry run, nothing changes"
        },
        plan.candidates.len(),
        plan.retained.len(),
        plan.rejected.len()
    );
    for c in &plan.candidates {
        println!(
            "  collect  {} [{:?}, {} bytes]: {}",
            c.key, c.kind, c.size, c.reason
        );
    }
    for (k, why) in &plan.retained {
        println!("  keep     {k}: {why}");
    }
    for (k, why) in &plan.rejected {
        println!("  refused  {k}: {why}");
    }
    for job in &plan.stale_leases {
        println!("  stale lease (owner is gone, ignored): {job}");
    }
    if a.apply {
        let r = quarantine(&plan, now, Mode::Apply)?;
        show("quarantined", &r);
        match r.batch {
            Some(b) => println!("undo with: rune cleanup --restore {b}"),
            None => println!("nothing was quarantined"),
        }
    }
    let held = Manifest::load(ws)?.quarantine.len();
    if a.purge {
        let r = purge(ws, now, mode)?;
        show(
            if a.apply {
                "purged (older than 7 days)"
            } else {
                "would purge (older than 7 days; dry run)"
            },
            &r,
        );
    } else if held > 0 {
        println!("{held} object(s) in quarantine; `--purge` removes those older than 7 days");
    }
    Ok(())
}
