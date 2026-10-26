//! `rune improve` campaign tests against a local mock provider (no network).

use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

const KEY: &str = "test-key-not-real";

struct Mock {
    url: String,
    requests: Arc<Mutex<Vec<(String, Value)>>>, // (authorization, body)
}

/// Serves each canned (status, body) once, in order, then stops accepting.
fn mock(replies: Vec<(u16, String)>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/api/v1",
        listener.local_addr().unwrap().port()
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    thread::spawn(move || {
        for (status, body) in replies {
            let Ok((mut s, _)) = listener.accept() else {
                return;
            };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let (head_end, len) = loop {
                let n = s.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map(|v| v.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    break (p + 4, len);
                }
                assert!(n > 0, "client closed early");
            };
            while buf.len() < head_end + len {
                let n = s.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
            }
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let auth = head
                .lines()
                .find(|l| l.to_lowercase().starts_with("authorization:"))
                .unwrap_or("")
                .to_string();
            let req: Value = serde_json::from_slice(&buf[head_end..head_end + len]).unwrap();
            log.lock().unwrap().push((auth, req));
            let resp = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(resp.as_bytes()).unwrap();
        }
    });
    Mock { url, requests }
}

fn ok_reply(usage: Value) -> (u16, String) {
    let content = json!({"proposal": "use chunks_exact", "risks": "fp reassociation"}).to_string();
    (
        200,
        json!({"id": "gen-1", "model": "moonshotai/kimi-k3.1-pro",
               "choices": [{"message": {"content": content}}], "usage": usage})
        .to_string(),
    )
}

fn fixture() -> tempfile::TempDir {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/hello");
    let dir = tempfile::tempdir().unwrap();
    copy(&src, dir.path());
    dir
}

