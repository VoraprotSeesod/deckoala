#![allow(dead_code)] // each integration-test crate uses a subset of these helpers

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use deckoala_server::{app, init_db, AppState};

pub const TEST_HOST: &str = "localhost:8080";

/// Fresh app + its pool, backed by a real migrated SQLite database in a
/// unique temp dir. `revision_min_secs` controls the snapshot throttle
/// (production value is 300; pass 0 to snapshot on every markdown change).
pub async fn test_app_full(
    name: &str,
    allow_signup: bool,
    revision_min_secs: i64,
) -> (Router, sqlx::SqlitePool) {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = init_db(&data_dir).await.expect("init_db failed");
    let router = app(
        AppState {
            db: db.clone(),
            allow_signup,
            allowed_origin: None,
            secure_cookie: false,
            revision_min_secs,
        },
        std::path::Path::new("nonexistent-static"),
    )
    .await
    .expect("app build failed");
    (router, db)
}

pub async fn test_app_with_db(name: &str, allow_signup: bool) -> (Router, sqlx::SqlitePool) {
    test_app_full(name, allow_signup, 300).await
}

pub async fn test_app_with(name: &str, allow_signup: bool) -> Router {
    test_app_with_db(name, allow_signup).await.0
}

pub async fn test_app(name: &str) -> Router {
    test_app_with(name, true).await
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub json: serde_json::Value,
    pub text: String,
}

/// One request against the app; returns status + headers + parsed JSON body.
pub async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> TestResponse {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, TEST_HOST);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    let request = builder
        .body(match body {
            Some(json) => Body::from(json.to_string()),
            None => Body::empty(),
        })
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    TestResponse {
        status,
        headers,
        json,
        text,
    }
}

/// Register a fresh user on the app and return their session cookie.
pub async fn signup(app: &Router, username: &str) -> String {
    let response = send(
        app,
        "POST",
        "/api/auth/register",
        Some(serde_json::json!({ "username": username, "password": "password123" })),
        None,
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::CREATED, "signup failed");
    session_cookie(&response).expect("signup must set a session cookie")
}

/// Extract the session cookie ("name=value") from a response, if set.
pub fn session_cookie(response: &TestResponse) -> Option<String> {
    response
        .headers
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().to_owned())
}
