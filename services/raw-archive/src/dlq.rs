use crate::error::RawArchiveError;
use chrono::{DateTime, Utc};
use quantsys_domain::{
    compute_payload_hash, DlqErrorCategory, DlqErrorCode, DlqEvent, Provider, SourceChannel,
};
use quantsys_storage::{
    ArchiveWriteRequest, InMemoryObjectArchive, ObjectArchive, ObjectKeyBuilder,
};
use sha2::{Digest, Sha256};

pub fn infer_provider_channel(topic: &str) -> (Provider, SourceChannel) {
    match topic {
        "raw.polymarket.user" => (Provider::Polymarket, SourceChannel::WsUser),
        "raw.polymarket.market" => (Provider::Polymarket, SourceChannel::WsMarket),
        _ => (Provider::TheRundown, SourceChannel::WsMarket),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn dlq_for_error(
    raw_id: Option<String>,
    provider: Provider,
    topic: String,
    source_channel: SourceChannel,
    err: &RawArchiveError,
    payload_bytes: &[u8],
    received_at: DateTime<Utc>,
    key_builder: &ObjectKeyBuilder,
    archive: &InMemoryObjectArchive,
) -> DlqEvent {
    let payload_hash = byte_hash(payload_bytes);
    let dlq_raw_id = raw_id
        .as_deref()
        .unwrap_or(payload_hash.trim_start_matches("sha256:"));
    let dlq_ref = key_builder.dlq_archive_key(
        provider.slug(),
        source_channel.slug(),
        dlq_raw_id,
        received_at,
    );
    let _ = archive.write(ArchiveWriteRequest::json(
        dlq_ref.clone(),
        payload_bytes.to_vec(),
        dlq_raw_id.to_string(),
    ));

    let (code, category, retryable) = classify_error(err);
    DlqEvent::new(
        raw_id,
        provider,
        topic,
        source_channel,
        code,
        category,
        err.to_string(),
        Some(payload_hash),
        None,
        dlq_ref,
        None,
        received_at,
        retryable,
    )
}

fn classify_error(err: &RawArchiveError) -> (DlqErrorCode, DlqErrorCategory, bool) {
    match err {
        RawArchiveError::Json(_) => (
            DlqErrorCode::MalformedJson,
            DlqErrorCategory::Validation,
            false,
        ),
        RawArchiveError::Archive(_) => (
            DlqErrorCode::ObjectArchiveWriteFailed,
            DlqErrorCategory::Archive,
            true,
        ),
        RawArchiveError::InvalidEnvelope(_) => (
            DlqErrorCode::TopicEnvelopeInvalid,
            DlqErrorCategory::Eventbus,
            false,
        ),
        RawArchiveError::Validation(message) if message.contains("payload_hash") => (
            DlqErrorCode::PayloadHashMismatch,
            DlqErrorCategory::Validation,
            false,
        ),
        RawArchiveError::Validation(message) if message.contains("raw_id") => (
            DlqErrorCode::RawIdInvalid,
            DlqErrorCategory::Validation,
            false,
        ),
        RawArchiveError::Validation(message) if message.contains("secret-like") => (
            DlqErrorCode::SecretScanFailed,
            DlqErrorCategory::Security,
            false,
        ),
        RawArchiveError::Validation(_) => (
            DlqErrorCode::UnknownSchemaBlocking,
            DlqErrorCategory::Validation,
            false,
        ),
    }
}

fn byte_hash(bytes: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice(bytes) {
        return compute_payload_hash(&value);
    }
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", to_hex(&hasher.finalize()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
