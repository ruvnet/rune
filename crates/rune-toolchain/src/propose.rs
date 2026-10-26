//! One bounded, synthetic OpenRouter proposal request. Returned text is data:
//! it is validated against a fixed schema, printed, and never applied.
//! Usage accounting is requested so the response carries the provider's cost.

use serde_json::{json, Value};
use std::time::Duration;

const MODEL: &str = "moonshotai/kimi-k3.1-pro";
const DEFAULT_BASE: &str = "https://openrouter.ai/api/v1";
const MAX_FIELD: usize = 20_000;
/// ADR 002 catalog prices (USD per token) used ONLY to size a reservation,
/// never recorded as spend. Spend always comes from the provider.
const RESERVE_USD_PER_IN_TOKEN: f64 = 2.0 / 1e6;
const RESERVE_USD_PER_OUT_TOKEN: f64 = 10.0 / 1e6;

/// `RUNE_OPENROUTER_URL` is honored only for a loopback http URL (test mocks).
pub fn base_url(override_url: Option<&str>) -> String {
    match override_url {
        Some(u) if u.starts_with("http://127.0.0.1") => u.trim_end_matches('/').to_string(),
        _ => DEFAULT_BASE.to_string(),
    }
}

/// No `temperature`: the only endpoint (Moonshot AI) does not list it, and
/// `require_parameters` (kept) makes unsupported parameters a 404.
pub fn request_body(max_tokens: u64) -> Value {
    json!({
        "model": MODEL,
        "max_tokens": max_tokens.min(2000),
        "reasoning": {"effort": "low"},
        "usage": {"include": true},
        "provider": {"data_collection": "deny", "zdr": true,
                     "require_parameters": true, "allow_fallbacks": false},
        "messages": [
            {"role": "system", "content": "Return a JSON object with exactly keys proposal and risks. Both values must be strings. Suggest one Rust build optimization. Do not output shell commands."},
            {"role": "user", "content": "Synthetic fixture: a Rust library computes squared Euclidean distances over f32 arrays. Propose a measurable optimization and its semantic risks. This is discovery only."}
        ]
    })
}

/// Conservative upper bound (USD, tokens) for one request. Every input token
/// costs at most one per request byte; output is capped by `max_tokens`.
pub fn estimate(body: &Value) -> (f64, u64) {
    let in_tokens = body.to_string().len() as u64;
    let out_tokens = body["max_tokens"].as_u64().unwrap_or(2000);
    let usd =
        in_tokens as f64 * RESERVE_USD_PER_IN_TOKEN + out_tokens as f64 * RESERVE_USD_PER_OUT_TOKEN;
    (usd, in_tokens + out_tokens)
}

/// Accept only `{proposal: string, risks: string}` with bounded lengths.
pub fn validate(content: &str) -> Result<Value, ()> {
    let v: Value = serde_json::from_str(content).map_err(|_| ())?;
    let obj = v.as_object().ok_or(())?;
    if obj.len() != 2 {
        return Err(());
    }
    for k in ["proposal", "risks"] {
        match obj.get(k).and_then(Value::as_str) {
            Some(s) if s.len() <= MAX_FIELD => {}
            _ => return Err(()),
        }
    }
    Ok(v)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// Provider-reported USD; `None` when the provider returned no cost.
    pub cost: Option<f64>,
}

pub fn parse_usage(resp: &Value) -> Usage {
    let u = &resp["usage"];
    let prompt = u["prompt_tokens"].as_u64().unwrap_or(0);
    let completion = u["completion_tokens"].as_u64().unwrap_or(0);
    Usage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: u["total_tokens"].as_u64().unwrap_or(prompt + completion),
        cost: u["cost"].as_f64().filter(|c| c.is_finite() && *c >= 0.0),
    }
}

pub struct Reply {
    pub id: String,
    pub model: String,
    pub finish_reason: String,
    pub usage: Usage,
    pub candidate: Result<Value, ()>,
}

/// The call failed with no usable accounting; the reservation is released.
/// The message is sanitized and never contains the key or request body.
pub struct CallFailure(pub String);

