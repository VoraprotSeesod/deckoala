mod common;

use axum::http::{header, StatusCode};
use common::{send, test_app};

#[tokio::test]
async fn health_reports_ok_with_migrated_db() {
    let app = test_app("health").await;
    let response = send(&app, "GET", "/api/health", None, None, None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.json["status"], "ok");
    assert_eq!(
        response.json["db"], "ok",
        "migrations must have seeded the meta table"
    );
    assert!(response.json["chromium"].is_boolean());
    assert!(response.json["version"].is_string());
}

#[tokio::test]
async fn unknown_api_route_returns_json_404_not_spa() {
    let app = test_app("api404").await;
    let response = send(&app, "GET", "/api/definitely-not-a-route", None, None, None).await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    let content_type = response
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        content_type.starts_with("application/json"),
        "unknown /api/* must be JSON, got content-type: {content_type}"
    );
}

#[tokio::test]
async fn instance_reports_signup_and_user_state() {
    let app = test_app("instance").await;
    let before = send(&app, "GET", "/api/instance", None, None, None).await;
    assert_eq!(before.status, StatusCode::OK);
    assert_eq!(before.json["allowSignup"], true);
    assert_eq!(before.json["hasUsers"], false);

    send(
        &app,
        "POST",
        "/api/auth/register",
        Some(serde_json::json!({ "username": "koala", "password": "password123" })),
        None,
        None,
    )
    .await;
    let after = send(&app, "GET", "/api/instance", None, None, None).await;
    assert_eq!(after.json["hasUsers"], true);
}
