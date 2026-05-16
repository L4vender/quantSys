use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{TimeZone, Utc};
use quantsys_domain::{Provider, SourceMode, SourceState, SourceStatus};
use quantsys_storage::{
    ConsumerLagSnapshot, InMemoryConsumerLagStore, InMemoryDlqStore, InMemoryRateBudgetStore,
    InMemoryRawArchiveIndex, InMemorySourceStateStore, RateBudgetSnapshot,
};
use source_health::{build_router, SourceHealthAppState};
use tower::ServiceExt;

fn state(status: SourceStatus) -> SourceState {
    SourceState {
        source: "polymarket_geoblock".to_string(),
        mode: SourceMode::RestGeoblock,
        tier: None,
        data_delay_seconds: None,
        websocket_access: None,
        status,
        last_message_at: Some(Utc.with_ymd_and_hms(2026, 5, 16, 10, 0, 0).unwrap()),
        last_heartbeat_at: None,
        stale_after_seconds: 30,
        rate_limited: false,
        geoblocked: true,
        error: None,
        live_signal_allowed: false,
        live_execution_allowed: false,
        block_reason: Some("geoblocked".to_string()),
    }
}

#[tokio::test]
async fn source_health_api_exposes_sources_rate_budgets_lag_and_dlq_read_only() {
    let source_store = InMemorySourceStateStore::default();
    source_store.update(
        "polymarket_geoblock",
        Provider::Polymarket,
        state(SourceStatus::Geoblocked),
    );

    let rate_store = InMemoryRateBudgetStore::default();
    rate_store.update(RateBudgetSnapshot::new(
        Provider::TheRundown,
        "events_bootstrap",
        Some(100),
        Some(88),
        None,
        None,
        Utc.with_ymd_and_hms(2026, 5, 16, 10, 0, 0).unwrap(),
    ));

    let lag_store = InMemoryConsumerLagStore::default();
    lag_store.update(ConsumerLagSnapshot::new(
        "raw.polymarket.market",
        "raw-archive",
        0,
        50,
        250,
        Utc.with_ymd_and_hms(2026, 5, 16, 10, 0, 0).unwrap(),
    ));

    let app = build_router(SourceHealthAppState::new(
        source_store,
        rate_store,
        lag_store,
        InMemoryDlqStore::default(),
        InMemoryRawArchiveIndex::default(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/source-health/polymarket_geoblock")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/rate-budgets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rate_response.status(), StatusCode::OK);

    let lag_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/consumer-lag")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lag_response.status(), StatusCode::OK);
}
