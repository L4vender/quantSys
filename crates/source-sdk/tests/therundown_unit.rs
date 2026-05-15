use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use quantsys_domain::{Provider, SourceChannel, SourceMode, SourceStatus};
use quantsys_source_sdk::therundown::{
    build_events_bootstrap_url, build_markets_delta_url, build_ws_url, parse_retry_after, ApiKey,
    DeltaCursor, EntitlementHeaders, ParsedPayloadKind, ParserError, SubscriptionFilters,
    TheRundownBackoff, TheRundownParser, TheRundownStateMachine,
};
use serde_json::json;
use std::time::Duration;

fn fixture(name: &str) -> serde_json::Value {
    let path = format!(
        "{}/../../tests/fixtures/external/therundown/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn therundown_rest_url_construction_uses_v2_contract_paths() {
    let base = "https://therundown.io/api/v2/";

    assert_eq!(
        build_events_bootstrap_url(base, 4, "2026-05-15").unwrap(),
        "https://therundown.io/api/v2/sports/4/events/2026-05-15"
    );
    assert_eq!(
        build_markets_delta_url(base, "tr_delta_20260515_000001").unwrap(),
        "https://therundown.io/api/v2/markets/delta?last_id=tr_delta_20260515_000001"
    );
}

#[test]
fn therundown_auth_header_and_debug_never_expose_api_key() {
    let key = ApiKey::new("mock_therundown_key_for_tests");

    assert_eq!(key.header_name(), "X-TheRundown-Key");
    assert_eq!(key.expose_for_transport(), "mock_therundown_key_for_tests");
    assert!(!format!("{key:?}").contains("mock_therundown_key_for_tests"));
    assert!(!key.to_string().contains("mock_therundown_key_for_tests"));
}

#[test]
fn therundown_header_parser_extracts_entitlement_rate_limit_and_datapoints() {
    let value = fixture("rate_limit_headers.json");
    let headers = value["successful_response_headers"].as_object().unwrap();
    let parsed = EntitlementHeaders::from_pairs(
        headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().unwrap())),
    );

    assert_eq!(parsed.tier.as_deref(), Some("ultra"));
    assert_eq!(parsed.rate_limit, Some(10));
    assert_eq!(parsed.data_delay_seconds, Some(0));
    assert_eq!(parsed.websocket_access, Some(true));
    assert_eq!(parsed.datapoints_remaining, Some(39_999_994));
    assert_eq!(parsed.datapoints_period.as_deref(), Some("monthly"));
    assert!(!parsed.datapoints_exhausted());
}

#[test]
fn therundown_retry_after_parser_accepts_seconds_and_http_dates() {
    let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();

    assert_eq!(
        parse_retry_after("2", now).unwrap(),
        Some(Duration::from_secs(2))
    );
    assert_eq!(
        parse_retry_after("Fri, 15 May 2026 12:00:05 GMT", now).unwrap(),
        Some(Duration::from_secs(5))
    );
}

#[test]
fn therundown_rate_and_datapoint_budget_exhaustion_are_detected() {
    let headers = EntitlementHeaders::from_pairs([
        ("Retry-After", "7"),
        ("X-Datapoints-Remaining", "0"),
        ("X-Rate-Limit", "10"),
    ]);

    assert_eq!(headers.retry_after, Some(Duration::from_secs(7)));
    assert!(headers.datapoints_exhausted());
}

#[test]
fn therundown_backoff_uses_exponential_delay_with_bounded_jitter() {
    let backoff = TheRundownBackoff::new(500, 30_000, 250, 5);

    assert_eq!(backoff.delay_ms(0), 500);
    assert_eq!(backoff.delay_ms(1), 1_000);
    assert!(backoff.delay_with_jitter_ms(4) >= backoff.delay_ms(4));
    assert!(backoff.delay_with_jitter_ms(4) <= backoff.delay_ms(4) + 250);
    assert!(backoff.should_rebootstrap_after_attempt(5));
}

