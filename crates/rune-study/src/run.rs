//! `verify` (generator correctness) and `run` (the real-evaluator sweep).

use crate::analyze::{load_rep, BORDER_HI, BORDER_LO};
use crate::gen::{io, write, Manifest, R};
use rune_core::contract::Verdict;
use std::collections::VecDeque;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub fn read_manifest(study: &Path) -> R<Manifest> {
    let p = study.join("manifest.json");
    serde_json::from_slice(&io(fs::read(&p), &p)?).map_err(|e| format!("{}: {e}", p.display()))
}

pub fn copy_tree(from: &Path, to: &Path) -> R<()> {
    io(fs::create_dir_all(to), to)?;
    for e in io(fs::read_dir(from), from)? {
        let e = io(e, from)?;
        let (s, d) = (e.path(), to.join(e.file_name()));
        if io(e.file_type(), &s)?.is_dir() {
            if e.file_name() != "target" {
                copy_tree(&s, &d)?;
            }
        } else {
            io(fs::copy(&s, &d), &s)?;
        }
    }
    Ok(())
}

fn pool<T: Send + 'static>(
    tasks: Vec<T>,
    workers: usize,
    f: impl Fn(usize, T) + Send + Sync + 'static,
) {
    let q = Arc::new(Mutex::new(tasks.into_iter().collect::<VecDeque<_>>()));
    let f = Arc::new(f);
    let hs: Vec<_> = (0..workers.max(1))
        .map(|w| {
            let (q, f) = (q.clone(), f.clone());
            std::thread::spawn(move || loop {
                let t = q.lock().unwrap().pop_front();
                match t {
                    Some(t) => f(w, t),
                    None => break,
                }
            })
        })
        .collect();
    for h in hs {
        let _ = h.join();
    }
}

/// Run each baseline and candidate through its own visible + holdout tests.
pub fn verify(study: &Path, cands: &Path, scratch: &Path, jobs: usize) -> R<()> {
    let m = read_manifest(study)?;
    let mut tasks = Vec::new();
    for w in &m.workloads {
        let wdir = study.join("workloads").join(&w.name);
        tasks.push((
            w.name.clone(),
            "baseline".to_string(),
            wdir.join("baseline"),
            wdir.join("holdout"),
        ));
        for c in &w.candidates {
            tasks.push((
                w.name.clone(),
                c.recipe.clone(),
                cands.join(&w.name).join(&c.recipe),
                wdir.join("holdout"),
            ));
        }
    }
    let n = tasks.len();
    let fails = Arc::new(Mutex::new(Vec::<String>::new()));
    let (fl, sc) = (fails.clone(), scratch.to_path_buf());
    pool(tasks, jobs, move |_, (w, r, src, hold)| {
        let dir = sc.join(&w).join(&r);
        let _ = fs::remove_dir_all(&dir);
        let res = (|| -> R<()> {
            copy_tree(&src, &dir)?;
            copy_tree(&hold, &dir.join("tests"))?;
            let out = Command::new("cargo")
                .args(["test", "--release", "--offline", "--quiet"])
                .current_dir(&dir)
                .env_remove("CARGO_TARGET_DIR")
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(())
            } else {
                let s = String::from_utf8_lossy(&out.stdout).to_string()
                    + &String::from_utf8_lossy(&out.stderr);
                let tail: String = s
                    .chars()
                    .rev()
                    .take(1500)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                Err(tail)
            }
        })();
        match res {
            Ok(()) => {
                let _ = fs::remove_dir_all(&dir);
            }
            Err(e) => fl.lock().unwrap().push(format!("{w}/{r}: {e}")),
        }
    });
    let f = fails.lock().unwrap();
    for x in f.iter() {
        eprintln!("FAIL {x}\n");
    }
    println!("verified {n} crates, {} failures", f.len());
    if f.is_empty() {
        Ok(())
    } else {
        Err("verification failures".into())
    }
}

pub struct SweepCfg {
    pub study: PathBuf,
    pub candidates: PathBuf,
    pub scratch: PathBuf,
    pub eval_bin: PathBuf,
    pub jobs: usize,
    /// Worker i is pinned to the SMT sibling pair (cores[i], cores[i] + 16).
    pub cores: Vec<usize>,
}

#[derive(Clone)]
struct Task {
    w: String,
    recipe: String,
    kind: &'static str,
    cand: PathBuf,
    out: PathBuf,
    digest: String,
}

