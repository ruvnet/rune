//! Detached ed25519 signatures over the artifact manifest.
//!
//! Signatures prove provenance (who vouched) and integrity (the manifest and,
//! through its digests, every listed file is unchanged). They say nothing about
//! whether the program is correct. This module never generates or stores keys:
//! the signing key is read from a path the caller supplies.

use crate::Error;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SIG_FILE: &str = "artifact.sig";
pub const ALG: &str = "ed25519";
const CONTEXT: &[u8] = b"rune.artifact/1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigFile {
    pub alg: String,
    pub public_key: String,
    pub signature: String,
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn from_hex(s: &str) -> Result<Vec<u8>, Error> {
    let s = s.trim();
    if s.len() & 1 == 1 || !s.is_ascii() {
        return Err(Error::Key(
            "hex must be an even count of ASCII digits".into(),
        ));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error::Key("invalid hex digit".into()))
        })
        .collect()
}

/// 32 key bytes: either raw (exactly 32 bytes) or 64 hex characters.
fn read_key_bytes(path: &Path) -> Result<[u8; 32], Error> {
    let raw = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    let bytes = if raw.len() == 32 {
        raw
    } else {
        from_hex(&String::from_utf8_lossy(&raw))?
    };
    bytes
        .try_into()
        .map_err(|_| Error::Key("key must be 32 bytes (raw or 64 hex characters)".into()))
}

pub fn load_signing_key(path: &Path) -> Result<SigningKey, Error> {
    Ok(SigningKey::from_bytes(&read_key_bytes(path)?))
}

pub fn load_verifying_key(path: &Path) -> Result<VerifyingKey, Error> {
    VerifyingKey::from_bytes(&read_key_bytes(path)?).map_err(|e| Error::Key(e.to_string()))
}

fn message(manifest: &[u8]) -> Vec<u8> {
    let mut m = CONTEXT.to_vec();
    m.extend_from_slice(manifest);
    m
}

pub fn sign_manifest(key: &SigningKey, manifest: &[u8]) -> SigFile {
    let sig = key.sign(&message(manifest));
    SigFile {
        alg: ALG.into(),
        public_key: to_hex(key.verifying_key().as_bytes()),
        signature: to_hex(&sig.to_bytes()),
    }
}

/// Separate signature domain: an artifact signature cannot authorize promotion.
pub fn sign_evaluation(key: &SigningKey, payload: &[u8]) -> SigFile {
    let mut msg = b"rune.evaluation/1\0".to_vec();
    msg.extend_from_slice(payload);
    let sig = key.sign(&msg);
    SigFile {
        alg: ALG.into(),
        public_key: to_hex(key.verifying_key().as_bytes()),
        signature: to_hex(&sig.to_bytes()),
    }
}

pub fn verify_evaluation(key: &VerifyingKey, sig: &SigFile, payload: &[u8]) -> Result<(), Error> {
    if sig.alg != ALG || sig.public_key != to_hex(key.as_bytes()) {
        return Err(Error::Verify(
            "evaluation signer is not the trusted key".into(),
        ));
    }
    let bytes: [u8; 64] = from_hex(&sig.signature)?
        .try_into()
        .map_err(|_| Error::Verify("signature must be 64 bytes".into()))?;
    let mut msg = b"rune.evaluation/1\0".to_vec();
    msg.extend_from_slice(payload);
    key.verify_strict(&msg, &Signature::from_bytes(&bytes))
        .map_err(|_| Error::Verify("evaluation signature mismatch".into()))
}

/// Verify `sig` over `manifest` with `key` (strict: rejects malleable forms).
pub fn verify_manifest(key: &VerifyingKey, sig: &SigFile, manifest: &[u8]) -> Result<(), Error> {
    if sig.alg != ALG {
        return Err(Error::Verify(format!(
            "unsupported signature alg {}",
            sig.alg
        )));
    }
    let bytes: [u8; 64] = from_hex(&sig.signature)
        .map_err(|e| Error::Verify(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Verify("signature must be 64 bytes".into()))?;
    key.verify_strict(&message(manifest), &Signature::from_bytes(&bytes))
        .map_err(|_| Error::Verify("signature does not match manifest".into()))
}

pub fn embedded_key(sig: &SigFile) -> Result<VerifyingKey, Error> {
    let bytes: [u8; 32] = from_hex(&sig.public_key)
        .map_err(|e| Error::Verify(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Verify("public key must be 32 bytes".into()))?;
    VerifyingKey::from_bytes(&bytes).map_err(|e| Error::Verify(e.to_string()))
}
