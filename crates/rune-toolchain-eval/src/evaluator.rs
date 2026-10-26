//! Rune's own independent evaluator (ADR 005).
//!
//! This is NOT MetaHarness: MetaHarness exposes no performance-evaluation API.
//! [`Evaluator`] is the seam where a MetaHarness adapter could be added later;
//! no such adapter exists here.
//!
//! The evaluator reads only the frozen contract, the two source trees and the
//! holdout directory the contract names. All three live outside the candidate
//! tree (enforced), and their digests are recorded in the report.

use crate::bench::{self, Metric, MetricKind};
use crate::gates::{self, read_tree};
use crate::harness::{harness_tampering, install_holdout};
use crate::report::{Evidence, FullReport, Step, RULES};
use crate::runner::{run_limited, Capabilities, Limits, RunResult, SandboxEnv};
use crate::sysinfo;
use crate::workspace::{resolve_inside, CowWorkspace};
use rune_core::contract::{EvalContract, EvalReport, Reject, Verdict};
use rune_core::{file_digest, hex_sha256, tree_digest, Error};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub trait Evaluator {
    fn name(&self) -> &'static str;
    fn evaluate(&self, req: &EvalRequest) -> Result<FullReport, Error>;
}

#[derive(Debug, Clone)]
pub struct EvalRequest {
    pub contract: PathBuf,
    pub baseline: PathBuf,
    pub candidate: PathBuf,
    pub workdir: PathBuf,
    /// If set, the contract's digest must equal this (tamper check).
    pub expect_contract_digest: Option<String>,
    pub cargo_home: Option<PathBuf>,
    /// Continue when netns/rlimit layers are unavailable (still reported).
    pub allow_degraded: bool,
    pub pin: bool,
}

impl EvalRequest {
    pub fn new(contract: &Path, baseline: &Path, candidate: &Path, workdir: &Path) -> Self {
        EvalRequest {
            contract: contract.into(),
            baseline: baseline.into(),
            candidate: candidate.into(),
            workdir: workdir.into(),
            expect_contract_digest: None,
            cargo_home: None,
            allow_degraded: false,
            pin: true,
        }
    }
}

pub struct RuneEvaluator;

enum Fail {
    Reject(Reject),
    Internal(Error),
}
impl From<Reject> for Fail {
    fn from(r: Reject) -> Self {
        Fail::Reject(r)
    }
}
impl From<Error> for Fail {
    fn from(e: Error) -> Self {
        Fail::Internal(e)
    }
}

fn tail(r: &RunResult) -> String {
    let s = format!("{}\n{}", r.stderr.trim_end(), r.stdout.trim_end());
    let s = s.trim();
    let start = s.len().saturating_sub(700);
    let start = (start..s.len())
        .find(|i| s.is_char_boundary(*i))
        .unwrap_or(0);
    format!("exit {:?}: {}", r.code, &s[start..])
}

fn canon(p: &Path) -> Result<PathBuf, Error> {
    fs::canonicalize(p).map_err(|e| Error::io(p, e))
}

struct Run<'a> {
    c: &'a EvalContract,
    req: &'a EvalRequest,
    ev: Evidence,
    layers: BTreeSet<String>,
    cargo_home: PathBuf,
    pin: Option<usize>,
}

