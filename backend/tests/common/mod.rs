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
            data_dir: data_dir.clone(),
            print_secret: [7u8; 32],
            local_addr: "127.0.0.1:8080".to_owned(),
            export_sem: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
            share_export_sem: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
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
    pub bytes: Vec<u8>,
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
        bytes: bytes.to_vec(),
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

/// Send a raw body with an explicit Content-Type (e.g. multipart uploads).
pub async fn send_raw(
    app: &Router,
    method: &str,
    uri: &str,
    content_type: &str,
    body: Vec<u8>,
    cookie: Option<&str>,
) -> TestResponse {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, TEST_HOST)
        .header(header::CONTENT_TYPE, content_type);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let request = builder.body(Body::from(body)).unwrap();
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
        bytes: bytes.to_vec(),
    }
}

/// Build a single-file `multipart/form-data` body (field name `file`).
/// Returns (content_type_header, body_bytes).
pub fn multipart_file(filename: &str, declared_mime: &str, data: &[u8]) -> (String, Vec<u8>) {
    let boundary = "deckoalatestboundary1234";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {declared_mime}\r\n\r\n").as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// Multipart body with text fields + one `file` field (e.g. font upload).
/// Returns (content_type_header, body_bytes).
pub fn multipart_fields(
    fields: &[(&str, &str)],
    file: Option<(&str, &str, &[u8])>,
) -> (String, Vec<u8>) {
    let boundary = "deckoalafontboundary9876";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some((filename, mime, data)) = file {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// Minimal bytes that pass the WOFF2 magic-byte sniff (not a real font).
pub fn fake_woff2() -> Vec<u8> {
    let mut v = b"wOF2".to_vec();
    v.extend(std::iter::repeat_n(0u8, 200));
    v
}

/// A minimal but valid 1x1 PNG (real signature + IHDR + IDAT + IEND).
pub fn tiny_png() -> Vec<u8> {
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    PNG.to_vec()
}

/// Extract the session cookie ("name=value") from a response, if set.
pub fn session_cookie(response: &TestResponse) -> Option<String> {
    response
        .headers
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().to_owned())
}
