use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] rune_core::Error),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("path escapes the artifact root or traverses a symlink: {0}")]
    Escape(String),
    #[error("invalid object key or name: {0:?}")]
    InvalidKey(String),
    #[error(
        "ownership manifest changed concurrently (expected version {expected}, found {found})"
    )]
    Conflict { expected: u64, found: u64 },
    #[error("ownership manifest is locked by pid {0}")]
    Locked(u32),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("refused: {0}")]
    Refused(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
