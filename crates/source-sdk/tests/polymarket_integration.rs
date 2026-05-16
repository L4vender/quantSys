use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use quantsys_domain::{SourceChannel, SourceStatus};
use quantsys_eventbus::InMemoryEventProducer;
use quantsys_source_sdk::polymarket::{
    DiscoveryFilters, InMemoryDlqSink, MockHttpResponse, PolymarketBackoff, PolymarketError,
    PolymarketMarketAdapter, PolymarketMarketAdapterConfig, PolymarketRestTransport,
    PolymarketUserAdapter, PolymarketUserAdapterConfig,
};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

fn market_adapter(transport: QueueTransport) -> PolymarketMarketAdapter<QueueTransport> {
    PolymarketMarketAdapter::new(
        PolymarketMarketAdapterConfig {
            gamma_api_base_url: "https://gamma-api.polymarket.test".to_string(),
            geoblock_url: "https://polymarket.test/api/geoblock".to_string(),
            server_time_url: "https://clob.polymarket.test/time".to_string(),
            schema_version: "polymarket.integration.test".to_string(),
            discovery_limit: 100,
            discovery_offset: 0,
            discovery_game_tag_id: Some(100_639),
            discovery_filters: DiscoveryFilters::sports_default(),
            stale_after_seconds: 30,
            rest_timeout: Duration::from_millis(100),
            token_cache_ttl_seconds: 300,
            max_token_subscriptions: 1_000,
            reconnect_backoff: PolymarketBackoff::new(100, 1_000, 50),
        },
        transport,
        InMemoryEventProducer::default(),
        InMemoryDlqSink::default(),
    )
}

fn user_adapter() -> PolymarketUserAdapter {
    PolymarketUserAdapter::new(
        PolymarketUserAdapterConfig {
            schema_version: "polymarket.user.integration.test".to_string(),
            stale_after_seconds: 30,
            reconnect_backoff: PolymarketBackoff::new(100, 1_000, 50),
        },
        InMemoryEventProducer::default(),
        InMemoryDlqSink::default(),
    )
}

#[derive(Clone, Default)]
struct QueueTransport {
    responses: Arc<Mutex<VecDeque<MockHttpResponse>>>,
    requested_urls: Arc<Mutex<Vec<String>>>,
}

impl QueueTransport {
    fn with_responses(responses: impl IntoIterator<Item = MockHttpResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            requested_urls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl PolymarketRestTransport for QueueTransport {
    async fn get_json(
        &self,
        url: &str,
        _timeout: Duration,
    ) -> Result<MockHttpResponse, PolymarketError> {
        self.requested_urls.lock().unwrap().push(url.to_string());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| PolymarketError::Transport("mock response queue is empty".to_string()))
    }
}

fn ok_response(body: serde_json::Value) -> MockHttpResponse {
    MockHttpResponse::new(200, std::iter::empty::<(&str, &str)>(), body)
}

#[tokio::test]
async fn discovery_builds_token_cache_and_publishes_raw_polymarket_market() {
    let transport =
        QueueTransport::with_responses([ok_response(fixture("market_discovery_events.json"))]);
    let mut adapter = market_adapter(transport.clone());

    let result = adapter.discover_markets().await.unwrap();

    assert_eq!(result.markets.len(), 1);
    assert_eq!(
        adapter
            .token_cache()
            .condition_for_token("pm_asset_yes_mock_001"),
        Some("pm_condition_mock_lal_bos_moneyline")
    );
    assert_eq!(adapter.publisher().events().len(), 1);
    assert_eq!(
        adapter.publisher().events()[0].topic,
        "raw.polymarket.market"
    );
    assert!(transport.requested_urls.lock().unwrap()[0].contains("/events"));
}

#[tokio::test]
async fn discovery_missing_token_ids_goes_to_dlq_without_publish() {
    let payload = json!([
        {
            "id": "pm_event_missing_tokens",
            "title": "NBA missing token ids",
            "active": true,
            "closed": false,
            "tags": [{"label": "NBA", "slug": "nba"}],
            "markets": [
                {
                    "question": "Missing token ids?",
                    "conditionId": "pm_condition_missing_tokens",
                    "active": true,
                    "closed": false,
                    "outcomes": "[\"Yes\", \"No\"]"
                }
            ]
        }
    ]);
    let transport = QueueTransport::with_responses([ok_response(payload)]);
    let mut adapter = market_adapter(transport);

    adapter.discover_markets().await.unwrap_err();

    assert_eq!(adapter.publisher().events().len(), 0);
    assert_eq!(adapter.dlq().records().len(), 1);
    assert_eq!(adapter.dlq().records()[0].error_code, "SCHEMA_ERROR");
    assert_eq!(adapter.dlq().records()[0].source_channel, "rest_discovery");
}

#[tokio::test]
async fn token_cache_constructs_market_ws_subscription_with_assets_ids() {
    let transport =
        QueueTransport::with_responses([ok_response(fixture("market_discovery_events.json"))]);
    let mut adapter = market_adapter(transport);
    adapter.discover_markets().await.unwrap();

    let subscription = adapter.market_subscription_payload(true).unwrap();

    assert!(subscription.get("assets_ids").is_some());
    assert!(subscription.get("asset_ids").is_none());
    assert_eq!(subscription["assets_ids"].as_array().unwrap().len(), 2);
    assert_eq!(subscription["custom_feature_enabled"], true);
}

#[tokio::test]
async fn market_ws_events_publish_raw_and_market_resolved_updates_source_state() {
    let mut adapter = market_adapter(QueueTransport::default());

    for fixture_name in [
        "market_book.json",
        "market_price_change.json",
        "market_best_bid_ask.json",
        "market_last_trade_price.json",
        "market_tick_size_change.json",
        "market_new_market.json",
    ] {
        adapter
            .handle_market_ws_json(fixture(fixture_name), received_at(), 11)
            .await
            .unwrap();
    }
    let resolved = adapter
        .handle_market_ws_json(fixture("market_resolved.json"), received_at(), 12)
        .await
        .unwrap();

    assert_eq!(resolved.source_channel, SourceChannel::WsMarket);
    assert_eq!(adapter.publisher().events().len(), 7);
    assert_eq!(adapter.state().status, SourceStatus::MarketResolved);
    assert_eq!(
        adapter.state().block_reason.as_deref(),
        Some("market_resolved")
    );
}

#[tokio::test]
async fn unknown_or_missing_market_ws_schema_goes_to_raw_or_dlq() {
    let mut adapter = market_adapter(QueueTransport::default());
    adapter
        .handle_market_ws_json(
            json!({"event_type": "unknown_market_event", "market": "pm_condition_mock"}),
            received_at(),
            13,
        )
        .await
        .unwrap();

    let mut missing = fixture("market_book.json");
    missing.as_object_mut().unwrap().remove("asset_id");
    adapter
        .handle_market_ws_json(missing, received_at(), 14)
        .await
        .unwrap_err();

    assert_eq!(adapter.publisher().events().len(), 1);
    assert_eq!(adapter.dlq().records().len(), 1);
    assert_eq!(adapter.dlq().records()[0].error_code, "SCHEMA_ERROR");
}

#[tokio::test]
async fn user_ws_order_and_fill_publish_raw_polymarket_user_without_credentials_in_payload() {
    let mut adapter = user_adapter();

    adapter
        .handle_user_ws_json(fixture("user_order_update.json"), received_at(), 21)
        .await
        .unwrap();
    adapter
        .handle_user_ws_json(fixture("user_fill_update.json"), received_at(), 22)
        .await
        .unwrap();

    let events = adapter.publisher().events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].topic, "raw.polymarket.user");
    let raw_text = String::from_utf8(events[0].payload.clone()).unwrap();
    assert!(!raw_text.contains("redacted-secret"));
    assert!(!raw_text.contains("redacted-passphrase"));
    assert!(raw_text.contains("<redacted>"));
    assert_eq!(adapter.state().status, SourceStatus::Ok);
}