impl Run<'_> {
    fn env(&self, role: &str) -> SandboxEnv {
        SandboxEnv::new(
            &self.req.workdir.join(format!("home-{role}")),
            &self.cargo_home,
        )
    }

    fn limits(&self, pin: Option<usize>, wall: u64) -> Limits {
        Limits {
            wall_secs: wall,
            mem_mb: self.c.max_memory_mb,
            cpu_secs: wall.saturating_add(10),
            deny_net: true,
            pin_core: pin,
        }
    }

    /// Run one contract-defined command; non-zero exit -> `mk(tail)`, timeout -> Timeout.
    fn step(
        &mut self,
        name: &str,
        role: &str,
        cmd: &[String],
        cwd: &Path,
        mk: &dyn Fn(String) -> Reject,
    ) -> Result<RunResult, Fail> {
        let t0 = Instant::now();
        let r = run_limited(
            cmd,
            cwd,
            &self.limits(None, self.c.max_seconds),
            &self.env(role),
        )?;
        self.layers.extend(r.layers.iter().cloned());
        let ok = r.success();
        self.ev.steps.push(Step {
            name: format!("{role}:{name}"),
            ok,
            seconds: t0.elapsed().as_secs_f64(),
        });
        if r.timed_out {
            return Err(
                Reject::Timeout(format!("{role}:{name} exceeded {}s", self.c.max_seconds)).into(),
            );
        }
        if !ok {
            return Err(mk(format!("{role}:{name}: {}", tail(&r))).into());
        }
        Ok(r)
    }

    fn validate(&self) -> Result<(), Fail> {
        let c = self.c;
        let bad = |m: String| Fail::Reject(Reject::MissingEvidence(m));
        if c.samples < crate::stats::MIN_SAMPLES {
            return Err(bad(format!(
                "contract.samples {} below minimum {}",
                c.samples,
                crate::stats::MIN_SAMPLES
            )));
        }
        if c.warmup < 1 {
            return Err(bad("contract.warmup must be >= 1".into()));
        }
        if !(c.min_improvement.is_finite() && c.min_improvement >= 0.0) {
            return Err(bad("contract.min_improvement invalid".into()));
        }
        let mut cmds = vec![&c.test_cmd, &c.holdout_cmd, &c.bench_cmd];
        cmds.extend(c.guard_benches.iter().map(|g| &g.bench_cmd));
        if cmds.iter().any(|v| v.is_empty()) {
            return Err(bad("contract has an empty command".into()));
        }
        for g in &c.guard_benches {
            if !(g.tolerance.is_finite() && (0.0..1.0).contains(&g.tolerance)) {
                return Err(bad(format!("guard {} tolerance invalid", g.name)));
            }
        }
        Ok(())
    }

    /// Everything the worker could influence must live outside what the
    /// evaluator trusts, and vice versa.
    fn integrity(&self, holdout: &Path) -> Result<(), Fail> {
        let (b, k) = (canon(&self.req.baseline)?, canon(&self.req.candidate)?);
        let mut trusted = vec![
            ("contract", canon(&self.req.contract)?),
            ("workdir", canon(&self.req.workdir)?),
        ];
        if holdout.exists() {
            trusted.push(("holdout", canon(holdout)?));
        }
        for (what, p) in &trusted {
            for (tree, root) in [("baseline", &b), ("candidate", &k)] {
                if p.starts_with(root) {
                    return Err(Reject::MissingEvidence(format!(
                        "{what} {} lies inside the {tree} tree; evaluator inputs must be outside proposer-controlled trees",
                        p.display()
                    ))
                    .into());
                }
            }
        }
        if b == k || b.starts_with(&k) || k.starts_with(&b) {
            return Err(
                Reject::MissingEvidence("baseline and candidate trees overlap".into()).into(),
            );
        }
        Ok(())
    }

    fn isolation(&mut self) -> Result<(), Fail> {
        let caps = Capabilities::probe();
        self.ev.isolation_unavailable = caps.notes.clone();
        let missing: Vec<&str> = [("netns", caps.netns), ("prlimit", caps.prlimit)]
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(n, _)| *n)
            .collect();
        if !missing.is_empty() && !self.req.allow_degraded {
            return Err(Reject::MissingEvidence(format!(
                "required isolation layers unavailable: {}",
                missing.join(", ")
            ))
            .into());
        }
        Ok(())
    }
}

