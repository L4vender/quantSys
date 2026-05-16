use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use quantsys_domain::{Provider, SourceChannel, SourceMode, SourceStatus};
use quantsys_source_sdk::polymarket::{
    build_discovery_url, build_market_subscription_payload, build_user_subscription_payload,
    redact_secret_json, validate_market_subscription_payload, DiscoveryFilters, GeoblockStatus,
    L2Credentials, ParsedPolymarketKind, ParserError, PolymarketParser, PolymarketStateMachine,
    TimeProbe, TokenCache,
};
use serde_json::json;

fn fixture(name: &str) -> serde_json::Value {
    let path = format!(
        "{}/../../tests/fixtures/external/polymarket/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn received_at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 15, 12, 1, 0).unwrap()
}

#[test]
fn market_subscription_payload_uses_assets_ids_and_custom_feature_contract() {
    let payload = build_market_subscription_payload(
        &[
            "pm_asset_yes_mock_001".to_string(),
            "pm_asset_no_mock_001".to_string(),
        ],
        true,
    )
    .unwrap();

    assert_eq!(payload["type"], "market");
    assert_eq!(payload["custom_feature_enabled"], true);
    assert!(payload.get("assets_ids").is_some());
    assert!(payload.get("asset_ids").is_none());
    validate_market_subscription_payload(&payload).unwrap();

    let invalid = json!({"type": "market", "asset_ids": ["wrong"]});
    assert!(validate_market_subscription_payload(&invalid)
        .unwrap_err()
        .to_string()
        .contains("assets_ids"));
}

#[test]
fn user_subscription_payload_uses_markets_condition_ids_and_redacts_auth() {
    let auth = L2Credentials::new("api-key-value", "secret-value", "passphrase-value");
    let payload =
        build_user_subscription_payload(&auth, &["pm_condition_mock_lal_bos_moneyline".into()])
            .unwrap();

    assert_eq!(payload["type"], "user");
    assert_eq!(payload["markets"][0], "pm_condition_mock_lal_bos_moneyline");
    assert_eq!(payload["auth"]["apiKey"], "api-key-value");

    let redacted = redact_secret_json(&payload);
    assert_eq!(redacted["auth"]["apiKey"], "<redacted>");
    assert_eq!(redacted["auth"]["secret"], "<redacted>");
    assert_eq!(redacted["auth"]["passphrase"], "<redacted>");
    assert!(!format!("{auth:?}").contains("secret-value"));
    assert!(!auth.to_string().contains("passphrase-value"));
}

#[test]
fn discovery_url_targets_polymarket_games_tag_for_event_level_markets() {
    let url =
        build_discovery_url("https://gamma-api.polymarket.com", 100, 0, Some(100_639)).unwrap();

    assert!(url.contains("/events?"));
    assert!(url.contains("active=true"));
    assert!(url.contains("closed=false"));
    assert!(url.contains("tag_id=100639"));
}

#[test]
fn discovery_parser_filters_active_open_sports_markets_and_builds_token_cache() {
    let parser = PolymarketParser::new("polymarket.discovery.test");
    let result = parser
        .parse_discovery_payload(
            fixture("market_discovery_events.json"),
            &DiscoveryFilters::sports_default(),
            received_at(),
            10,
        )
        .unwrap();

    assert_eq!(result.markets.len(), 1);
    assert_eq!(result.filtered_closed, 1);
    assert_eq!(result.filtered_non_sports, 1);
    assert_eq!(result.filtered_unsupported_market_types, 1);
    assert_eq!(
        result.markets[0].event_id.as_deref(),
        Some("pm_event_mock_lal_bos")
    );
    assert_eq!(
        result.markets[0].condition_id,
        "pm_condition_mock_lal_bos_moneyline"
    );
    assert_eq!(result.markets[0].sport, "nba");
    assert_eq!(result.markets[0].league, "nba");
    assert_eq!(
        result.markets[0].event_title.as_deref(),
        Some("Los Angeles Lakers vs Boston Celtics")
    );
    assert_eq!(
        result.markets[0].start_time.as_deref(),
        Some("2026-05-15T23:00:00Z")
    );
    assert_eq!(result.markets[0].market_type.as_deref(), Some("moneyline"));
    assert_eq!(
        result.markets[0].token_ids,
        vec!["pm_asset_yes_mock_001", "pm_asset_no_mock_001"]
    );
    assert_eq!(result.raw.provider, Provider::Polymarket);
    assert_eq!(result.raw.source_channel, SourceChannel::RestDiscovery);

    let mut cache = TokenCache::new(300);
    cache.upsert_markets(result.markets.clone(), received_at());
    assert_eq!(
        cache
            .token_ids_for_condition("pm_condition_mock_lal_bos_moneyline")
            .unwrap(),
        vec!["pm_asset_yes_mock_001", "pm_asset_no_mock_001"]
    );
    assert_eq!(
        cache.condition_for_token("pm_asset_yes_mock_001"),
        Some("pm_condition_mock_lal_bos_moneyline")
    );
    assert_eq!(
        cache.outcome_for_token("pm_asset_no_mock_001"),
        Some("Celtics")
    );
    assert_eq!(
        cache.condition_for_slug("nba-lakers-celtics-moneyline"),
        Some("pm_condition_mock_lal_bos_moneyline")
    );
    assert_eq!(
        cache.condition_ids_for_event("pm_event_mock_lal_bos"),
        vec!["pm_condition_mock_lal_bos_moneyline"]
    );
    assert_eq!(
        cache
            .market_for_token("pm_asset_yes_mock_001")
            .and_then(|market| market.event_title.as_deref()),
        Some("Los Angeles Lakers vs Boston Celtics")
    );
    assert!(!cache.is_expired(received_at() + ChronoDuration::seconds(299)));
    assert!(cache.is_expired(received_at() + ChronoDuration::seconds(301)));
}

#[test]
fn market_ws_parser_dispatches_supported_market_event_types() {
    let parser = PolymarketParser::new("polymarket.ws.market.test");

    let cases = [
        (
            "market_book.json",
            ParsedPolymarketKind::MarketBook,
            "pm_condition_mock_lal_bos_moneyline",
        ),
        (
            "market_price_change.json",
            ParsedPolymarketKind::MarketPriceChange,
            "pm_condition_mock_lal_bos_moneyline",
        ),
        (
            "market_best_bid_ask.json",
            ParsedPolymarketKind::MarketBestBidAsk,
            "pm_condition_mock_lal_bos_moneyline",
        ),
        (
            "market_last_trade_price.json",
            ParsedPolymarketKind::MarketLastTradePrice,
            "pm_condition_mock_lal_bos_moneyline",
        ),
        (
            "market_tick_size_change.json",
            ParsedPolymarketKind::MarketTickSizeChange,
            "pm_condition_mock_lal_bos_moneyline",
        ),
        (
            "market_new_market.json",
            ParsedPolymarketKind::NewMarket,
            "pm_condition_mock_new_sports",
        ),
        (
            "market_resolved.json",
            ParsedPolymarketKind::MarketResolved,
            "pm_condition_mock_lal_bos_moneyline",
        ),
    ];

    for (fixture_name, expected_kind, expected_condition) in cases {
        let parsed = parser
            .parse_market_ws_payload(fixture(fixture_name), received_at(), 11)
            .unwrap();
        assert_eq!(parsed.kind, expected_kind, "{fixture_name}");
        assert_eq!(parsed.raw.provider, Provider::Polymarket);
        assert_eq!(parsed.raw.source_channel, SourceChannel::WsMarket);
        assert_eq!(
            parsed.raw.provider_event_id.as_deref(),
            Some(expected_condition)
        );
    }
}

#[test]
fn market_ws_parser_preserves_unknown_and_rejects_missing_required_fields() {
    let parser = PolymarketParser::new("polymarket.ws.market.test");
    let unknown = parser
        .parse_market_ws_payload(
            json!({"event_type": "liquidity_changed", "market": "pm_condition_mock"}),
            received_at(),
            12,
        )
        .unwrap();

    assert_eq!(
        unknown.kind,
        ParsedPolymarketKind::Unknown {
            event_type: Some("liquidity_changed".to_string())
        }
    );
    assert!(unknown.quality_flags.unknown_schema);

    let mut missing = fixture("market_book.json");
    missing.as_object_mut().unwrap().remove("market");
    assert_eq!(
        parser
            .parse_market_ws_payload(missing, received_at(), 13)
            .unwrap_err(),
        ParserError::MissingRequiredField {
            field: "market".to_string()
        }
    );
}

#[test]
fn user_ws_parser_parses_order_fill_and_redacts_secrets_from_raw() {
    let parser = PolymarketParser::new("polymarket.ws.user.test");
    let order = parser
        .parse_user_ws_payload(fixture("user_order_update.json"), received_at(), 14)
        .unwrap();
    let fill = parser
        .parse_user_ws_payload(fixture("user_fill_update.json"), received_at(), 15)
        .unwrap();

    assert_eq!(order.kind, ParsedPolymarketKind::UserOrder);
    assert_eq!(fill.kind, ParsedPolymarketKind::UserFill);
    let order_update = parser
        .parse_user_ws_payload(
            json!({
                "event_type": "order_update",
                "id": "pm_order_update_mock_001",
                "market": "pm_condition_mock_lal_bos_moneyline",
                "asset_id": "pm_asset_yes_mock_001",
                "status": "CANCELLED"
            }),
            received_at(),
            16,
        )
        .unwrap();
    assert_eq!(order_update.kind, ParsedPolymarketKind::UserOrderUpdate);
    assert_eq!(order.raw.source_channel, SourceChannel::WsUser);
    assert_eq!(fill.raw.source_channel, SourceChannel::WsUser);
    let raw_text = serde_json::to_string(&order.raw.payload).unwrap();
    assert!(!raw_text.contains("redacted-secret"));
    assert!(!raw_text.contains("redacted-passphrase"));
    assert!(!raw_text.contains("redacted-signature"));
    assert!(raw_text.contains("<redacted>"));
}

#[test]
fn geoblock_parser_redacts_ip_and_state_machine_fails_closed_when_blocked() {
    let blocked = GeoblockStatus::parse(fixture("geoblock_blocked.json")).unwrap();
    assert!(blocked.blocked);
    assert_eq!(blocked.country.as_deref(), Some("US"));
    assert_eq!(blocked.ip.as_deref(), Some("<redacted-ip>"));

    let allowed = GeoblockStatus::parse(fixture("geoblock_allowed.json")).unwrap();
    assert!(!allowed.blocked);

    let machine = PolymarketStateMachine::new(30);
    let blocked_state = machine.geoblock_state(&blocked, SourceMode::RestGeoblock);
    assert_eq!(blocked_state.status, SourceStatus::Blocked);
    assert!(blocked_state.geoblocked);
    assert!(!blocked_state.live_execution_allowed);
    assert_eq!(blocked_state.block_reason.as_deref(), Some("geoblocked"));

    let ok_state = machine.geoblock_state(&allowed, SourceMode::RestGeoblock);
    assert_eq!(ok_state.status, SourceStatus::Ok);
    assert!(!ok_state.geoblocked);
    assert!(!ok_state.live_execution_allowed);

    assert!(GeoblockStatus::parse(json!({"country": "US"})).is_err());
}

#[test]
fn time_probe_parser_calculates_offsets_and_large_offset_warning() {
    let local = Utc.timestamp_opt(1_778_864_120, 0).unwrap();
    let probe = TimeProbe::parse_json(fixture("time_probe.json"), local).unwrap();
    assert_eq!(probe.server_unix_seconds, 1_778_864_123);
    assert_eq!(probe.offset_ms, 3_000);
    assert!(!probe.large_offset_warning);

    let large = TimeProbe::from_server_unix_seconds(1_778_864_200, local);
    assert_eq!(large.offset_ms, 80_000);
    assert!(large.large_offset_warning);

    let negative = TimeProbe::from_server_unix_seconds(1_778_864_119, local);
    assert_eq!(negative.offset_ms, -1_000);

    assert!(TimeProbe::parse_json(json!({"server_time": "not-a-time"}), local).is_err());
    assert!(TimeProbe::parse_json(json!({}), local).is_err());
}

#[test]
fn polymarket_payload_hash_and_raw_message_construction_are_deterministic() {
    let parser = PolymarketParser::new("polymarket.ws.market.test");
    let left = parser
        .parse_market_ws_payload(fixture("market_book.json"), received_at(), 33)
        .unwrap();
    let right = parser
        .parse_market_ws_payload(fixture("market_book.json"), received_at(), 33)
        .unwrap();

    assert_eq!(left.raw.payload_hash, right.raw.payload_hash);
    assert_eq!(left.raw.raw_id, right.raw.raw_id);
    assert_eq!(left.raw.provider, Provider::Polymarket);
    assert_eq!(left.raw.source_channel, SourceChannel::WsMarket);
}

#[test]
fn source_state_covers_market_stale_user_auth_missing_and_market_resolved() {
    let machine = PolymarketStateMachine::new(30);
    let last_seen = received_at();
    assert!(!machine.is_stale(Some(last_seen), last_seen + ChronoDuration::seconds(30)));
    assert!(machine.is_stale(Some(last_seen), last_seen + ChronoDuration::seconds(31)));

    let stale = machine.market_stale();
    assert_eq!(stale.source, "polymarket_market");
    assert_eq!(stale.status, SourceStatus::Stale);
    assert!(!stale.live_signal_allowed);

    let auth_missing = machine.user_auth_missing();
    assert_eq!(auth_missing.source, "polymarket_user");
    assert_eq!(auth_missing.status, SourceStatus::AuthMissing);
    assert!(!auth_missing.live_execution_allowed);

    let resolved = machine.market_resolved("pm_condition_mock_lal_bos_moneyline");
    assert_eq!(resolved.status, SourceStatus::MarketResolved);
    assert_eq!(resolved.block_reason.as_deref(), Some("market_resolved"));
}
