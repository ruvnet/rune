use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid plan: {0}")]
    Json(#[from] serde_json::Error),
    #[error("stale plan: {field} changed (plan {planned}, now {actual})")]
    Stale {
        field: &'static str,
        planned: String,
        actual: String,
    },
    #[error("plan id mismatch: file claims {claimed}, content hashes to {actual}")]
    IdMismatch { claimed: String, actual: String },
    #[error("unsupported plan schema {0}")]
    Schema(String),
    #[error("policy rejected: {0}")]
    Policy(String),
    #[error("no approved plan to {0}")]
    NoPlan(&'static str),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
