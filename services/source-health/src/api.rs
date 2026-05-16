use crate::app::SourceHealthAppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use quantsys_domain::Provider;
use quantsys_storage::RawArchiveSearchQuery;
use serde::{Deserialize, Serialize};

pub fn build_router(state: SourceHealthAppState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/metrics", get(metrics))
        .route("/api/v1/source-health", get(source_health))
        .route("/api/v1/source-health/{source}", get(source_health_one))
        .route("/api/v1/raw/archive/{raw_id}", get(raw_by_id))
        .route("/api/v1/raw/by-ref", get(raw_by_ref))
        .route("/api/v1/raw/search", get(raw_search))
        .route("/api/v1/dlq", get(dlq))
        .route("/api/v1/rate-budgets", get(rate_budgets))
        .route("/api/v1/consumer-lag", get(consumer_lag))
        .with_state(state)
}

async fn live() -> Json<HealthResponse> {
    Json(HealthResponse::ok("live"))
}

async fn ready() -> Json<HealthResponse> {
    Json(HealthResponse::ok("ready"))
}

async fn metrics(State(state): State<SourceHealthAppState>) -> Response {
    let max_lag = state
        .consumer_lag
        .list()
        .into_iter()
        .map(|snapshot| snapshot.lag)
        .max()
        .unwrap_or(0);
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        format!(
            "source_health_ready 1\nsource_health_consumer_lag_max {}\nsource_health_dlq_total {}\n",
            max_lag,
            state.dlq.len()
        ),
    )
        .into_response()
}

async fn source_health(State(state): State<SourceHealthAppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "sources": state.source_states.list_latest() }))
}

async fn source_health_one(
    State(state): State<SourceHealthAppState>,
    Path(source): Path<String>,
) -> Response {
    match state.source_states.latest(&source) {
        Some(snapshot) => Json(snapshot).into_response(),
        None => api_error(
            StatusCode::NOT_FOUND,
            "source_not_found",
            "source not found",
        ),
    }
}

async fn rate_budgets(State(state): State<SourceHealthAppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "rate_budgets": state.rate_budgets.list() }))
}

async fn consumer_lag(State(state): State<SourceHealthAppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "consumer_lag": state.consumer_lag.list() }))
}

async fn dlq(State(state): State<SourceHealthAppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "dlq": state.dlq.list() }))
}

async fn raw_by_id(
    State(state): State<SourceHealthAppState>,
    Path(raw_id): Path<String>,
) -> Response {
    match state.raw_index.get(&raw_id) {
        Some(record) => Json(record).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "raw_not_found", "raw_id not found"),
    }
}

#[derive(Debug, Deserialize)]
struct ByRefQuery {
    raw_ref: String,
}

async fn raw_by_ref(
    State(state): State<SourceHealthAppState>,
    Query(query): Query<ByRefQuery>,
) -> Response {
    match state.raw_index.get_by_raw_ref(&query.raw_ref) {
        Some(record) => Json(record).into_response(),
        None => api_error(
            StatusCode::NOT_FOUND,
            "raw_ref_not_found",
            "raw_ref not found",
        ),
    }
}

#[derive(Debug, Deserialize)]
struct RawSearchParams {
    provider: Option<String>,
    topic: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

async fn raw_search(
    State(state): State<SourceHealthAppState>,
    Query(params): Query<RawSearchParams>,
) -> Json<serde_json::Value> {
    let provider = params
        .provider
        .as_deref()
        .and_then(|provider| match provider {
            "therundown" => Some(Provider::TheRundown),
            "polymarket" => Some(Provider::Polymarket),
            _ => None,
        });
    let records = state.raw_index.search(RawArchiveSearchQuery {
        provider,
        topic: params.topic,
        from: params.from,
        to: params.to,
        ..RawArchiveSearchQuery::default()
    });
    Json(serde_json::json!({ "raw": records }))
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    check: &'static str,
}

impl HealthResponse {
    fn ok(check: &'static str) -> Self {
        Self {
            status: "ok",
            check,
        }
    }
}

fn api_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": {
                "code": code,
                "message": message
            }
        })),
    )
        .into_response()
}
