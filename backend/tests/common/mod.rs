#![allow(dead_code)] // each integration-test crate uses a subset of these helpers

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use deckoala_server::{app, init_db, AppState};

pub const TEST_HOST: &str = "localhost:8080";

/// Fresh app + its pool, backed by a real migrated SQLite database in a
/// unique temp dir.
pub async fn test_app_with_db(name: &str, allow_signup: bool) -> (Router, sqlx::SqlitePool) {
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
        },
        std::path::Path::new("nonexistent-static"),
    )
    .await
    .expect("app build failed");
    (router, db)
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
    TestResponse {
        status,
        headers,
        json,
    }
}

/// Extract the session cookie ("name=value") from a response, if set.
pub fn session_cookie(response: &TestResponse) -> Option<String> {
    response
        .headers
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().to_owned())
}