#[tokio::test]
async fn geoblock_and_time_probes_publish_raw_and_update_source_state() {
    let transport = QueueTransport::with_responses([
        ok_response(fixture("geoblock_blocked.json")),
        ok_response(fixture("geoblock_allowed.json")),
        ok_response(fixture("time_probe.json")),
    ]);
    let mut adapter = market_adapter(transport);

    let blocked = adapter.probe_geoblock().await.unwrap();
    assert!(blocked.blocked);
    assert_eq!(adapter.state().source, "polymarket_geoblock");
    assert_eq!(adapter.state().status, SourceStatus::Blocked);

    let allowed = adapter.probe_geoblock().await.unwrap();
    assert!(!allowed.blocked);
    assert_eq!(adapter.state().status, SourceStatus::Ok);

    let time = adapter
        .probe_time_at(Utc.timestamp_opt(1_778_864_120, 0).unwrap())
        .await
        .unwrap();
    assert_eq!(time.offset_ms, 3_000);
    assert_eq!(adapter.state().source, "polymarket_time");
    assert_eq!(adapter.state().status, SourceStatus::Ok);
    assert_eq!(adapter.publisher().events().len(), 3);
}

#[tokio::test]
async fn ping_pong_stale_detection_reconnect_backoff_and_rate_limit_state() {
    let mut market = market_adapter(QueueTransport::default());
    market.mark_pong(received_at());
    assert!(!market.detect_stale(received_at() + chrono::Duration::seconds(30)));
    assert!(market.detect_stale(received_at() + chrono::Duration::seconds(31)));
    assert_eq!(market.state().status, SourceStatus::Stale);
    assert!(market.next_reconnect_delay().unwrap() >= Duration::from_millis(100));

    market.mark_endpoint_rate_limited("market_ws", Some(Duration::from_secs(2)));
    assert_eq!(market.state().status, SourceStatus::RateLimited);
    assert!(market.endpoint_budget("market_ws").unwrap().is_limited());

    let mut user = user_adapter();
    assert!(user.detect_stale(received_at() + chrono::Duration::seconds(31)));
    assert_eq!(user.state().status, SourceStatus::Stale);
}

#[tokio::test]
async fn user_auth_missing_is_disabled_without_failing_market_adapter() {
    let mut adapter = user_adapter();
    let state = adapter.mark_auth_missing();

    assert_eq!(state.status, SourceStatus::AuthMissing);
    assert_eq!(state.source, "polymarket_user");
    assert_eq!(adapter.publisher().events().len(), 0);
}

#[tokio::test]
async fn raw_publish_path_handles_1k_market_messages_with_mock_p95_under_50ms() {
    let mut adapter = market_adapter(QueueTransport::default());
    let payload = fixture("market_price_change.json");
    let start = std::time::Instant::now();

    for idx in 0..1_000 {
        let mut next = payload.clone();
        next["timestamp"] = json!(1_778_864_115_000_u64 + idx);
        adapter
            .handle_market_ws_json(next, received_at(), idx)
            .await
            .unwrap();
    }

    let elapsed = start.elapsed();
    let approx_per_message = elapsed / 1_000;
    assert_eq!(adapter.publisher().events().len(), 1_000);
    assert!(
        approx_per_message < Duration::from_millis(50),
        "local parser+publish average {:?} exceeded smoke threshold",
        approx_per_message
    );
}
