use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectArchiveBackend {
    InMemory,
    LocalFilesystem,
    S3Compatible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectArchiveConfig {
    pub backend: ObjectArchiveBackend,
    pub root: Option<PathBuf>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ObjectKey(String);

impl ObjectKey {
    pub fn new(value: impl Into<String>) -> Result<Self, ArchiveError> {
        let value = value.into();
        validate_key(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveWriteRequest {
    pub key: String,
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub idempotency_key: String,
}

impl ArchiveWriteRequest {
    pub fn json(
        key: impl Into<String>,
        bytes: Vec<u8>,
        idempotency_key: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            bytes,
            content_type: "application/json".to_string(),
            idempotency_key: idempotency_key.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveWriteResult {
    pub raw_ref: String,
    pub content_hash: String,
    pub bytes_written: usize,
    pub duplicate: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveReadRequest {
    pub raw_ref: String,
}

impl ArchiveReadRequest {
    pub fn by_ref(raw_ref: impl Into<String>) -> Self {
        Self {
            raw_ref: raw_ref.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveReadResult {
    pub raw_ref: String,
    pub bytes: Vec<u8>,
    pub content_hash: String,
}

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("invalid object key {key}: {reason}")]
    InvalidKey { key: String, reason: String },
    #[error("secret-like object key rejected")]
    SecretLikeKey,
    #[error("object not found: {0}")]
    NotFound(String),
    #[error("object already exists with different content: {0}")]
    AlreadyExists(String),
    #[error("local object archive io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("s3-compatible object archive is configured but client write/read is not implemented")]
    UnsupportedS3Backend,
}

pub trait ObjectArchive: Clone + Send + Sync + 'static {
    fn write(&self, request: ArchiveWriteRequest) -> Result<ArchiveWriteResult, ArchiveError>;

    fn read(&self, request: ArchiveReadRequest) -> Result<ArchiveReadResult, ArchiveError>;

    fn write_batch(
        &self,
        requests: Vec<ArchiveWriteRequest>,
    ) -> Vec<Result<ArchiveWriteResult, ArchiveError>> {
        requests
            .into_iter()
            .map(|request| self.write(request))
            .collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryObjectArchive {
    objects: Arc<Mutex<HashMap<String, StoredObject>>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredObject {
    bytes: Vec<u8>,
    content_hash: String,
    content_type: String,
    idempotency_key: String,
}

impl ObjectArchive for InMemoryObjectArchive {
    fn write(&self, request: ArchiveWriteRequest) -> Result<ArchiveWriteResult, ArchiveError> {
        validate_key(&request.key)?;
        let request_hash = content_hash(&request.bytes);
        let mut objects = self.objects.lock().expect("object archive mutex poisoned");
        if let Some(existing) = objects.get(&request.key) {
            if existing.content_hash == request_hash {
                return Ok(ArchiveWriteResult {
                    raw_ref: request.key,
                    content_hash: request_hash,
                    bytes_written: existing.bytes.len(),
                    duplicate: true,
                });
            }
            return Err(ArchiveError::AlreadyExists(request.key));
        }

        let bytes_written = request.bytes.len();
        objects.insert(
            request.key.clone(),
            StoredObject {
                bytes: request.bytes,
                content_hash: request_hash.clone(),
                content_type: request.content_type,
                idempotency_key: request.idempotency_key,
            },
        );
        Ok(ArchiveWriteResult {
            raw_ref: request.key,
            content_hash: request_hash,
            bytes_written,
            duplicate: false,
        })
    }

    fn read(&self, request: ArchiveReadRequest) -> Result<ArchiveReadResult, ArchiveError> {
        validate_key(&request.raw_ref)?;
        let objects = self.objects.lock().expect("object archive mutex poisoned");
        let object = objects
            .get(&request.raw_ref)
            .ok_or_else(|| ArchiveError::NotFound(request.raw_ref.clone()))?;
        Ok(ArchiveReadResult {
            raw_ref: request.raw_ref,
            bytes: object.bytes.clone(),
            content_hash: object.content_hash.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct LocalFilesystemObjectArchive {
    root: PathBuf,
}

impl LocalFilesystemObjectArchive {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(|source| ArchiveError::Io {
            path: root.display().to_string(),
            source,
        })?;
        Ok(Self { root })
    }

    fn path_for(&self, raw_ref: &str) -> Result<PathBuf, ArchiveError> {
        validate_key(raw_ref)?;
        Ok(self.root.join(raw_ref))
    }
}

impl ObjectArchive for LocalFilesystemObjectArchive {
    fn write(&self, request: ArchiveWriteRequest) -> Result<ArchiveWriteResult, ArchiveError> {
        let path = self.path_for(&request.key)?;
        let request_hash = content_hash(&request.bytes);
        if path.exists() {
            let existing = fs::read(&path).map_err(|source| ArchiveError::Io {
                path: path.display().to_string(),
                source,
            })?;
            if content_hash(&existing) == request_hash {
                return Ok(ArchiveWriteResult {
                    raw_ref: request.key,
                    content_hash: request_hash,
                    bytes_written: existing.len(),
                    duplicate: true,
                });
            }
            return Err(ArchiveError::AlreadyExists(request.key));
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ArchiveError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        fs::write(&path, &request.bytes).map_err(|source| ArchiveError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(ArchiveWriteResult {
            raw_ref: request.key,
            content_hash: request_hash,
            bytes_written: request.bytes.len(),
            duplicate: false,
        })
    }

    fn read(&self, request: ArchiveReadRequest) -> Result<ArchiveReadResult, ArchiveError> {
        let path = self.path_for(&request.raw_ref)?;
        let bytes = fs::read(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ArchiveError::NotFound(request.raw_ref.clone())
            } else {
                ArchiveError::Io {
                    path: path.display().to_string(),
                    source,
                }
            }
        })?;
        Ok(ArchiveReadResult {
            raw_ref: request.raw_ref,
            content_hash: content_hash(&bytes),
            bytes,
        })
    }
}

#[derive(Clone, Debug)]
pub struct S3CompatibleObjectArchive {
    pub config: ObjectArchiveConfig,
}

impl ObjectArchive for S3CompatibleObjectArchive {
    fn write(&self, _request: ArchiveWriteRequest) -> Result<ArchiveWriteResult, ArchiveError> {
        Err(ArchiveError::UnsupportedS3Backend)
    }

    fn read(&self, _request: ArchiveReadRequest) -> Result<ArchiveReadResult, ArchiveError> {
        Err(ArchiveError::UnsupportedS3Backend)
    }
}

fn validate_key(key: &str) -> Result<(), ArchiveError> {
    if key.trim().is_empty() || key.starts_with('/') || key.contains("..") {
        return Err(ArchiveError::InvalidKey {
            key: key.to_string(),
            reason: "key must be relative and non-empty".to_string(),
        });
    }
    let lower = key.to_ascii_lowercase();
    if lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("private_key")
        || lower.contains("passphrase")
        || lower.contains("signature")
        || lower.contains("authorization")
    {
        return Err(ArchiveError::SecretLikeKey);
    }
    Ok(())
}

fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", to_hex(&hasher.finalize()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