#[test]
fn therundown_heartbeat_stale_detector_marks_stale_after_threshold() {
    let machine = TheRundownStateMachine::new(30);
    let last_seen = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();

    assert!(!machine.is_stale(Some(last_seen), last_seen + ChronoDuration::seconds(30)));
    assert!(machine.is_stale(Some(last_seen), last_seen + ChronoDuration::seconds(31)));
    assert!(machine.is_stale(None, last_seen));
}

#[test]
fn therundown_delta_cursor_updates_and_decides_stale_recovery() {
    let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
    let mut cursor = DeltaCursor::new(30);

    assert!(cursor
        .update_from_bootstrap(&fixture("events_bootstrap.json"), now)
        .unwrap()
        .is_complete());
    assert_eq!(cursor.last_id(), Some("tr_delta_20260515_000001"));
    assert!(!cursor.is_stale(now + ChronoDuration::minutes(29)));
    assert!(cursor.is_stale(now + ChronoDuration::minutes(31)));

    cursor
        .update_from_delta(
            &fixture("markets_delta.json"),
            now + ChronoDuration::minutes(1),
        )
        .unwrap();
    assert_eq!(cursor.last_id(), Some("tr_delta_20260515_000004"));
}

#[test]
fn therundown_payload_hash_and_raw_message_construction_are_deterministic() {
    let parser = TheRundownParser::new("therundown.v2.ws_market_price.mock.v1");
    let payload = fixture("ws_market_price.json");
    let received_at = Utc.with_ymd_and_hms(2026, 5, 15, 12, 1, 0).unwrap();

    let left = parser
        .parse_ws_payload(payload.clone(), received_at, 123)
        .unwrap();
    let right = parser.parse_ws_payload(payload, received_at, 123).unwrap();

    assert_eq!(left.raw.payload_hash, right.raw.payload_hash);
    assert_eq!(left.raw.raw_id, right.raw.raw_id);
    assert_eq!(left.raw.provider, Provider::TheRundown);
    assert_eq!(left.raw.source_channel, SourceChannel::WsMarket);
    assert_eq!(left.raw.provider_message_id.as_deref(), Some("193600383"));
    assert_eq!(
        left.raw.provider_event_id.as_deref(),
        Some("tr_evt_mock_20260515_lal_bos")
    );
}

#[test]
fn therundown_parser_dispatches_market_price_heartbeat_and_unknown_types() {
    let parser = TheRundownParser::new("therundown.v2.test");
    let received_at = Utc.with_ymd_and_hms(2026, 5, 15, 12, 1, 0).unwrap();

    let market = parser
        .parse_ws_payload(fixture("ws_market_price.json"), received_at, 1)
        .unwrap();
    assert_eq!(market.kind, ParsedPayloadKind::MarketPrice);
    assert!(!market.quality_flags.off_board);

    let heartbeat = parser
        .parse_ws_payload(fixture("ws_heartbeat.json"), received_at, 2)
        .unwrap();
    assert_eq!(heartbeat.kind, ParsedPayloadKind::Heartbeat);

    let unknown = parser
        .parse_ws_payload(
            json!({"meta": {"type": "bookmaker_status"}, "data": {"anything": true}}),
            received_at,
            3,
        )
        .unwrap();
    assert_eq!(
        unknown.kind,
        ParsedPayloadKind::Unknown {
            meta_type: Some("bookmaker_status".to_string())
        }
    );
    assert!(unknown.quality_flags.unknown_schema);
}

#[test]
fn therundown_missing_required_market_price_field_returns_schema_error() {
    let parser = TheRundownParser::new("therundown.v2.test");
    let mut payload = fixture("ws_market_price.json");
    payload["data"].as_object_mut().unwrap().remove("event_id");

    let err = parser
        .parse_ws_payload(
            payload,
            Utc.with_ymd_and_hms(2026, 5, 15, 12, 1, 0).unwrap(),
            1,
        )
        .unwrap_err();

    assert_eq!(
        err,
        ParserError::MissingRequiredField {
            field: "data.event_id".to_string()
        }
    );
}

