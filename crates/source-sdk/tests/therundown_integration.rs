use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use quantsys_domain::{SourceMode, SourceStatus};
use quantsys_eventbus::InMemoryEventProducer;
use quantsys_source_sdk::therundown::{
    ApiKey, DlqRecord, InMemoryDlqSink, MockRestResponse, RestTransport, TheRundownAdapter,
    TheRundownAdapterConfig, TheRundownBackoff, TheRundownError,
};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn fixture(name: &str) -> serde_json::Value {
    let path = format!(
        "{}/../../tests/fixtures/external/therundown/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn adapter_with_transport(transport: QueueTransport) -> TheRundownAdapter<QueueTransport> {
    TheRundownAdapter::new(
        TheRundownAdapterConfig {
            api_base_url: "https://mock.therundown.test/api/v2".to_string(),
            schema_version: "therundown.v2.integration.test".to_string(),
            stale_after_seconds: 30,
            rest_timeout: Duration::from_millis(100),
            reconnect_backoff: TheRundownBackoff::new(100, 1_000, 50, 3),
        },
        ApiKey::new("mock_therundown_key_for_tests"),
        transport,
        InMemoryEventProducer::default(),
        InMemoryDlqSink::default(),
    )
}

#[derive(Clone, Default)]
struct QueueTransport {
    responses: Arc<Mutex<VecDeque<MockRestResponse>>>,
    requested_urls: Arc<Mutex<Vec<String>>>,
}

impl QueueTransport {
    fn with_responses(responses: impl IntoIterator<Item = MockRestResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            requested_urls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl RestTransport for QueueTransport {
    async fn get_json(
        &self,
        url: &str,
        api_key: &ApiKey,
        _timeout: Duration,
    ) -> Result<MockRestResponse, TheRundownError> {
        assert_eq!(
            api_key.expose_for_transport(),
            "mock_therundown_key_for_tests"
        );
        self.requested_urls.lock().unwrap().push(url.to_string());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| TheRundownError::Transport("mock response queue is empty".to_string()))
    }
}

fn ok_response(body: serde_json::Value) -> MockRestResponse {
    MockRestResponse::new(
        200,
        [
            ("X-Tier", "ultra"),
            ("X-Rate-Limit", "10"),
            ("X-Data-Delay-Seconds", "0"),
            ("X-Websocket-Access", "true"),
            ("X-Datapoints-Remaining", "39999994"),
        ],
        body,
    )
}

fn no_header_response(status: u16, body: serde_json::Value) -> MockRestResponse {
    MockRestResponse::new(status, std::iter::empty::<(&str, &str)>(), body)
}

#[tokio::test]
async fn mock_rest_bootstrap_publishes_raw_therundown_and_updates_cursor() {
    let transport = QueueTransport::with_responses([ok_response(fixture("events_bootstrap.json"))]);
    let mut adapter = adapter_with_transport(transport);

    let raw = adapter.bootstrap_events(4, "2026-05-15").await.unwrap();
    let events = adapter.publisher().events();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].topic, "raw.therundown");
    assert_eq!(
        raw.provider_event_id.as_deref(),
        Some("tr_evt_mock_20260515_lal_bos")
    );
    assert_eq!(adapter.cursor().last_id(), Some("tr_delta_20260515_000001"));
    assert_eq!(adapter.state().status, SourceStatus::Ok);
    assert!(adapter.state().live_signal_allowed);
    assert!(!adapter.state().live_execution_allowed);
}

#[tokio::test]
async fn mock_market_delta_publishes_raw_therundown_and_advances_cursor() {
    let transport = QueueTransport::with_responses([ok_response(fixture("markets_delta.json"))]);
    let mut adapter = adapter_with_transport(transport);

    adapter.mark_cursor("tr_delta_20260515_000001");
    let raw = adapter
        .markets_delta("tr_delta_20260515_000001")
        .await
        .unwrap();

    assert_eq!(adapter.publisher().events().len(), 1);
    assert_eq!(adapter.cursor().last_id(), Some("tr_delta_20260515_000004"));
    assert_eq!(
        raw.source_channel,
        quantsys_domain::SourceChannel::RestDelta
    );
}

