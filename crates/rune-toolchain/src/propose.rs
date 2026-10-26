//! One bounded, synthetic OpenRouter proposal request. Returned text is data:
//! it is validated against a fixed schema, printed, and never applied.

use serde_json::{json, Value};
use std::time::Duration;

const MODEL: &str = "moonshotai/kimi-k3.1-pro";
const URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MAX_FIELD: usize = 20_000;

pub fn request_body(max_tokens: u64) -> Value {
    json!({
        "model": MODEL,
        "max_tokens": max_tokens.min(1000),
        "temperature": 0,
        "provider": {"data_collection": "deny", "zdr": true,
                     "require_parameters": true, "allow_fallbacks": false},
        "messages": [
            {"role": "system", "content": "Return a JSON object with exactly keys proposal and risks. Both values must be strings. Suggest one Rust build optimization. Do not output shell commands."},
            {"role": "user", "content": "Synthetic fixture: a Rust library computes squared Euclidean distances over f32 arrays. Propose a measurable optimization and its semantic risks. This is discovery only."}
        ]
    })
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

/// Returns the JSON report line. Errors never include request or provider bodies.
pub fn run(key: &str, max_tokens: u64, timeout_s: u64) -> Result<Value, ()> {
    let resp: Value = ureq::post(URL)
        .timeout(Duration::from_secs(timeout_s.clamp(1, 60)))
        .set("Authorization", &format!("Bearer {key}"))
        .send_json(request_body(max_tokens))
        .map_err(|_| ())?
        .into_json()
        .map_err(|_| ())?;
    let content = resp["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(())?;
    let candidate = validate(content)?;
    Ok(
        json!({"status": "proposal_only", "model": resp["model"], "id": resp["id"],
              "usage": resp["usage"], "candidate": candidate, "validated": false}),
    )
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
    fn body_pins_privacy_and_caps_tokens() {
        let b = request_body(10_000_000);
        assert_eq!(b["max_tokens"], 1000);
        assert_eq!(b["provider"]["zdr"], true);
        assert_eq!(b["provider"]["allow_fallbacks"], false);
    }
}
