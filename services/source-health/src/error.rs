use thiserror::Error;

#[derive(Debug, Error)]
pub enum SourceHealthError {
    #[error("source not found: {0}")]
    SourceNotFound(String),
}
