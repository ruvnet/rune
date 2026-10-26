//! Executors and honest evidence classification (ADR 006).
//!
//! Evidence is decided by *how* an artifact ran, never by what the caller
//! hopes: `Native` only on matching hardware with every required feature,
//! `Emulated` under qemu-user or a foreign runtime, `Unvalidated` when no
//! executor can run it at all.

use crate::descriptors::{
    RUNTIME_NATIVE, RUNTIME_NODE_WASI, RUNTIME_QEMU_AARCH64, RVM_UNVALIDATED_REASON,
};
use crate::host::Host;
use crate::Error;
use rune_core::target::{Evidence, TargetDescriptor};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Performance claims are allowed for native execution only. Emulated,
/// compile-only and unvalidated evidence supports at most correctness claims.
pub fn perf_claim_allowed(evidence: Evidence) -> bool {
    evidence == Evidence::Native
}

pub trait Executor {
    fn name(&self) -> &str;
    /// Evidence class a successful run through this executor yields.
    fn evidence(&self) -> Evidence;
    /// `Err(reason)` when the executor cannot run at all (binary missing, ...).
    fn availability(&self) -> Result<(), String>;
    /// Triple this executor can run; `None` means "the host's own".
    fn supported_triple(&self) -> Option<&str>;
    /// ABI this executor provides; `None` means "the host's own".
    fn supported_abi(&self) -> Option<&str>;
    fn run(&self, artifact: &Path, args: &[String]) -> Result<Output, Error>;
}

/// Locate an executable by explicit path or on `PATH`.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

fn spawn(cmd: &mut Command, label: &str) -> Result<Output, Error> {
    cmd.stdin(Stdio::null())
        .output()
        .map_err(|e| Error::io(label, e))
}

#[derive(Debug, Clone, Default)]
pub struct NativeExecutor;

impl Executor for NativeExecutor {
    fn name(&self) -> &str {
        RUNTIME_NATIVE
    }
    fn evidence(&self) -> Evidence {
        Evidence::Native
    }
    fn availability(&self) -> Result<(), String> {
        Ok(())
    }
    fn supported_triple(&self) -> Option<&str> {
        None
    }
    fn supported_abi(&self) -> Option<&str> {
        None
    }
    fn run(&self, artifact: &Path, args: &[String]) -> Result<Output, Error> {
        spawn(
            Command::new(artifact).args(args),
            &artifact.to_string_lossy(),
        )
    }
}

/// aarch64 under qemu-user: correctness evidence only.
#[derive(Debug, Clone)]
pub struct QemuUserExecutor {
    pub qemu: String,
    pub sysroot: Option<String>,
}

impl QemuUserExecutor {
    pub fn new(qemu: impl Into<String>, sysroot: Option<String>) -> Self {
        Self {
            qemu: qemu.into(),
            sysroot,
        }
    }
    pub fn for_target(t: &TargetDescriptor) -> Self {
        Self::new(RUNTIME_QEMU_AARCH64, t.sysroot.clone())
    }
}

impl Executor for QemuUserExecutor {
    fn name(&self) -> &str {
        RUNTIME_QEMU_AARCH64
    }
    fn evidence(&self) -> Evidence {
        Evidence::Emulated
    }
    fn availability(&self) -> Result<(), String> {
        find_executable(&self.qemu)
            .map(|_| ())
            .ok_or_else(|| format!("executor `{}` not found", self.qemu))
    }
    fn supported_triple(&self) -> Option<&str> {
        Some(crate::descriptors::AARCH64_LINUX)
    }
    fn supported_abi(&self) -> Option<&str> {
        Some("aapcs64")
    }
    fn run(&self, artifact: &Path, args: &[String]) -> Result<Output, Error> {
        let qemu = find_executable(&self.qemu).ok_or_else(|| {
            Error::io(
                &self.qemu,
                std::io::Error::new(std::io::ErrorKind::NotFound, "qemu not found"),
            )
        })?;
        let mut cmd = Command::new(qemu);
        if let Some(root) = &self.sysroot {
            cmd.arg("-L").arg(root);
        }
        spawn(cmd.arg(artifact).args(args), &self.qemu)
    }
}

