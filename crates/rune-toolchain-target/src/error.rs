use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("core: {0}")]
    Core(#[from] rune_core::Error),
    #[error("build failed: {0}")]
    Build(String),
    #[error("invalid key: {0}")]
    Key(String),
    #[error("unsafe path in envelope: {0}")]
    UnsafePath(String),
    #[error("verification failed: {0}")]
    Verify(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