fn copy(from: &Path, to: &Path) {
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            fs::create_dir_all(&dest).unwrap();
            copy(&e.path(), &dest);
        } else {
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn rune(ws: &Path, url: Option<&str>, key: Option<&str>, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_rune"));
    c.arg("-C")
        .arg(ws)
        .args(args)
        .env_remove("OPENROUTER_API_KEY")
        .env_remove("RUNE_OPENROUTER_URL");
    if let Some(u) = url {
        c.env("RUNE_OPENROUTER_URL", u);
    }
    if let Some(k) = key {
        c.env("OPENROUTER_API_KEY", k);
    }
    c.output().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn ledger(ws: &Path, campaign: &str) -> Vec<Value> {
    fs::read_to_string(ws.join(format!(".rune/ledger/{campaign}.jsonl")))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn reserve_settle_reconcile_with_provider_cost() {
    let d = fixture();
    let m = mock(vec![ok_reply(
        json!({"prompt_tokens": 120, "completion_tokens": 80, "total_tokens": 200, "cost": 0.00123}),
    )]);
    let o = rune(
        d.path(),
        Some(&m.url),
        Some(KEY),
        &["improve", "--campaign", "c1", "--max-usd", "0.05"],
    );
    assert!(o.status.success(), "{}", err(&o));
    let report: Value = serde_json::from_str(out(&o).trim()).unwrap();
    assert_eq!(report["status"], "proposal_only");
    assert_eq!(report["validated"], false);
    assert_eq!(report["usage"]["cost_usd"], 0.00123);

    let reqs = m.requests.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(
        reqs[0].0.to_lowercase(),
        format!("authorization: bearer {KEY}")
    );
    let body = &reqs[0].1;
    assert_eq!(body["usage"]["include"], true);
    assert_eq!(body["provider"]["data_collection"], "deny");
    assert_eq!(body["provider"]["zdr"], true);
    assert_eq!(body["provider"]["allow_fallbacks"], false);

    let l = ledger(d.path(), "c1");
    assert_eq!(l.len(), 2);
    assert_eq!(l[0]["kind"], "reserve", "reservation precedes the spend");
    assert_eq!(l[1]["kind"], "spend");
    assert_eq!(l[1]["reservation"], l[0]["seq"]);
    assert_eq!(l[1]["usd_micros"], 1230);
    assert_eq!(l[1]["tokens"], 200);

    let r = rune(
        d.path(),
        None,
        None,
        &["improve", "--reconcile", "--campaign", "c1"],
    );
    assert!(r.status.success(), "{}{}", out(&r), err(&r));
    let s = out(&r);
    assert!(
        s.contains("provider-reported  1230 micro-USD, 200 tokens"),
        "{s}"
    );
    assert!(
        s.contains("ledger spent       1230 micro-USD, 200 tokens"),
        "{s}"
    );
    assert!(s.contains("reconciled"), "{s}");
}

#[test]
fn missing_cost_is_recorded_as_zero_never_invented() {
    let d = fixture();
    let m = mock(vec![ok_reply(
        json!({"prompt_tokens": 50, "completion_tokens": 25}),
    )]);
    let o = rune(
        d.path(),
        Some(&m.url),
        Some(KEY),
        &["improve", "--campaign", "nc"],
    );
    assert!(o.status.success(), "{}", err(&o));
    let l = ledger(d.path(), "nc");
    assert_eq!(l[1]["usd_micros"], 0);
    assert_eq!(l[1]["tokens"], 75);
    assert_eq!(l[1]["note"], "provider reported no cost");
    assert!(out(&o).contains("provider reported no cost"));
    let r = rune(
        d.path(),
        None,
        None,
        &["improve", "--reconcile", "--campaign", "nc"],
    );
    assert!(r.status.success(), "{}", err(&r));
    assert!(out(&r).contains("1 without cost"));
}

#[test]
fn provider_error_releases_reservation() {
    let d = fixture();
    let body =
        json!({"error": {"message": format!("no endpoint satisfies zdr for {KEY}")}}).to_string();
    let m = mock(vec![(404, body)]);
    let o = rune(
        d.path(),
        Some(&m.url),
        Some(KEY),
        &["improve", "--campaign", "e1"],
    );
    assert_eq!(o.status.code(), Some(1));
    let e = err(&o);
    assert!(e.contains("provider HTTP 404") && e.contains("zdr"), "{e}");
    assert!(
        !e.contains(KEY) && !out(&o).contains(KEY),
        "key must never be printed"
    );
    let l = ledger(d.path(), "e1");
    assert_eq!(l.len(), 2);
    assert_eq!(l[0]["kind"], "reserve");
    assert_eq!(l[1]["kind"], "release");
    assert_eq!(l[1]["reservation"], l[0]["seq"]);
    assert!(!l[1]["note"].as_str().unwrap().contains(KEY));
    // Nothing spent, nothing open: reconcile is clean.
    let r = rune(
        d.path(),
        None,
        None,
        &["improve", "--reconcile", "--campaign", "e1"],
    );
    assert!(r.status.success(), "{}", err(&r));
}

#[test]
fn schema_failure_still_settles_the_charge() {
    let d = fixture();
    let bad = (
        200,
        json!({"id": "g", "model": "m", "choices": [{"message": {"content": "not json"}}],
               "usage": {"total_tokens": 10, "cost": 0.0001}})
        .to_string(),
    );
    let m = mock(vec![bad]);
    let o = rune(
        d.path(),
        Some(&m.url),
        Some(KEY),
        &["improve", "--campaign", "s1"],
    );
    assert_eq!(o.status.code(), Some(1));
    let l = ledger(d.path(), "s1");
    assert_eq!(l[1]["kind"], "spend");
    assert_eq!(l[1]["usd_micros"], 100);
    assert!(rune(
        d.path(),
        None,
        None,
        &["improve", "--reconcile", "--campaign", "s1"]
    )
    .status
    .success());
}

#[test]
fn budget_exhaustion_exits_1_with_no_request() {
    let d = fixture();
    let m = mock(vec![ok_reply(json!({"total_tokens": 1, "cost": 0.0}))]);
    for extra in [["--max-usd", "0.000001"], ["--max-tokens", "50"]] {
        let mut args = vec!["improve", "--campaign", "b1"];
        args.extend(extra);
        let o = rune(d.path(), Some(&m.url), Some(KEY), &args);
        assert_eq!(o.status.code(), Some(1), "{}", err(&o));
        assert!(err(&o).contains("budget exhausted"), "{}", err(&o));
    }
    assert!(
        m.requests.lock().unwrap().is_empty(),
        "no request may be made"
    );
    assert!(
        ledger(d.path(), "b1").is_empty(),
        "failed reservation writes nothing"
    );
}

#[test]
fn second_request_fails_when_first_exhausts_the_budget() {
    let d = fixture();
    let m = mock(vec![ok_reply(json!({"total_tokens": 3000, "cost": 0.02}))]);
    // 0.03 USD fits one reservation (~0.01) but not spent 0.02 + a new ~0.01+.
    let args = ["improve", "--campaign", "b2", "--max-usd", "0.03"];
    assert!(rune(d.path(), Some(&m.url), Some(KEY), &args)
        .status
        .success());
    let o = rune(d.path(), Some(&m.url), Some(KEY), &args);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("budget exhausted"), "{}", err(&o));
    assert_eq!(m.requests.lock().unwrap().len(), 1);
}

#[test]
fn missing_key_exits_2_before_any_ledger_write() {
    let d = fixture();
    let m = mock(vec![]);
    let o = rune(
        d.path(),
        Some(&m.url),
        None,
        &["improve", "--campaign", "k1"],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(!d.path().join(".rune").exists());
    assert!(m.requests.lock().unwrap().is_empty());
}

#[test]
fn default_campaign_is_derived_from_plan_id() {
    let d = fixture();
    let o = rune(d.path(), None, Some(KEY), &["improve"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("--campaign"), "{}", err(&o));
    assert!(!d.path().join(".rune/ledger").exists());

    assert!(rune(d.path(), None, None, &["build", "--freeze"])
        .status
        .success());
    let m = mock(vec![ok_reply(json!({"total_tokens": 5, "cost": 0.00001}))]);
    let o = rune(d.path(), Some(&m.url), Some(KEY), &["improve"]);
    assert!(o.status.success(), "{}", err(&o));
    let report: Value = serde_json::from_str(out(&o).trim()).unwrap();
    let id = report["campaign"].as_str().unwrap();
    assert!(id.starts_with("plan-") && id.len() == 17, "{id}");
    assert_eq!(ledger(d.path(), id).len(), 2);
}

#[test]
fn reconcile_detects_unsettled_reservation() {
    let d = fixture();
    // Simulate a crash after reserve: write only the reserve entry.
    fs::create_dir_all(d.path().join(".rune/ledger")).unwrap();
    fs::write(
        d.path().join(".rune/ledger/x1.jsonl"),
        "{\"seq\":1,\"campaign\":\"x1\",\"kind\":\"reserve\",\"usd_micros\":9000,\"tokens\":900,\"reservation\":null,\"note\":\"r\"}\n",
    )
    .unwrap();
    let r = rune(
        d.path(),
        None,
        None,
        &["improve", "--reconcile", "--campaign", "x1"],
    );
    assert_eq!(r.status.code(), Some(1));
    assert!(err(&r).contains("reconcile FAILED"), "{}", err(&r));
}
