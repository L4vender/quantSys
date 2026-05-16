use chrono::{TimeZone, Utc};
use quantsys_domain::{
    compute_payload_hash, compute_raw_id, scan_json_for_secrets, DlqErrorCategory, DlqErrorCode,
    DlqEvent, Provider, RawArchiveStatus, RawMessage, SourceChannel,
};
use serde_json::json;

#[test]
fn payload_hash_is_deterministic_for_json_key_order() {
    let left = json!({"b": 2, "a": {"d": 4, "c": 3}});
    let right = json!({"a": {"c": 3, "d": 4}, "b": 2});

    assert_eq!(compute_payload_hash(&left), compute_payload_hash(&right));
    assert!(compute_payload_hash(&left).starts_with("sha256:"));
}

#[test]
fn raw_id_is_deterministic_and_does_not_use_timestamps() {
    let payload = json!({"meta": {"type": "market_price"}, "data": {"id": 193600383}});
    let payload_hash = compute_payload_hash(&payload);

    let left = compute_raw_id(
        &Provider::TheRundown,
        &SourceChannel::WsMarket,
        Some("193600383"),
        Some("event-1"),
        Some("market-3"),
        &payload_hash,
    );
    let right = compute_raw_id(
        &Provider::TheRundown,
        &SourceChannel::WsMarket,
        Some("193600383"),
        Some("event-1"),
        Some("market-3"),
        &payload_hash,
    );

    assert_eq!(left, right);
    assert!(left.contains("therundown:ws_market:event-1:market-3:193600383:"));
}

#[test]
fn raw_message_new_sets_phase5_archive_defaults() {
    let raw = RawMessage::new(
        Provider::Polymarket,
        SourceChannel::WsMarket,
        Some("price-change-1".to_string()),
        Some("condition-1".to_string()),
        Some("asset-yes".to_string()),
        Utc.with_ymd_and_hms(2026, 5, 16, 9, 0, 0).unwrap(),
        42,
        "raw/polymarket/ws_market/2026/05/16/09/raw-1.json".to_string(),
        "polymarket.market_ws.mock.v1".to_string(),
        json!({"event_type": "price_change", "market": "condition-1"}),
    );

    assert_eq!(raw.archive_status, RawArchiveStatus::Received);
    assert!(!raw.quality_flags.stale);
    assert!(raw.payload_hash.starts_with("sha256:"));
}

#[test]
fn secret_scan_rejects_secret_like_payloads_but_allows_redacted_values() {
    let secret = json!({
        "auth": {
            "apiKey": "pm_live_123456789",
            "secret": "super-secret-value",
            "signature": "0xabcdef"
        }
    });
    let redacted = json!({
        "auth": {
            "apiKey": "<redacted-api-key>",
            "secret": "<redacted>",
            "signature": "<redacted-signature>"
        }
    });

    assert!(scan_json_for_secrets(&secret).is_err());
    assert!(scan_json_for_secrets(&redacted).is_ok());
}

#[test]
fn dlq_event_redacts_error_messages_and_keeps_replay_metadata() {
    let event = DlqEvent::new(
        Some("raw-1".to_string()),
        Provider::Polymarket,
        "raw.polymarket.user".to_string(),
        SourceChannel::WsUser,
        DlqErrorCode::SecretScanFailed,
        DlqErrorCategory::Validation,
        "secret=super-secret-value should never appear",
        Some("sha256:abc".to_string()),
        None,
        "dlq/polymarket/ws_user/2026/05/16/09/raw-1.json".to_string(),
        Some("trace-1".to_string()),
        Utc.with_ymd_and_hms(2026, 5, 16, 9, 0, 0).unwrap(),
        false,
    );

    assert_eq!(event.error_code, DlqErrorCode::SecretScanFailed);
    assert!(!event.error_message.contains("super-secret-value"));
    assert_eq!(event.provider, Provider::Polymarket);
    assert_eq!(event.source_channel, SourceChannel::WsUser);
    assert!(event.dlq_id.starts_with("dlq:"));
}
