use crate::source::{Provider, SourceChannel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualityFlags {
    pub off_board: bool,
    pub delayed_source: bool,
    pub stale: bool,
    pub unknown_schema: bool,
    pub missing_required_field: bool,
}

impl QualityFlags {
    pub fn off_board() -> Self {
        Self {
            off_board: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RawArchiveStatus {
    #[default]
    Received,
    Archived,
    Duplicate,
    Dlq,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RawMessage {
    pub raw_id: String,
    pub provider: Provider,
    pub source_channel: SourceChannel,
    pub provider_message_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub provider_market_id: Option<String>,
    pub received_at: DateTime<Utc>,
    pub received_mono_ns: u64,
    pub payload_hash: String,
    pub raw_ref: String,
    pub schema_version: String,
    pub trace_id: Uuid,
    pub payload: Value,
    #[serde(default)]
    pub quality_flags: QualityFlags,
    #[serde(default)]
    pub archive_status: RawArchiveStatus,
}

impl RawMessage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Provider,
        source_channel: SourceChannel,
        provider_message_id: Option<String>,
        provider_event_id: Option<String>,
        provider_market_id: Option<String>,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
        raw_ref: String,
        schema_version: String,
        payload: Value,
    ) -> Self {
        let payload_hash = compute_payload_hash(&payload);
        let raw_id = compute_raw_id(
            &provider,
            &source_channel,
            provider_message_id.as_deref(),
            provider_event_id.as_deref(),
            provider_market_id.as_deref(),
            &payload_hash,
        );

        Self {
            raw_id,
            provider,
            source_channel,
            provider_message_id,
            provider_event_id,
            provider_market_id,
            received_at,
            received_mono_ns,
            payload_hash,
            raw_ref,
            schema_version,
            trace_id: Uuid::new_v4(),
            payload,
            quality_flags: QualityFlags::default(),
            archive_status: RawArchiveStatus::Received,
        }
    }
}

pub fn compute_payload_hash(value: &Value) -> String {
    let canonical = canonical_json(value);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("sha256:{}", to_hex(&hasher.finalize()))
}

pub fn compute_raw_id(
    provider: &Provider,
    source_channel: &SourceChannel,
    provider_message_id: Option<&str>,
    provider_event_id: Option<&str>,
    provider_market_id: Option<&str>,
    payload_hash: &str,
) -> String {
    [
        provider.slug(),
        source_channel.slug(),
        stable_component(provider_event_id, "unknown_event").as_str(),
        stable_component(provider_market_id, "unknown_market").as_str(),
        stable_component(provider_message_id, "unknown_message").as_str(),
        payload_hash.trim_start_matches("sha256:"),
    ]
    .join(":")
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SecretScanError {
    #[error("secret-like field detected at {path}")]
    SecretLikeField { path: String },
}

pub fn scan_json_for_secrets(value: &Value) -> Result<(), SecretScanError> {
    scan_json_value(value, "$")
}

fn scan_json_value(value: &Value, path: &str) -> Result<(), SecretScanError> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let child_path = format!("{path}.{key}");
                if is_secret_key(key) && !is_redacted_or_empty(value) {
                    return Err(SecretScanError::SecretLikeField { path: child_path });
                }
                scan_json_value(value, &child_path)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (idx, value) in items.iter().enumerate() {
                scan_json_value(value, &format!("{path}[{idx}]"))?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
    }
}

fn is_secret_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "apikey"
            | "secret"
            | "passphrase"
            | "signature"
            | "privatekey"
            | "xtherundownkey"
            | "authorization"
            | "authpayload"
    )
}

fn is_redacted_or_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => {
            let trimmed = value.trim().to_ascii_lowercase();
            trimmed.is_empty()
                || trimmed.contains("redacted")
                || trimmed == "<hidden>"
                || trimmed == "***"
        }
        _ => false,
    }
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).expect("string serialization"),
        Value::Array(items) => {
            let inner = items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{inner}]")
        }
        Value::Object(map) => {
            let mut pairs = map.iter().collect::<Vec<_>>();
            pairs.sort_by(|left, right| left.0.cmp(right.0));
            let inner = pairs
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key serialization"),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
    }
}

fn stable_component(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