#[test]
fn therundown_off_board_sentinel_sets_raw_marker_only() {
    let parser = TheRundownParser::new("therundown.v2.off_board_price.mock.v1");
    let parsed = parser
        .parse_ws_payload(
            fixture("off_board_price.json"),
            Utc.with_ymd_and_hms(2026, 5, 15, 12, 1, 0).unwrap(),
            1,
        )
        .unwrap();

    assert_eq!(parsed.kind, ParsedPayloadKind::MarketPrice);
    assert!(parsed.quality_flags.off_board);
    assert_eq!(parsed.raw.payload["data"]["price"], "0.0001");
}

#[test]
fn therundown_source_state_gates_delayed_no_ws_stale_and_datapoints() {
    let machine = TheRundownStateMachine::new(30);

    let delayed = machine.from_headers(
        SourceMode::LiveWs,
        &EntitlementHeaders::from_pairs([
            ("X-Data-Delay-Seconds", "15"),
            ("X-Websocket-Access", "true"),
        ]),
    );
    assert_eq!(delayed.status, SourceStatus::DataDelayDetected);
    assert_eq!(delayed.block_reason.as_deref(), Some("delayed_source"));
    assert!(!delayed.live_signal_allowed);
    assert!(!delayed.live_execution_allowed);

    let no_ws = machine.from_headers(
        SourceMode::LiveWs,
        &EntitlementHeaders::from_pairs([
            ("X-Data-Delay-Seconds", "0"),
            ("X-Websocket-Access", "false"),
        ]),
    );
    assert_eq!(no_ws.status, SourceStatus::NoWebsocketAccess);

    let exhausted = machine.from_headers(
        SourceMode::LiveWs,
        &EntitlementHeaders::from_pairs([
            ("X-Data-Delay-Seconds", "0"),
            ("X-Websocket-Access", "true"),
            ("X-Datapoints-Remaining", "0"),
        ]),
    );
    assert_eq!(exhausted.status, SourceStatus::DatapointsExhausted);

    let stale = machine.mark_stale(SourceMode::LiveWs);
    assert_eq!(stale.status, SourceStatus::Stale);
    assert_eq!(stale.block_reason.as_deref(), Some("stale_source"));
}

#[test]
fn therundown_secret_scrubber_removes_keys_and_query_params() {
    let key = ApiKey::new("mock_therundown_key_for_tests");
    let text = "wss://therundown.io/api/v2/ws/markets?key=mock_therundown_key_for_tests X-TheRundown-Key: mock_therundown_key_for_tests";

    let scrubbed = key.scrub(text);
    assert!(!scrubbed.contains("mock_therundown_key_for_tests"));
    assert!(scrubbed.contains("key=<redacted>"));
    assert!(scrubbed.contains("X-TheRundown-Key: <redacted>"));
}

#[test]
fn therundown_ws_url_uses_query_key_auth_and_subscription_filters() {
    let key = ApiKey::new("mock_therundown_key_for_tests");
    let filters = SubscriptionFilters {
        sport_ids: vec![4, 7],
        market_ids: vec![1],
        affiliate_ids: vec![19, 23],
        event_ids: vec!["tr_evt_mock_20260515_lal_bos".to_string()],
    };

    let url = build_ws_url("wss://therundown.io/api/v2/ws/markets", &key, &filters).unwrap();

    assert!(url.contains("key=mock_therundown_key_for_tests"));
    assert!(url.contains("sport_ids=4%2C7"));
    assert!(url.contains("market_ids=1"));
    assert!(url.contains("affiliate_ids=19%2C23"));
    assert!(url.contains("event_ids=tr_evt_mock_20260515_lal_bos"));
    assert!(filters.has_any_filter());
}
