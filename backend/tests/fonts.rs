mod common;

use axum::http::{header, StatusCode};
use common::{fake_woff2, multipart_fields, send, send_raw, signup, test_app, test_app_with_db};

async fn upload_font(
    app: &axum::Router,
    cookie: Option<&str>,
    family: &str,
    weight: &str,
    style: &str,
    data: &[u8],
) -> common::TestResponse {
    let (ct, body) = multipart_fields(
        &[("family", family), ("weight", weight), ("style", style)],
        Some(("font.woff2", "font/woff2", data)),
    );
    send_raw(app, "POST", "/api/fonts", &ct, body, cookie).await
}

#[tokio::test]
async fn install_is_admin_only() {
    let app = test_app("fonts-admin").await;
    let admin = signup(&app, "admin").await; // first user = admin
    let user = signup(&app, "user").await; // second = not admin

    // Non-admin upload → 403.
    let denied = upload_font(&app, Some(&user), "Sarabun", "400", "normal", &fake_woff2()).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);

    // Non-admin google + delete → 403.
    let g = send(
        &app,
        "POST",
        "/api/fonts/google",
        Some(serde_json::json!({ "family": "Roboto" })),
        Some(&user),
        Some("http://localhost:8080"),
    )
    .await;
    assert_eq!(g.status, StatusCode::FORBIDDEN);
    let d = send(
        &app,
        "DELETE",
        "/api/fonts/x",
        None,
        Some(&user),
        Some("http://localhost:8080"),
    )
    .await;
    assert_eq!(d.status, StatusCode::FORBIDDEN);

    // Unauthenticated → 401.
    let anon = upload_font(&app, None, "Sarabun", "400", "normal", &fake_woff2()).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    // Admin upload → 201.
    let ok = upload_font(
        &app,
        Some(&admin),
        "Sarabun",
        "400",
        "normal",
        &fake_woff2(),
    )
    .await;
    assert_eq!(ok.status, StatusCode::CREATED);
}

#[tokio::test]
async fn upload_validates_bytes_family_weight_and_dedups() {
    let app = test_app("fonts-validate").await;
    let admin = signup(&app, "admin").await;

    // Non-font bytes → 415.
    let bad = upload_font(
        &app,
        Some(&admin),
        "Sarabun",
        "400",
        "normal",
        b"<html>not a font",
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // Bad family (CSS-injection chars) → 422.
    let hostile = upload_font(
        &app,
        Some(&admin),
        "evil'; }",
        "400",
        "normal",
        &fake_woff2(),
    )
    .await;
    assert_eq!(hostile.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Bad weight → 422.
    let weight = upload_font(
        &app,
        Some(&admin),
        "Sarabun",
        "401",
        "normal",
        &fake_woff2(),
    )
    .await;
    assert_eq!(weight.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Valid → 201; identical variant again → 409.
    let ok = upload_font(
        &app,
        Some(&admin),
        "Sarabun",
        "400",
        "normal",
        &fake_woff2(),
    )
    .await;
    assert_eq!(ok.status, StatusCode::CREATED);
    let dup = upload_font(
        &app,
        Some(&admin),
        "Sarabun",
        "400",
        "normal",
        &fake_woff2(),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn fonts_css_and_serve_and_delete() {
    let (app, db) = test_app_with_db("fonts-css", true).await;
    let admin = signup(&app, "admin").await;
    let created = upload_font(
        &app,
        Some(&admin),
        "Sarabun",
        "400",
        "normal",
        &fake_woff2(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);

    // fonts.css (public) carries the @font-face + no-cache.
    let css = send(&app, "GET", "/api/fonts.css", None, None, None).await;
    assert_eq!(css.status, StatusCode::OK);
    assert!(css.headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/css"));
    assert_eq!(css.headers[header::CACHE_CONTROL], "no-cache");
    assert!(css.text.contains("@font-face"));
    assert!(css.text.contains("font-family: 'Sarabun'"));
    assert!(css.text.contains("/fonts/"));

    // The stored filename → serve it (public, session-less).
    let filename: String = sqlx::query_scalar("SELECT filename FROM fonts LIMIT 1")
        .fetch_one(&db)
        .await
        .unwrap();
    let served = send(&app, "GET", &format!("/fonts/{filename}"), None, None, None).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(served.headers[header::CONTENT_TYPE], "font/woff2");

    // Unknown + traversal → 404.
    assert_eq!(
        send(&app, "GET", "/fonts/nope.woff2", None, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, "GET", "/fonts/%2e%2e", None, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Delete (admin) → 204, gone from list, file removed.
    let id: String = sqlx::query_scalar("SELECT id FROM fonts LIMIT 1")
        .fetch_one(&db)
        .await
        .unwrap();
    let del = send(
        &app,
        "DELETE",
        &format!("/api/fonts/{id}"),
        None,
        Some(&admin),
        Some("http://localhost:8080"),
    )
    .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fonts")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        send(&app, "GET", &format!("/fonts/{filename}"), None, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn fonts_migration_roundtrip_to_v5() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v5", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 5).await.expect("undo to version 5");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'fonts'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 0);

    migrator.run(&db).await.expect("re-apply");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'fonts'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 1);
}
