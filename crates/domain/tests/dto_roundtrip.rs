use chrono::{TimeZone, Utc};
use quantsys_domain::{
    ErrorInfo, MarketType, NormalizedQuote, Period, Provider, QualityFlags, RawMessage,
    SourceChannel, SourceMode, SourceState, SourceStatus,
};
use serde_json::json;

#[test]
fn raw_message_roundtrips_without_losing_trace_or_payload_hash() {
    let received_at = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
    let raw = RawMessage::new(
        Provider::TheRundown,
        SourceChannel::WsMarket,
        Some("193600383".to_string()),
        Some("event-1".to_string()),
        Some("market-3".to_string()),
        received_at,
        8451294412233,
        "raw/2026/05/15/therundown/ws_market/event-1/193600383.json".to_string(),
        "therundown.v2.ws_market_price.mock.v1".to_string(),
        json!({"meta": {"type": "market_price"}, "data": {"id": 193600383}}),
    );

    let encoded = serde_json::to_string(&raw).unwrap();
    let decoded: RawMessage = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded.provider, Provider::TheRundown);
    assert_eq!(decoded.source_channel, SourceChannel::WsMarket);
    assert_eq!(decoded.provider_message_id.as_deref(), Some("193600383"));
    assert_eq!(decoded.trace_id, raw.trace_id);
    assert!(decoded.payload_hash.starts_with("sha256:"));
}

#[test]
fn normalized_quote_roundtrips_with_moneyline_scope_and_quality_flags() {
    let quote = NormalizedQuote {
        quote_id: "quote-1".to_string(),
        provider: Provider::Polymarket,
        canonical_market_key: Some("nba:lal-bos:full_game:moneyline:home".to_string()),
        canonical_event_id: None,
        provider_event_id: Some("condition-1".to_string()),
        provider_market_id: Some("token-yes".to_string()),
        provider_participant_id: Some("yes".to_string()),
        normalized_participant_id: Some("home".to_string()),
        sport: Some("nba".to_string()),
        market_type: MarketType::Moneyline,
        period: Period::FullGame,
        side: Some("home".to_string()),
        line: None,
        raw_price: Some("0.52".to_string()),
        normalized_probability: Some("0.52".to_string()),
        best_bid: Some("0.51".to_string()),
        best_ask: Some("0.52".to_string()),
        size: Some("100.00".to_string()),
        provider_ts: None,
        ingest_ts: Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 1).unwrap(),
        ingest_mono_ns: 8451294412234,
        raw_ref: "raw/2026/05/15/polymarket/ws_market/condition-1/token-yes.json".to_string(),
        quality_flags: QualityFlags::default(),
    };

    let value = serde_json::to_value(&quote).unwrap();
    let decoded: NormalizedQuote = serde_json::from_value(value).unwrap();

    assert_eq!(decoded.market_type, MarketType::Moneyline);
    assert_eq!(decoded.period, Period::FullGame);
    assert!(!decoded.quality_flags.off_board);
}

#[test]
fn source_state_marks_delayed_or_stale_sources_as_live_blocked() {
    let state = SourceState {
        source: "therundown".to_string(),
        mode: SourceMode::LiveWs,
        tier: Some("delayed".to_string()),
        data_delay_seconds: Some(15),
        websocket_access: Some(true),
        status: SourceStatus::Degraded,
        last_message_at: None,
        last_heartbeat_at: None,
        stale_after_seconds: 30,
        rate_limited: false,
        geoblocked: false,
        error: Some(ErrorInfo::new("SOURCE_DELAYED", "source is delayed")),
        live_signal_allowed: false,
        live_execution_allowed: false,
        block_reason: Some("delayed_source".to_string()),
    };

    let decoded: SourceState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();

    assert_eq!(decoded.status, SourceStatus::Degraded);
    assert_eq!(decoded.block_reason.as_deref(), Some("delayed_source"));
    assert!(!decoded.live_signal_allowed);
    assert!(!decoded.live_execution_allowed);
}
