use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use quantsys_telemetry::Metrics;
use serde::Serialize;

#[derive(Clone)]
pub struct AppState {
    metrics: Metrics,
}

pub fn build_router() -> Router {
    let metrics =
        Metrics::new("adapter-polymarket-market").expect("metrics skeleton should initialize");
    metrics.set_service_ready(true);
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/metrics", get(metrics_handler))
        .with_state(AppState { metrics })
}

async fn live() -> Json<HealthResponse> {
    Json(HealthResponse::ok("live"))
}

async fn ready() -> Json<HealthResponse> {
    Json(HealthResponse::ok("ready"))
}

async fn metrics_handler(State(state): State<AppState>) -> Response {
    match state.metrics.gather() {
        Ok(body) => ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("metrics encode error: {err}"),
        )
            .into_response(),
    }
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