impl Evaluator for RuneEvaluator {
    fn name(&self) -> &'static str {
        "rune-eval"
    }

    fn evaluate(&self, req: &EvalRequest) -> Result<FullReport, Error> {
        let bytes = fs::read(&req.contract).map_err(|e| Error::io(&req.contract, e))?;
        let contract: EvalContract = serde_json::from_slice(&bytes)?;
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| file_digest(&p).ok());
        let ev = Evidence {
            evaluator: format!(
                "rune-eval {} (Rune's own evaluator; not MetaHarness)",
                env!("CARGO_PKG_VERSION")
            ),
            evaluator_exe_sha256: exe.unwrap_or_else(|| "unavailable".into()),
            contract_file_sha256: hex_sha256(&bytes),
            holdout_tree_sha256: String::new(),
            rules: RULES.iter().map(|s| s.to_string()).collect(),
            isolation_unavailable: vec![],
            fork_mode: None,
            pinning: None,
            load_before: Some(sysinfo::load()),
            load_after: None,
            gates: None,
            steps: vec![],
            metrics: vec![],
            notes: vec![],
        };
        let (bd, cd) = (tree_digest(&req.baseline)?, tree_digest(&req.candidate)?);
        fs::create_dir_all(&req.workdir).map_err(|e| Error::io(&req.workdir, e))?;
        let cargo_home = match &req.cargo_home {
            Some(p) => p.clone(),
            None => {
                let p = req.workdir.join("cargo-home");
                fs::create_dir_all(&p).map_err(|e| Error::io(&p, e))?;
                p
            }
        };
        let mut run = Run {
            c: &contract,
            req,
            ev,
            layers: BTreeSet::new(),
            cargo_home,
            pin: None,
        };
        let digest = contract.digest();
        let outcome = match &req.expect_contract_digest {
            Some(want) if *want != digest => Err(Fail::Reject(Reject::MissingEvidence(format!(
                "contract digest mismatch: expected {want}, contract hashes to {digest}"
            )))),
            _ => flow(&mut run, &cd),
        };
        let verdict = match outcome {
            Ok(()) => Verdict::Accept,
            Err(Fail::Reject(r)) => Verdict::Reject(r),
            Err(Fail::Internal(e)) => return Err(e),
        };
        run.ev.load_after = Some(sysinfo::load());
        let p = run.ev.metrics.iter().find(|m| m.role == "improvement");
        let report = EvalReport {
            contract_digest: digest,
            baseline_digest: bd,
            candidate_digest: cd,
            samples: p.map_or(0, |m| m.pairs as u32),
            speedup_mean: p.map_or(0.0, |m| m.geo_mean_speedup),
            speedup_ci95_low: p.map_or(0.0, |m| m.ci95_low),
            speedup_ci95_high: p.map_or(0.0, |m| m.ci95_high),
            verdict,
            isolation: run.layers.iter().cloned().collect(),
        };
        Ok(FullReport {
            report,
            evidence: run.ev,
        })
    }
}

