use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use deckoala_server::{app, init_db, AppState};

/// Fresh app backed by a real migrated SQLite database in a unique temp dir.
async fn test_app(name: &str) -> axum::Router {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-{name}", std::process::id()));
    let db = init_db(&data_dir).await.expect("init_db failed");
    app(AppState { db }, std::path::Path::new("nonexistent-static"))
}

#[tokio::test]
async fn health_reports_ok_with_migrated_db() {
    let app = test_app("health").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(
        json["db"], "ok",
        "migrations must have seeded the meta table"
    );
    assert!(json["chromium"].is_boolean());
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn unknown_api_route_returns_json_404_not_spa() {
    let app = test_app("api404").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/definitely-not-a-route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        content_type.starts_with("application/json"),
        "unknown /api/* must be JSON, got content-type: {content_type}"
    );
}