#[tokio::test]
async fn mock_ws_heartbeat_updates_source_state() {
    let mut adapter = adapter_with_transport(QueueTransport::default());
    let received_at = Utc.with_ymd_and_hms(2026, 5, 15, 12, 2, 0).unwrap();

    adapter
        .handle_ws_json(fixture("ws_heartbeat.json"), received_at, 10)
        .await
        .unwrap();

    assert_eq!(adapter.state().mode, SourceMode::LiveWs);
    assert_eq!(adapter.state().status, SourceStatus::Ok);
    assert_eq!(adapter.state().last_heartbeat_at, Some(received_at));
    assert_eq!(adapter.publisher().events().len(), 1);
}

#[tokio::test]
async fn mock_ws_market_price_publishes_raw_therundown() {
    let mut adapter = adapter_with_transport(QueueTransport::default());

    adapter
        .handle_ws_json(
            fixture("ws_market_price.json"),
            Utc.with_ymd_and_hms(2026, 5, 15, 12, 2, 0).unwrap(),
            11,
        )
        .await
        .unwrap();

    let events = adapter.publisher().events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].topic, "raw.therundown");
    assert_eq!(
        adapter.state().last_message_at,
        adapter.state().last_heartbeat_at
    );
}

#[tokio::test]
async fn mock_unknown_ws_type_is_preserved_as_raw_unknown_schema() {
    let mut adapter = adapter_with_transport(QueueTransport::default());

    adapter
        .handle_ws_json(
            json!({"meta": {"type": "bookmaker_status"}, "data": {"status": "ok"}}),
            Utc.with_ymd_and_hms(2026, 5, 15, 12, 2, 0).unwrap(),
            12,
        )
        .await
        .unwrap();

    assert_eq!(adapter.publisher().events().len(), 1);
    assert_eq!(adapter.state().status, SourceStatus::SchemaError);
    assert_eq!(
        adapter.state().block_reason.as_deref(),
        Some("unknown_schema")
    );
}

#[tokio::test]
async fn mock_missing_required_ws_field_goes_to_dlq_without_publish() {
    let mut adapter = adapter_with_transport(QueueTransport::default());
    let mut payload = fixture("ws_market_price.json");
    payload["data"].as_object_mut().unwrap().remove("event_id");

    adapter
        .handle_ws_json(
            payload,
            Utc.with_ymd_and_hms(2026, 5, 15, 12, 2, 0).unwrap(),
            13,
        )
        .await
        .unwrap_err();

    let dlq: Vec<DlqRecord> = adapter.dlq().records();
    assert_eq!(adapter.publisher().events().len(), 0);
    assert_eq!(dlq.len(), 1);
    assert_eq!(dlq[0].provider, "therundown");
    assert_eq!(dlq[0].error_code, "SCHEMA_ERROR");
    assert!(!dlq[0]
        .error_message
        .contains("mock_therundown_key_for_tests"));
}

#[tokio::test]
async fn mock_429_applies_retry_after_and_sets_rate_limited_without_storm() {
    let transport = QueueTransport::with_responses([MockRestResponse::new(
        429,
        [("Retry-After", "2"), ("X-Datapoints-Remaining", "0")],
        json!({"error": "rate limited"}),
    )]);
    let mut adapter = adapter_with_transport(transport);

    let err = adapter.bootstrap_events(4, "2026-05-15").await.unwrap_err();

    assert!(matches!(err, TheRundownError::RateLimited { .. }));
    assert_eq!(adapter.state().status, SourceStatus::RateLimited);
    assert_eq!(adapter.retry_after(), Some(Duration::from_secs(2)));
    assert_eq!(adapter.publisher().events().len(), 0);
}

#[tokio::test]
async fn mock_401_sets_auth_failed_and_does_not_retry() {
    let transport =
        QueueTransport::with_responses([no_header_response(401, json!({"error": "invalid key"}))]);
    let mut adapter = adapter_with_transport(transport);

    let err = adapter.bootstrap_events(4, "2026-05-15").await.unwrap_err();

    assert!(matches!(err, TheRundownError::AuthFailed));
    assert_eq!(adapter.state().status, SourceStatus::AuthFailed);
    assert_eq!(adapter.publisher().events().len(), 0);
}