fn sanitize(s: &str, key: &str) -> String {
    let s = if key.is_empty() {
        s.to_string()
    } else {
        s.replace(key, "[redacted]")
    };
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(300)
        .collect()
}

pub fn call(base: &str, key: &str, body: &Value, timeout_s: u64) -> Result<Reply, CallFailure> {
    let url = format!("{base}/chat/completions");
    let resp = ureq::post(&url)
        .timeout(Duration::from_secs(timeout_s.clamp(1, 60)))
        .set("Authorization", &format!("Bearer {key}"))
        .send_json(body.clone());
    let resp: Value = match resp {
        Ok(r) => r
            .into_json()
            .map_err(|_| CallFailure("unparseable provider response; charge unknown".into()))?,
        Err(ureq::Error::Status(code, r)) => {
            let detail = r
                .into_json::<Value>()
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(|m| sanitize(m, key)))
                .unwrap_or_default();
            return Err(CallFailure(format!("provider HTTP {code}: {detail}")));
        }
        Err(ureq::Error::Transport(t)) => {
            return Err(CallFailure(format!(
                "transport error: {}",
                sanitize(&t.kind().to_string(), key)
            )))
        }
    };
    let candidate = resp["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(())
        .and_then(validate);
    Ok(Reply {
        id: resp["id"].as_str().unwrap_or_default().to_string(),
        model: resp["model"].as_str().unwrap_or_default().to_string(),
        finish_reason: resp["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("unknown")
            .chars()
            .take(32)
            .collect(),
        usage: parse_usage(&resp),
        candidate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_accepts_exact_shape() {
        assert!(validate(r#"{"proposal":"a","risks":"b"}"#).is_ok());
    }

    #[test]
    fn schema_rejects_extras_types_and_size() {
        assert!(validate(r#"{"proposal":"a","risks":"b","cmd":"rm -rf /"}"#).is_err());
        assert!(validate(r#"{"proposal":1,"risks":"b"}"#).is_err());
        assert!(validate(r#"{"proposal":"a"}"#).is_err());
        assert!(validate("not json").is_err());
        let big = format!(
            r#"{{"proposal":"{}","risks":"b"}}"#,
            "x".repeat(MAX_FIELD + 1)
        );
        assert!(validate(&big).is_err());
    }

    #[test]
    fn body_pins_privacy_caps_tokens_and_requests_usage() {
        let b = request_body(10_000_000);
        assert_eq!(b["max_tokens"], 2000);
        assert_eq!(b["provider"]["zdr"], true);
        assert_eq!(b["provider"]["data_collection"], "deny");
        assert_eq!(b["provider"]["allow_fallbacks"], false);
        assert_eq!(b["usage"]["include"], true);
    }

    #[test]
    fn url_override_only_for_loopback_http() {
        assert_eq!(base_url(None), DEFAULT_BASE);
        assert_eq!(
            base_url(Some("http://127.0.0.1:8080/v1/")),
            "http://127.0.0.1:8080/v1"
        );
        assert_eq!(base_url(Some("https://evil.example/v1")), DEFAULT_BASE);
        assert_eq!(base_url(Some("http://localhost:1/v1")), DEFAULT_BASE);
    }

    #[test]
    fn usage_parsing_never_invents_cost() {
        let u = parse_usage(&json!({"usage":{"prompt_tokens":10,"completion_tokens":5}}));
        assert_eq!((u.total_tokens, u.cost), (15, None));
        let u = parse_usage(&json!({"usage":{"total_tokens":9,"cost":0.5}}));
        assert_eq!((u.total_tokens, u.cost), (9, Some(0.5)));
        assert_eq!(parse_usage(&json!({"usage":{"cost":-1.0}})).cost, None);
        assert_eq!(parse_usage(&json!({})).total_tokens, 0);
    }

    #[test]
    fn estimate_bounds_a_default_request() {
        let (usd, tokens) = estimate(&request_body(200_000));
        assert!(usd > 0.0 && usd < 0.05, "{usd}");
        assert!(tokens > 1000 && tokens < 4000);
    }

    #[test]
    fn sanitize_redacts_key_and_controls() {
        assert_eq!(sanitize("bad sk-abc\nx", "sk-abc"), "bad [redacted] x");
    }
}
