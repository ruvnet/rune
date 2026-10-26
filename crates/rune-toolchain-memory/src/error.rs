use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid record: {0}")]
    Invalid(String),
    #[error("unsafe path: {0}")]
    UnsafePath(String),
    #[error("private record {0} cannot be exported")]
    PrivateExport(String),
    #[error("unknown recipe {0}")]
    UnknownRecipe(String),
    #[error("evidence rejected: {0}")]
    EvidenceRejected(String),
    #[error("approval refused: {0}")]
    ApprovalRefused(String),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
