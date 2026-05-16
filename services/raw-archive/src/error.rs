use quantsys_storage::ArchiveError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RawArchiveError {
    #[error("invalid raw envelope: {0}")]
    InvalidEnvelope(String),
    #[error("raw validation failed: {0}")]
    Validation(String),
    #[error("object archive error: {0}")]
    Archive(#[from] ArchiveError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