fn flow(r: &mut Run, cand_digest: &str) -> Result<(), Fail> {
    let c = r.c;
    r.validate()?;
    let contract_dir = r
        .req
        .contract
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let holdout_dir = contract_dir.join(&c.holdout_dir);
    r.integrity(&holdout_dir)?;
    if !holdout_dir.is_dir() {
        return Err(Reject::MissingEvidence(format!(
            "holdout dir missing: {}",
            holdout_dir.display()
        ))
        .into());
    }
    r.isolation()?;
    let holdout = read_tree(&holdout_dir)?;
    if holdout.files.is_empty() || !holdout.symlinks.is_empty() {
        return Err(
            Reject::MissingEvidence("holdout dir empty or contains symlinks".into()).into(),
        );
    }
    r.ev.holdout_tree_sha256 = tree_digest(&holdout_dir)?;
    if r.req.pin {
        r.ev.pinning = sysinfo::quietest_core();
        r.pin = r.ev.pinning.as_ref().map(|p| p.core);
    }
    if r.pin.is_none() {
        r.ev.notes
            .push("benchmarks were NOT pinned to a core".into());
    }
    if let Some(l) = &r.ev.load_before {
        if l.loadavg_1m > 0.25 * l.cpus as f64 {
            r.ev.notes.push(format!(
                "host load {:.2} on {} cpus before measuring",
                l.loadavg_1m, l.cpus
            ));
        }
    }

    let mut base = CowWorkspace::fork(&r.req.baseline, &r.req.workdir.join("baseline"))?;
    let mut cand = CowWorkspace::fork(&r.req.candidate, &r.req.workdir.join("candidate"))?;
    r.ev.fork_mode = Some(cand.mode);
    r.layers.insert(cand.layer_name().into());
    let (bdir, cdir) = (
        resolve_inside(&base.root, Path::new(&c.kernel_dir))?,
        resolve_inside(&cand.root, Path::new(&c.kernel_dir))?,
    );
    let build: Vec<String> = ["cargo", "build", "--release", "--offline"]
        .map(String::from)
        .to_vec();
    let missing = |m: String| Reject::MissingEvidence(format!("baseline precondition failed: {m}"));

    // Preconditions: the baseline builds, passes visible tests and its own holdout.
    r.step("build", "baseline", &build, &bdir, &missing)?;
    r.step("visible", "baseline", &c.test_cmd, &bdir, &missing)?;
    install_holdout(&mut base, c, &holdout.files)?;
    r.step("holdout", "baseline", &c.holdout_cmd, &bdir, &missing)?;

    // Static gates: nothing from the candidate has been executed yet.
    let tamper = harness_tampering(&r.req.baseline, &r.req.candidate, &c.kernel_dir)?;
    if !tamper.is_empty() {
        return Err(Reject::VisibleTestsFailed(tamper.join("; ")).into());
    }
    let report = gates::run_gates(&r.req.baseline, &r.req.candidate, c)?;
    let rejected = report.first_reject();
    r.ev.gates = Some(report);
    if let Some(rej) = rejected {
        return Err(rej.into());
    }

    r.step("build", "candidate", &build, &cdir, &Reject::BuildFailed)?;
    r.step(
        "visible",
        "candidate",
        &c.test_cmd,
        &cdir,
        &Reject::VisibleTestsFailed,
    )?;
    install_holdout(&mut cand, c, &holdout.files)?;
    r.step(
        "holdout",
        "candidate",
        &c.holdout_cmd,
        &cdir,
        &Reject::HoldoutFailed,
    )?;

    // Benchmarks: interleaved pairs, every metric measured before deciding.
    let metrics = bench::metrics_for(&c.bench_cmd, c.min_improvement, &c.guard_benches);
    for m in &metrics {
        let res = measure_metric(r, m, &bdir, &cdir)?;
        r.ev.metrics.push(res);
    }
    for m in &metrics {
        let res =
            r.ev.metrics
                .iter()
                .find(|x| x.name == m.name)
                .expect("measured");
        if !res.passed {
            let detail = bench::gate_detail(m, res);
            return Err(match m.kind {
                MetricKind::Improvement { .. } => Reject::InsufficientImprovement(detail),
                MetricKind::Guard { .. } => Reject::GuardRegression(detail),
            }
            .into());
        }
    }
    // The candidate tree must not have changed under us.
    if tree_digest(&r.req.candidate)? != cand_digest {
        return Err(
            Reject::MissingEvidence("candidate tree changed during evaluation".into()).into(),
        );
    }
    Ok(())
}

fn measure_metric(
    r: &mut Run,
    m: &Metric,
    bdir: &Path,
    cdir: &Path,
) -> Result<crate::report::MetricResult, Fail> {
    let wall = r.c.max_seconds.min(120);
    let limits = r.limits(r.pin, wall);
    let (benv, cenv) = (r.env("baseline"), r.env("candidate"));
    let mut layers = BTreeSet::new();
    let t0 = Instant::now();
    let res = bench::measure(m, r.c.warmup as usize, r.c.samples as usize, |is_cand| {
        let (dir, env, who) = if is_cand {
            (cdir, &cenv, "candidate")
        } else {
            (bdir, &benv, "baseline")
        };
        let out = run_limited(&m.cmd, dir, &limits, env).map_err(|e| {
            Reject::MissingEvidence(format!("{}: {who} bench could not run: {e}", m.name))
        })?;
        layers.extend(out.layers.iter().cloned());
        if out.timed_out {
            return Err(Reject::Timeout(format!(
                "{}: {who} bench exceeded {wall}s",
                m.name
            )));
        }
        if !out.success() {
            return Err(Reject::MissingEvidence(format!(
                "{}: {who} bench failed: {}",
                m.name,
                tail(&out)
            )));
        }
        bench::parse_ns(&out.stdout)
            .map_err(|e| Reject::MissingEvidence(format!("{}: {who}: {e}", m.name)))
    });
    r.layers.extend(layers);
    r.ev.steps.push(Step {
        name: format!("bench:{}", m.name),
        ok: res.is_ok(),
        seconds: t0.elapsed().as_secs_f64(),
    });
    Ok(res?)
}