fn loadavg() -> String {
    fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn run_tasks(cfg: &Arc<SweepCfg>, tasks: Vec<Task>, log: &Arc<Mutex<fs::File>>) {
    let (c, lg) = (cfg.clone(), log.clone());
    pool(tasks, cfg.jobs.min(cfg.cores.len()), move |wi, t| {
        let core = c.cores[wi];
        let pair = format!("{},{}", core, core + 16);
        let wdir = c.study.join("workloads").join(&t.w);
        let work = c
            .scratch
            .join("work")
            .join(&t.w)
            .join(format!("{}-{}", t.recipe, t.kind));
        let mut code = None;
        let mut err = String::new();
        let t0 = Instant::now();
        let load = loadavg();
        for attempt in 0..2 {
            let _ = fs::remove_dir_all(&work);
            let o = Command::new("taskset")
                .args(["-c", &pair])
                .arg(&c.eval_bin)
                .arg("--contract")
                .arg(wdir.join("contract.json"))
                .arg("--baseline")
                .arg(wdir.join("baseline"))
                .arg("--candidate")
                .arg(&t.cand)
                .arg("--workdir")
                .arg(&work)
                .arg("--out")
                .arg(&t.out)
                .args(["--expect-digest", &t.digest])
                .output();
            match o {
                Ok(o) => {
                    code = o.status.code();
                    err = String::from_utf8_lossy(&o.stderr).to_string();
                    if matches!(code, Some(0) | Some(1)) {
                        break;
                    }
                }
                Err(e) => err = e.to_string(),
            }
            err = format!("attempt {attempt}: {err}");
        }
        let _ = fs::remove_dir_all(&work);
        if !matches!(code, Some(0) | Some(1)) {
            let _ = write(&t.out.with_extension("error.txt"), &err);
        }
        let line = format!(
            "{{\"unix\":{},\"workload\":\"{}\",\"recipe\":\"{}\",\"kind\":\"{}\",\"cpus\":\"{}\",\"exit\":{},\"secs\":{:.1},\"host_loadavg_before\":\"{}\"}}\n",
            SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            t.w, t.recipe, t.kind, pair,
            code.map_or("null".to_string(), |x| x.to_string()),
            t0.elapsed().as_secs_f64(), load
        );
        let _ = lg.lock().unwrap().write_all(line.as_bytes());
    });
}

/// Run every applicable pair once, then re-run borderline Accepts once.
pub fn sweep(cfg: &SweepCfg) -> R<()> {
    let m = read_manifest(&cfg.study)?;
    let reports = cfg.study.join("reports");
    io(fs::create_dir_all(&reports), &reports)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(reports.join("run-log.jsonl"))
        .map_err(|e| e.to_string())?;
    let log = Arc::new(Mutex::new(log));
    let cfg = Arc::new(SweepCfg {
        study: cfg.study.clone(),
        candidates: cfg.candidates.clone(),
        scratch: cfg.scratch.clone(),
        eval_bin: cfg.eval_bin.clone(),
        jobs: cfg.jobs,
        cores: cfg.cores.clone(),
    });
    let mk = |w: &str, r: &str, kind: &'static str, digest: &str| Task {
        w: w.into(),
        recipe: r.into(),
        kind,
        cand: cfg.candidates.join(w).join(r),
        out: reports.join(w).join(if kind == "first" {
            format!("{r}.json")
        } else {
            format!("{r}.rerun.json")
        }),
        digest: digest.into(),
    };
    let _ = log.lock().unwrap().write_all(
        format!("{{\"event\":\"sweep-start\",\"host_loadavg\":\"{}\",\"nproc\":{},\"jobs\":{},\"cores\":{:?}}}\n",
            loadavg(), std::thread::available_parallelism().map_or(0, |n| n.get()), cfg.jobs, cfg.cores).as_bytes(),
    );
    let mut first = Vec::new();
    for w in &m.workloads {
        for c in &w.candidates {
            let t = mk(&w.name, &c.recipe, "first", &w.contract_struct_digest);
            if !t.out.exists() {
                io(fs::create_dir_all(t.out.parent().unwrap()), &t.out)?;
                first.push(t);
            }
        }
    }
    println!("phase 1: {} evaluations", first.len());
    run_tasks(&cfg, first, &log);
    let mut second = Vec::new();
    for w in &m.workloads {
        for c in &w.candidates {
            let t = mk(&w.name, &c.recipe, "rerun", &w.contract_struct_digest);
            let p1 = reports.join(&w.name).join(format!("{}.json", c.recipe));
            if let Ok(r) = load_rep(&p1) {
                if matches!(r.verdict, Verdict::Accept)
                    && (BORDER_LO..=BORDER_HI).contains(&r.speedup_ci95_low)
                    && !t.out.exists()
                {
                    second.push(t);
                }
            }
        }
    }
    println!("phase 2: {} borderline confirmation re-runs", second.len());
    run_tasks(&cfg, second, &log);
    let _ = log.lock().unwrap().write_all(
        format!(
            "{{\"event\":\"sweep-end\",\"host_loadavg\":\"{}\"}}\n",
            loadavg()
        )
        .as_bytes(),
    );
    Ok(())
}
