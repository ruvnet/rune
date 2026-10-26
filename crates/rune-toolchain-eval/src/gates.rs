//! Security gates (ADR 005): explicit, individually reported properties of the
//! baseline -> candidate diff. They are never folded into a score. A gate only
//! sees the two source trees; it executes nothing.

use crate::cargo_toml;
use crate::scan::{scan_rust, Kind};
use rune_core::contract::{EvalContract, Reject};
use rune_core::{hex_sha256, Error};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &["target", ".git", ".rune", "node_modules"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResult {
    pub gate: String,
    /// Whether the contract makes this gate rejecting.
    pub enforced: bool,
    pub passed: bool,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateReport {
    pub results: Vec<GateResult>,
}

impl GateReport {
    /// The first enforced, failed gate as its precise [`Reject`].
    pub fn first_reject(&self) -> Option<Reject> {
        let r = self.results.iter().find(|r| r.enforced && !r.passed)?;
        let detail = r.findings.join("; ");
        Some(match r.gate.as_str() {
            "unsafe" => Reject::NewUnsafe(detail),
            "dependencies" => Reject::NewDependency(detail),
            _ => Reject::NewCapability(detail),
        })
    }
}

#[derive(Debug, Default)]
pub struct TreeFiles {
    /// rel path -> file bytes (regular files only)
    pub files: BTreeMap<String, Vec<u8>>,
    pub symlinks: Vec<String>,
}

pub fn read_tree(root: &Path) -> Result<TreeFiles, Error> {
    fn go(dir: &Path, rel: &Path, out: &mut TreeFiles) -> Result<(), Error> {
        for e in fs::read_dir(dir).map_err(|e| Error::io(dir, e))? {
            let e = e.map_err(|e| Error::io(dir, e))?;
            let name = e.file_name();
            let rel_c: PathBuf = rel.join(&name);
            let ep = e.path();
            let ft = e.file_type().map_err(|x| Error::io(&ep, x))?;
            let rel_s = rel_c.to_string_lossy().replace('\\', "/");
            if ft.is_symlink() {
                out.symlinks.push(rel_s);
            } else if ft.is_dir() {
                if !SKIP_DIRS.iter().any(|s| name == *s) {
                    go(&e.path(), &rel_c, out)?;
                }
            } else if ft.is_file() {
                let p = e.path();
                out.files
                    .insert(rel_s, fs::read(&p).map_err(|e| Error::io(&p, e))?);
            }
        }
        Ok(())
    }
    let mut t = TreeFiles::default();
    go(root, Path::new(""), &mut t)?;
    t.symlinks.sort();
    Ok(t)
}

type Counts = BTreeMap<(String, Kind), Vec<(usize, String)>>;

fn scan_tree(t: &TreeFiles) -> Counts {
    let mut m: Counts = BTreeMap::new();
    for (rel, bytes) in &t.files {
        if rel.ends_with(".rs") {
            let text = String::from_utf8_lossy(bytes);
            for f in scan_rust(&text, Path::new(rel)) {
                m.entry((rel.clone(), f.kind))
                    .or_default()
                    .push((f.line, f.detail));
            }
        }
    }
    m
}

/// Findings in `cand` beyond what `base` already had in the same file+kind.
fn new_findings(base: &Counts, cand: &Counts, kinds: &[Kind]) -> Vec<String> {
    let mut out = Vec::new();
    for ((file, kind), items) in cand {
        if !kinds.contains(kind) {
            continue;
        }
        let had = base.get(&(file.clone(), *kind)).map_or(0, Vec::len);
        for (line, detail) in items.iter().skip(had) {
            out.push(format!("{file}:{line} new {} ({detail})", kind.label()));
        }
    }
    out
}

fn manifests(t: &TreeFiles) -> BTreeMap<String, cargo_toml::ManifestFacts> {
    t.files
        .iter()
        .filter(|(p, _)| p.rsplit('/').next() == Some("Cargo.toml"))
        .map(|(p, b)| (p.clone(), cargo_toml::parse(&String::from_utf8_lossy(b))))
        .collect()
}

fn dependency_changes(base: &TreeFiles, cand: &TreeFiles) -> Vec<String> {
    let (bm, cm) = (manifests(base), manifests(cand));
    let empty = BTreeSet::new();
    let mut out = Vec::new();
    for (file, facts) in &cm {
        let before = bm.get(file).map_or(&empty, |f| &f.deps);
        for d in facts.deps.difference(before) {
            out.push(format!("{file}: added/changed {d}"));
        }
    }
    for (file, facts) in &bm {
        let after = cm.get(file).map_or(&empty, |f| &f.deps);
        for d in facts.deps.difference(after) {
            out.push(format!("{file}: removed/changed {d}"));
        }
    }
    out
}

fn file_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Files that make a build execute attacker-chosen code or change the
/// toolchain, plus symlinks (which cannot be resolved safely).
fn build_surface(base: &TreeFiles, cand: &TreeFiles) -> Vec<String> {
    let mut out = Vec::new();
    for (p, bytes) in &cand.files {
        let name = file_name(p);
        let sensitive = name == "build.rs"
            || name == "rust-toolchain"
            || name == "rust-toolchain.toml"
            || p.starts_with(".cargo/")
            || p.contains("/.cargo/");
        if sensitive {
            let same = base
                .files
                .get(p)
                .is_some_and(|b| hex_sha256(b) == hex_sha256(bytes));
            if !same {
                out.push(format!("{p}: new or changed build-time executable surface"));
            }
        }
    }
    let (bm, cm) = (manifests(base), manifests(cand));
    for (file, f) in &cm {
        let b = bm.get(file).cloned().unwrap_or_default();
        if f.build_script && !b.build_script {
            out.push(format!("{file}: new `build =` script"));
        }
        if f.proc_macro && !b.proc_macro {
            out.push(format!("{file}: crate became a proc-macro"));
        }
    }
    for s in &cand.symlinks {
        out.push(format!("{s}: symlink in candidate tree"));
    }
    out
}

fn result(gate: &str, enforced: bool, findings: Vec<String>) -> GateResult {
    GateResult {
        gate: gate.into(),
        enforced,
        passed: findings.is_empty(),
        findings,
    }
}

/// Run every gate. All are computed and reported; the contract decides which
/// ones are enforcing.
pub fn run_gates(baseline: &Path, candidate: &Path, c: &EvalContract) -> Result<GateReport, Error> {
    let (b, k) = (read_tree(baseline)?, read_tree(candidate)?);
    let (bc, kc) = (scan_tree(&b), scan_tree(&k));
    Ok(GateReport {
        results: vec![
            result(
                "unsafe",
                c.forbid_unsafe,
                new_findings(&bc, &kc, &[Kind::Unsafe]),
            ),
            result(
                "dependencies",
                c.forbid_new_deps,
                dependency_changes(&b, &k),
            ),
            result(
                "build-surface",
                c.forbid_new_capabilities,
                build_surface(&b, &k),
            ),
            result(
                "capabilities",
                c.forbid_new_capabilities,
                new_findings(
                    &bc,
                    &kc,
                    &[
                        Kind::Net,
                        Kind::Process,
                        Kind::FsWrite,
                        Kind::Env,
                        Kind::ExternC,
                        Kind::IncludeOutside,
                    ],
                ),
            ),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> EvalContract {
        EvalContract {
            name: "t".into(),
            kernel_dir: ".".into(),
            test_cmd: vec![],
            holdout_dir: String::new(),
            holdout_dest: String::new(),
            holdout_cmd: vec![],
            bench_cmd: vec![],
            guard_benches: vec![],
            samples: 30,
            warmup: 1,
            min_improvement: 0.1,
            forbid_unsafe: true,
            forbid_new_deps: true,
            forbid_new_capabilities: true,
            max_seconds: 10,
            max_memory_mb: 1024,
        }
    }

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (p, c) in files {
            let path = d.path().join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, c).unwrap();
        }
        d
    }

    const MANIFEST: &str = "[package]\nname=\"k\"\n[dependencies]\n";

    fn verdict(base: &[(&str, &str)], cand: &[(&str, &str)]) -> Option<Reject> {
        let (b, c) = (tree(base), tree(cand));
        run_gates(b.path(), c.path(), &contract())
            .unwrap()
            .first_reject()
    }

    #[test]
    fn identical_trees_pass() {
        let f = [("Cargo.toml", MANIFEST), ("src/lib.rs", "pub fn f() {}")];
        assert_eq!(verdict(&f, &f), None);
    }

    #[test]
    fn new_unsafe_rejected_existing_unsafe_tolerated() {
        let base = [("Cargo.toml", MANIFEST), ("src/lib.rs", "pub fn f() {}")];
        let cand = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "pub fn f() { unsafe { } }"),
        ];
        assert!(matches!(verdict(&base, &cand), Some(Reject::NewUnsafe(_))));
        assert_eq!(verdict(&cand, &cand), None);
    }

    #[test]
    fn dependency_changes_rejected() {
        let base = [("Cargo.toml", MANIFEST), ("src/lib.rs", "")];
        let cand = [
            (
                "Cargo.toml",
                "[package]\nname=\"k\"\n[dependencies]\nhelper = { path = \"h\" }\n",
            ),
            ("src/lib.rs", ""),
        ];
        assert!(matches!(
            verdict(&base, &cand),
            Some(Reject::NewDependency(_))
        ));
        let build_dep = [
            (
                "Cargo.toml",
                "[package]\nname=\"k\"\n[build-dependencies]\ncc=\"1\"\n",
            ),
            ("src/lib.rs", ""),
        ];
        assert!(matches!(
            verdict(&base, &build_dep),
            Some(Reject::NewDependency(_))
        ));
    }

    #[test]
    fn build_surface_and_capabilities_rejected() {
        let base = [("Cargo.toml", MANIFEST), ("src/lib.rs", "")];
        for extra in [
            ("build.rs", "fn main(){}"),
            (".cargo/config.toml", "[build]\nrustc-wrapper=\"x\""),
            ("rust-toolchain.toml", "[toolchain]"),
        ] {
            let cand = [("Cargo.toml", MANIFEST), ("src/lib.rs", ""), extra];
            assert!(
                matches!(verdict(&base, &cand), Some(Reject::NewCapability(_))),
                "{extra:?}"
            );
        }
        let net = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "use std::net::TcpStream;"),
        ];
        assert!(matches!(
            verdict(&base, &net),
            Some(Reject::NewCapability(_))
        ));
        let pm = [
            (
                "Cargo.toml",
                "[package]\nname=\"k\"\n[dependencies]\n[lib]\nproc-macro = true\n",
            ),
            ("src/lib.rs", ""),
        ];
        assert!(matches!(
            verdict(&base, &pm),
            Some(Reject::NewCapability(_))
        ));
    }

    #[test]
    fn symlink_in_candidate_rejected() {
        let base = tree(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "")]);
        let cand = tree(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "")]);
        std::os::unix::fs::symlink("/etc/passwd", cand.path().join("src/x.rs")).unwrap();
        let r = run_gates(base.path(), cand.path(), &contract()).unwrap();
        assert!(matches!(r.first_reject(), Some(Reject::NewCapability(_))));
    }

    #[test]
    fn unenforced_gate_is_reported_not_rejecting() {
        let (b, c) = (
            tree(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "")]),
            tree(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "unsafe fn f() {}")]),
        );
        let mut k = contract();
        k.forbid_unsafe = false;
        let r = run_gates(b.path(), c.path(), &k).unwrap();
        assert_eq!(r.first_reject(), None);
        assert!(!r.results[0].passed);
    }
}
