use crate::fsio::{load, reject, write_atomic};
use rune_core::edit::{EditApplier, EditOutcome, EditRequest};
use rune_core::{hex_sha256, Error};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SCRIPT: &str = include_str!("sidecar.cjs");

/// Applies `req.find` as the Agent Booster edit snippet (`req.replace` is unused)
/// via `new AgentBooster().apply({code, edit, language})`, taking `result.output`.
/// The package is loaded from `node_modules` (pin: agent-booster 0.2.2).
#[derive(Debug, Clone)]
pub struct AgentBoosterSidecar {
    node: PathBuf,
    node_modules: PathBuf,
    timeout: Duration,
}

impl AgentBoosterSidecar {
    pub fn new(node_modules: impl Into<PathBuf>) -> Self {
        Self {
            node: "node".into(),
            node_modules: node_modules.into(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_node(mut self, node: impl Into<PathBuf>) -> Self {
        self.node = node.into();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Run the script; returns raw stdout, capped at `cap` bytes (hitting the cap is an error).
    fn run(&self, payload: Vec<u8>, cap: usize) -> Result<Vec<u8>, String> {
        let mut child = Command::new(&self.node)
            .arg("-e")
            .arg(SCRIPT)
            .current_dir(std::env::temp_dir())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("RUNE_NODE_MODULES", &self.node_modules)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "could not start node".to_string())?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let writer = thread::spawn(move || {
            let _ = stdin.write_all(&payload);
        });
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let reader = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.take(cap as u64 + 1).read_to_end(&mut buf);
            buf
        });
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break Ok(s),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err("sidecar timed out".to_string());
                }
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(_) => break Err("could not wait for sidecar".to_string()),
            }
        };
        let _ = writer.join();
        let out = reader.join().unwrap_or_default();
        let status = status?;
        if !status.success() {
            return Err("sidecar exited with failure".into());
        }
        if out.len() > cap {
            return Err("sidecar output over the size bound".into());
        }
        Ok(out)
    }
}

fn fail(why: &str) -> Error {
    reject(format!("agent-booster sidecar rejected the edit: {why}"))
}

impl EditApplier for AgentBoosterSidecar {
    fn name(&self) -> &'static str {
        "agent_booster_sidecar"
    }

    fn apply(&self, req: &EditRequest) -> Result<EditOutcome, Error> {
        // Path, size and stale-hash checks all happen here, before any process is spawned.
        let file = load(req)?;
        if req.find.is_empty() {
            return Err(reject("edit snippet is empty"));
        }
        let lang_ok = !req.language.is_empty()
            && req.language.len() <= 16
            && req.language.bytes().all(|b| b.is_ascii_lowercase());
        if !lang_ok {
            return Err(reject("language must be 1-16 lowercase ASCII letters"));
        }
        let payload = json!({"code": file.text, "edit": req.find, "language": req.language});
        let payload = serde_json::to_vec(&payload).map_err(Error::from)?;
        // JSON escaping can expand text up to 6x; add slack for the envelope.
        let cap = (req.max_bytes as usize).saturating_mul(6).saturating_add(4096);
        let raw = self.run(payload, cap).map_err(|e| fail(&e))?;
        let v: Value = serde_json::from_slice(&raw).map_err(|_| fail("malformed result"))?;
        if v["success"] != Value::Bool(true) {
            return Err(fail("package reported success=false"));
        }
        let output = match v["output"].as_str() {
            Some(s) if !s.is_empty() => s,
            _ => return Err(fail("empty or missing output")),
        };
        if output.len() as u64 > req.max_bytes {
            return Err(fail("output over the size bound"));
        }
        write_atomic(&file.path, &file.sha256, output.as_bytes())?;
        Ok(EditOutcome {
            applier: self.name().into(),
            before_sha256: file.sha256,
            after_sha256: hex_sha256(output.as_bytes()),
            confidence: v["confidence"].as_f64(),
        })
    }
}
