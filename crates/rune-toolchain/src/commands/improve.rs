//! `rune improve`: a budgeted campaign. Reserve -> request -> settle/release.
//! Proposals are data; nothing is ever applied here.

use super::{Budget, Failure};
use crate::propose;
use rune_core::ledger::usd_to_micros;
use rune_core::{Ledger, Store};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) fn improve(
    ws: &Path,
    store: &Store,
    budget: &Budget,
    campaign: Option<&str>,
    reconcile: bool,
) -> Result<(), Failure> {
    let policy = budget.policy();
    policy.validate()?;
    if reconcile {
        let id = campaign_id(store, campaign)?;
        return do_reconcile(ws, &Ledger::open(ws, &id, &policy)?, &id);
    }
    // The key is checked before any ledger write or request.
    let key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    if key.is_empty() {
        return Err(Failure::MissingCredentials(
            "OPENROUTER_API_KEY is not set; no request was made".into(),
        ));
    }
    let id = campaign_id(store, campaign)?;
    let ledger = Ledger::open(ws, &id, &policy)?;
    let body = propose::request_body(budget.max_tokens);
    let (est_usd, est_tokens) = propose::estimate(&body);
    let note = format!("reserve for one proposal request (estimate ${est_usd:.6})");
    let reservation = ledger.reserve(est_usd, est_tokens, &note)?;

    let base = propose::base_url(std::env::var("RUNE_OPENROUTER_URL").ok().as_deref());
    let reply = match propose::call(&base, &key, &body, budget.max_seconds) {
        Ok(r) => r,
        Err(propose::CallFailure(msg)) => {
            ledger.release(reservation, &msg)?;
            return Err(Failure::Other(format!(
                "{msg}; reservation released, nothing was applied"
            )));
        }
    };
    let u = &reply.usage;
    let (cost, note) = match u.cost {
        Some(c) => (c, "provider-reported cost"),
        None => (0.0, "provider reported no cost"),
    };
    // Record the provider's figures first, then settle: a crash in between
    // shows up as a reconcile mismatch instead of vanishing.
    append_provider(
        ws,
        &id,
        &json!({
            "reservation": reservation, "id": reply.id, "model": reply.model,
            "prompt_tokens": u.prompt_tokens, "completion_tokens": u.completion_tokens,
            "total_tokens": u.total_tokens, "cost_usd": u.cost,
        }),
    )?;
    ledger.settle(reservation, cost, u.total_tokens, note)?;
    let candidate = reply.candidate.map_err(|_| {
        Failure::Other("provider or schema failure; usage settled, nothing was applied".into())
    })?;
    println!(
        "{}",
        json!({
            "status": "proposal_only", "campaign": id, "model": reply.model, "id": reply.id,
            "usage": {"prompt_tokens": u.prompt_tokens, "completion_tokens": u.completion_tokens,
                      "total_tokens": u.total_tokens, "cost_usd": u.cost},
            "cost_note": note, "candidate": candidate, "validated": false,
        })
    );
    Ok(())
}

fn campaign_id(store: &Store, given: Option<&str>) -> Result<String, Failure> {
    if let Some(c) = given {
        return Ok(c.to_string());
    }
    match store.current_id()? {
        Some(plan) => Ok(format!("plan-{}", &plan[..12])),
        None => Err(Failure::Other(
            "no approved plan to derive a campaign id; pass --campaign or run `rune build --freeze`".into(),
        )),
    }
}

fn provider_log(ws: &Path, campaign: &str) -> PathBuf {
    ws.join(".rune/improve").join(format!("{campaign}.jsonl"))
}

fn append_provider(ws: &Path, campaign: &str, rec: &Value) -> Result<(), Failure> {
    let path = provider_log(ws, campaign);
    let io = |e: std::io::Error| Failure::Other(format!("io error at {}: {e}", path.display()));
    fs::create_dir_all(path.parent().expect("has parent")).map_err(io)?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(io)?;
    writeln!(f, "{rec}").and_then(|_| f.sync_all()).map_err(io)
}

fn do_reconcile(ws: &Path, ledger: &Ledger, id: &str) -> Result<(), Failure> {
    let path = provider_log(ws, id);
    let text = fs::read_to_string(&path).unwrap_or_default();
    let (mut p_micros, mut p_tokens, mut calls, mut no_cost) = (0u64, 0u64, 0u64, 0u64);
    for line in text.lines().filter(|l| !l.is_empty()) {
        let v: Value = serde_json::from_str(line)
            .map_err(|_| Failure::Other(format!("corrupt provider log {}", path.display())))?;
        calls += 1;
        p_tokens += v["total_tokens"].as_u64().unwrap_or(0);
        match v["cost_usd"].as_f64() {
            Some(c) => p_micros += usd_to_micros(c),
            None => no_cost += 1,
        }
    }
    let t = ledger.totals()?;
    println!("campaign {id}");
    println!("  provider-reported  {p_micros} micro-USD, {p_tokens} tokens, {calls} calls ({no_cost} without cost)");
    println!(
        "  ledger spent       {} micro-USD, {} tokens",
        t.spent_usd_micros, t.spent_tokens
    );
    println!(
        "  ledger open        {} micro-USD reserved, {} overrun",
        t.open_reserved_usd_micros, t.overrun_usd_micros
    );
    match ledger.reconcile(p_micros as f64 / 1e6, p_tokens) {
        Ok(()) => {
            println!("  reconciled: ledger equals provider-reported totals");
            Ok(())
        }
        Err(e) => Err(Failure::Other(format!("reconcile FAILED: {e}"))),
    }
}