/// wasm32-wasip1 under Node's `node:wasi`: a foreign runtime, so `Emulated`.
#[derive(Debug, Clone)]
pub struct NodeWasiExecutor {
    pub node: String,
}

impl NodeWasiExecutor {
    pub fn new(node: impl Into<String>) -> Self {
        Self { node: node.into() }
    }
}

impl Default for NodeWasiExecutor {
    fn default() -> Self {
        Self::new("node")
    }
}

/// Minimal WASI host (preview1). Exit code propagates; no preopened dirs, no
/// environment: the module is given no ambient authority.
const WASI_RUNNER: &str = r#"
const { WASI } = require('node:wasi');
const fs = require('node:fs');
(async () => {
  const file = process.argv[1];
  const wasi = new WASI({ version: 'preview1', args: [file, ...process.argv.slice(2)], env: {}, preopens: {} });
  const mod = await WebAssembly.compile(fs.readFileSync(file));
  const inst = await WebAssembly.instantiate(mod, wasi.getImportObject());
  const code = wasi.start(inst);
  process.exitCode = typeof code === 'number' ? code : 0;
})().catch((e) => { console.error(String(e)); process.exit(70); });
"#;

impl Executor for NodeWasiExecutor {
    fn name(&self) -> &str {
        RUNTIME_NODE_WASI
    }
    fn evidence(&self) -> Evidence {
        Evidence::Emulated
    }
    fn availability(&self) -> Result<(), String> {
        find_executable(&self.node)
            .map(|_| ())
            .ok_or_else(|| format!("executor `{}` not found", self.node))
    }
    fn supported_triple(&self) -> Option<&str> {
        Some(crate::descriptors::WASM32_WASI)
    }
    fn supported_abi(&self) -> Option<&str> {
        Some("wasi-preview1")
    }
    fn run(&self, artifact: &Path, args: &[String]) -> Result<Output, Error> {
        let node = find_executable(&self.node).ok_or_else(|| {
            Error::io(
                &self.node,
                std::io::Error::new(std::io::ErrorKind::NotFound, "node not found"),
            )
        })?;
        spawn(
            Command::new(node)
                .args(["--no-warnings", "-e", WASI_RUNNER])
                .arg(artifact)
                .args(args),
            &self.node,
        )
    }
}

/// Explicitly cannot run anything (e.g. RVM): always unvalidated.
#[derive(Debug, Clone)]
pub struct UnavailableExecutor {
    pub reason: String,
}

