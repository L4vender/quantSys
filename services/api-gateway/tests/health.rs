use axum::body::Body;
use axum::http::{Request, StatusCode};
use quantsys_api_gateway::build_router;
use tower::ServiceExt;

#[tokio::test]
async fn health_endpoints_return_ok() {
    let app = build_router();

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
