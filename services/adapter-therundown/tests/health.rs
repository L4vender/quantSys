use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn adapter_therundown_health_endpoints_return_ok() {
    let app = adapter_therundown::app::build_router();

    let live = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);

    let ready = app
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
}