impl UnavailableExecutor {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl Executor for UnavailableExecutor {
    fn name(&self) -> &str {
        "unavailable"
    }
    fn evidence(&self) -> Evidence {
        Evidence::Unvalidated
    }
    fn availability(&self) -> Result<(), String> {
        Err(self.reason.clone())
    }
    fn supported_triple(&self) -> Option<&str> {
        Some("")
    }
    fn supported_abi(&self) -> Option<&str> {
        Some("")
    }
    fn run(&self, _: &Path, _: &[String]) -> Result<Output, Error> {
        Err(Error::Verify(self.reason.clone()))
    }
}

/// Executor named by a descriptor's `runtime` field.
pub fn executor_for(t: &TargetDescriptor) -> Box<dyn Executor> {
    match t.runtime.as_str() {
        RUNTIME_NATIVE => Box::new(NativeExecutor),
        RUNTIME_QEMU_AARCH64 => Box::new(QemuUserExecutor::for_target(t)),
        RUNTIME_NODE_WASI => Box::new(NodeWasiExecutor::default()),
        _ if t.triple == crate::descriptors::RVM => {
            Box::new(UnavailableExecutor::new(RVM_UNVALIDATED_REASON))
        }
        other => Box::new(UnavailableExecutor::new(format!(
            "no executor for runtime `{other}`"
        ))),
    }
}

#[derive(Debug)]
pub struct Outcome {
    /// `None` when nothing was executed (unvalidated).
    pub output: Option<Output>,
    pub evidence: Evidence,
    /// Why the run was not performed or not native.
    pub note: Option<String>,
}

fn unvalidated(note: String) -> Outcome {
    Outcome {
        output: None,
        evidence: Evidence::Unvalidated,
        note: Some(note),
    }
}

/// Run `artifact` through the executor the descriptor names and classify.
pub fn run_and_classify(
    target: &TargetDescriptor,
    host: &Host,
    artifact: &Path,
    args: &[String],
) -> Result<Outcome, Error> {
    run_and_classify_with(target, host, executor_for(target).as_ref(), artifact, args)
}

pub fn run_and_classify_with(
    target: &TargetDescriptor,
    host: &Host,
    exec: &dyn Executor,
    artifact: &Path,
    args: &[String],
) -> Result<Outcome, Error> {
    if let Err(reason) = exec.availability() {
        return Ok(unvalidated(reason));
    }
    if let Some(t) = exec.supported_triple() {
        if t != target.triple {
            return Ok(unvalidated(format!(
                "executor `{}` cannot run {}",
                exec.name(),
                target.triple
            )));
        }
    }
    if exec.evidence() == Evidence::Native {
        if target.triple != host.triple {
            return Ok(unvalidated(format!(
                "target {} is not the host {}",
                target.triple, host.triple
            )));
        }
        let missing: Vec<&str> = target
            .cpu_features
            .iter()
            .filter(|f| !host.features.contains(*f))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            return Ok(unvalidated(format!(
                "host lacks required cpu features: {}",
                missing.join(", ")
            )));
        }
    }
    let output = exec.run(artifact, args)?;
    Ok(Outcome {
        output: Some(output),
        evidence: exec.evidence(),
        note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptors::*;

    #[test]
    fn only_native_may_claim_performance() {
        assert!(perf_claim_allowed(Evidence::Native));
        for e in [
            Evidence::Emulated,
            Evidence::CompileOnly,
            Evidence::Unvalidated,
        ] {
            assert!(!perf_claim_allowed(e));
        }
    }

    #[test]
    fn missing_executor_is_unvalidated() {
        let host = Host::detect();
        let t = aarch64_linux_gnu(&[]);
        let q = QemuUserExecutor::new("/nonexistent/qemu-aarch64-static", None);
        let o = run_and_classify_with(&t, &host, &q, Path::new("/x"), &[]).unwrap();
        assert_eq!(o.evidence, Evidence::Unvalidated);
        assert!(o.output.is_none());
        let n = NodeWasiExecutor::new("/nonexistent/node");
        let o = run_and_classify_with(&wasm32_wasip1(), &host, &n, Path::new("/x"), &[]).unwrap();
        assert_eq!(o.evidence, Evidence::Unvalidated);
    }

    #[test]
    fn rvm_is_unvalidated_specification_required() {
        let o = run_and_classify(&rvm(), &Host::detect(), Path::new("/x"), &[]).unwrap();
        assert_eq!(o.evidence, Evidence::Unvalidated);
        assert_eq!(o.note.as_deref(), Some("specification required"));
    }

    #[test]
    fn native_requires_matching_triple_and_features() {
        let host = Host::detect();
        let mut other = x86_64_linux_gnu(&[]);
        other.triple = "riscv64gc-unknown-linux-gnu".into();
        let o =
            run_and_classify_with(&other, &host, &NativeExecutor, Path::new("/x"), &[]).unwrap();
        assert_eq!(o.evidence, Evidence::Unvalidated);
        let mut need = x86_64_linux_gnu(&["no_such_cpu_feature"]);
        need.triple = host.triple.clone();
        let o = run_and_classify_with(&need, &host, &NativeExecutor, Path::new("/x"), &[]).unwrap();
        assert_eq!(o.evidence, Evidence::Unvalidated);
        assert!(o.note.unwrap().contains("no_such_cpu_feature"));
    }
}