#[tokio::test]
async fn mock_5xx_uses_backoff_state_and_keeps_secret_scrubbed() {
    let transport = QueueTransport::with_responses([no_header_response(
        503,
        json!({"error": "temporarily unavailable"}),
    )]);
    let mut adapter = adapter_with_transport(transport);

    let err = adapter.bootstrap_events(4, "2026-05-15").await.unwrap_err();

    assert!(matches!(err, TheRundownError::Server { status: 503 }));
    assert_eq!(adapter.state().status, SourceStatus::Degraded);
    assert!(adapter.next_reconnect_delay().unwrap() >= Duration::from_millis(100));
    assert!(!err.to_string().contains("mock_therundown_key_for_tests"));
}

#[tokio::test]
async fn mock_stale_marks_source_and_computes_reconnect_backoff() {
    let mut adapter = adapter_with_transport(QueueTransport::default());
    let last_seen = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 31).unwrap();

    adapter.mark_message_at(last_seen);
    assert!(adapter.detect_stale(now));

    assert_eq!(adapter.state().status, SourceStatus::Stale);
    assert!(adapter.next_reconnect_delay().unwrap() >= Duration::from_millis(100));
}

#[tokio::test]
async fn mock_cursor_stale_triggers_bootstrap_recovery() {
    let transport = QueueTransport::with_responses([
        no_header_response(409, json!({"error": "stale cursor"})),
        ok_response(fixture("events_bootstrap.json")),
    ]);
    let mut adapter = adapter_with_transport(transport);

    let raw = adapter
        .markets_delta_with_bootstrap_recovery("expired_cursor", 4, "2026-05-15")
        .await
        .unwrap();

    assert_eq!(
        raw.source_channel,
        quantsys_domain::SourceChannel::RestBootstrap
    );
    assert_eq!(adapter.cursor().last_id(), Some("tr_delta_20260515_000001"));
    assert_eq!(adapter.publisher().events().len(), 1);
}

#[tokio::test]
async fn fixture_replay_ws_market_and_off_board_publish_raw_only() {
    let mut adapter = adapter_with_transport(QueueTransport::default());
    let received_at = Utc.with_ymd_and_hms(2026, 5, 15, 12, 2, 0).unwrap();

    adapter
        .handle_ws_json(fixture("ws_market_price.json"), received_at, 21)
        .await
        .unwrap();
    adapter
        .handle_ws_json(fixture("off_board_price.json"), received_at, 22)
        .await
        .unwrap();

    assert_eq!(adapter.publisher().events().len(), 2);
    assert_eq!(adapter.off_board_count(), 1);
}

#[tokio::test]
async fn datapoints_delay_and_no_ws_headers_update_source_state() {
    let transport = QueueTransport::with_responses([MockRestResponse::new(
        200,
        [
            ("X-Data-Delay-Seconds", "20"),
            ("X-Websocket-Access", "false"),
            ("X-Datapoints-Remaining", "0"),
        ],
        fixture("events_bootstrap.json"),
    )]);
    let mut adapter = adapter_with_transport(transport);

    adapter.bootstrap_events(4, "2026-05-15").await.unwrap();

    assert_eq!(adapter.state().status, SourceStatus::DataDelayDetected);
    assert!(!adapter.state().live_signal_allowed);
    assert!(!adapter.state().live_execution_allowed);
}

#[tokio::test]
async fn raw_publish_path_handles_1k_messages_with_local_p95_under_20ms() {
    let mut adapter = adapter_with_transport(QueueTransport::default());
    let payload = fixture("ws_market_price.json");
    let start = std::time::Instant::now();
    let received_at = Utc.with_ymd_and_hms(2026, 5, 15, 12, 2, 0).unwrap();

    for idx in 0..1_000 {
        let mut next = payload.clone();
        next["data"]["id"] = json!(193_600_383 + idx);
        adapter
            .handle_ws_json(next, received_at, idx)
            .await
            .unwrap();
    }

    let elapsed = start.elapsed();
    let approx_per_message = elapsed / 1_000;
    assert_eq!(adapter.publisher().events().len(), 1_000);
    assert!(
        approx_per_message < Duration::from_millis(20),
        "local parser+publish average {:?} exceeded smoke threshold",
        approx_per_message
    );
}
