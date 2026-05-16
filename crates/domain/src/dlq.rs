use crate::raw::{compute_payload_hash, to_hex};
use crate::source::{Provider, SourceChannel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DlqErrorCode {
    MalformedJson,
    MissingRequiredField,
    UnknownSchemaBlocking,
    ObjectArchiveWriteFailed,
    ArchiveIndexWriteFailed,
    SecretScanFailed,
    TopicEnvelopeInvalid,
    ProviderInvalid,
    PayloadHashMismatch,
    RawIdInvalid,
    SourceChannelInvalid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DlqErrorCategory {
    Validation,
    Archive,
    Index,
    Eventbus,
    Security,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DlqEvent {
    pub dlq_id: String,
    pub raw_id: Option<String>,
    pub provider: Provider,
    pub topic: String,
    pub source_channel: SourceChannel,
    pub error_code: DlqErrorCode,
    pub error_message: String,
    pub error_category: DlqErrorCategory,
    pub payload_hash: Option<String>,
    pub raw_ref: Option<String>,
    pub dlq_ref: String,
    pub trace_id: Option<String>,
    pub received_at: DateTime<Utc>,
    pub failed_at: DateTime<Utc>,
    pub retryable: bool,
}

impl DlqEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        raw_id: Option<String>,
        provider: Provider,
        topic: String,
        source_channel: SourceChannel,
        error_code: DlqErrorCode,
        error_category: DlqErrorCategory,
        error_message: impl AsRef<str>,
        payload_hash: Option<String>,
        raw_ref: Option<String>,
        dlq_ref: String,
        trace_id: Option<String>,
        received_at: DateTime<Utc>,
        retryable: bool,
    ) -> Self {
        let failed_at = Utc::now();
        let error_message = sanitize_error_message(error_message.as_ref());
        let id_payload = json!({
            "raw_id": raw_id,
            "provider": provider,
            "topic": topic,
            "source_channel": source_channel,
            "error_code": error_code,
            "payload_hash": payload_hash,
            "dlq_ref": dlq_ref,
            "failed_at": failed_at,
        });
        let id_hash = compute_payload_hash(&id_payload);
        Self {
            dlq_id: format!("dlq:{}", id_hash.trim_start_matches("sha256:")),
            raw_id,
            provider,
            topic,
            source_channel,
            error_code,
            error_message,
            error_category,
            payload_hash,
            raw_ref,
            dlq_ref,
            trace_id,
            received_at,
            failed_at,
            retryable,
        }
    }
}

pub fn sanitize_error_message(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("secret=")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("private_key=")
        || lower.contains("passphrase=")
        || lower.contains("signature=")
        || lower.contains("authorization:")
    {
        return "redacted secret-like error message".to_string();
    }

    let mut hasher = Sha256::new();
    if message.len() > 512 {
        hasher.update(message.as_bytes());
        return format!(
            "error message too large; sha256:{}",
            to_hex(&hasher.finalize())
        );
    }

    message.to_string()
}
