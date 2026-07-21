mod common;

use axum::http::{header, StatusCode};
use common::{
    multipart_file, send, send_raw, signup, test_app, test_app_with_db, tiny_png, TestResponse,
};
use serde_json::json;

async fn make_deck(app: &axum::Router, cookie: &str) -> String {
    let created = send(
        app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Asset Deck" })),
        Some(cookie),
        None,
    )
    .await;
    created.json["id"].as_str().unwrap().to_owned()
}

async fn upload(
    app: &axum::Router,
    cookie: Option<&str>,
    deck_id: &str,
    filename: &str,
    declared_mime: &str,
    data: &[u8],
) -> TestResponse {
    let (content_type, body) = multipart_file(filename, declared_mime, data);
    send_raw(
        app,
        "POST",
        &format!("/api/decks/{deck_id}/assets"),
        &content_type,
        body,
        cookie,
    )
    .await
}

#[tokio::test]
async fn upload_requires_auth() {
    let app = test_app("assets-auth").await;
    let res = upload(&app, None, "some-deck", "x.png", "image/png", &tiny_png()).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn upload_stores_and_serves_a_png() {
    let (app, db) = test_app_with_db("assets-store", true).await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    let res = upload(
        &app,
        Some(&cookie),
        &id,
        "photo.png",
        "image/png",
        &tiny_png(),
    )
    .await;
    assert_eq!(res.status, StatusCode::CREATED);
    assert_eq!(res.json["mime"], "image/png");
    assert_eq!(res.json["originalName"], "photo.png");
    let url = res.json["url"].as_str().unwrap().to_owned();
    assert!(url.starts_with(&format!("/assets/{id}/")));
    assert!(url.ends_with(".png"));

    // Row exists.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE deck_id = ?1")
        .bind(&id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // The URL serves the bytes back, as image/png + nosniff.
    let served = send(&app, "GET", &url, None, Some(&cookie), None).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.headers[header::CONTENT_TYPE], "image/png");
    assert_eq!(served.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(
        served.bytes,
        tiny_png(),
        "served bytes must match the upload"
    );
    assert_eq!(served.bytes[0..4], [0x89, 0x50, 0x4E, 0x47]);
}

#[tokio::test]
async fn upload_to_foreign_deck_is_404() {
    let app = test_app("assets-foreign-up").await;
    let owner = signup(&app, "owner").await;
    let stranger = signup(&app, "stranger").await;
    let id = make_deck(&app, &owner).await;

    let res = upload(
        &app,
        Some(&stranger),
        &id,
        "x.png",
        "image/png",
        &tiny_png(),
    )
    .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn non_image_bytes_rejected_even_with_image_content_type() {
    let app = test_app("assets-sniff").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    // Declares image/png but the bytes are HTML — magic sniff must catch it.
    let res = upload(
        &app,
        Some(&cookie),
        &id,
        "evil.png",
        "image/png",
        b"<html><script>alert(1)</script></html>",
    )
    .await;
    assert_eq!(res.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn oversized_image_rejected() {
    let app = test_app("assets-size").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    // Valid PNG signature followed by >5 MB of padding.
    let mut big = tiny_png();
    big.resize(6 * 1024 * 1024, 0u8);
    let res = upload(&app, Some(&cookie), &id, "big.png", "image/png", &big).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn foreign_and_missing_serve_are_404() {
    let app = test_app("assets-serve-scope").await;
    let owner = signup(&app, "owner").await;
    let stranger = signup(&app, "stranger").await;
    let id = make_deck(&app, &owner).await;
    let uploaded = upload(&app, Some(&owner), &id, "p.png", "image/png", &tiny_png()).await;
    let url = uploaded.json["url"].as_str().unwrap().to_owned();

    // Owner sees it; stranger and anonymous do not.
    assert_eq!(
        send(&app, "GET", &url, None, Some(&owner), None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        send(&app, "GET", &url, None, Some(&stranger), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, "GET", &url, None, None, None).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Unknown filename under the owner's deck → 404.
    let missing = send(
        &app,
        "GET",
        &format!("/assets/{id}/deadbeef.png"),
        None,
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn path_traversal_filename_is_404() {
    let app = test_app("assets-traversal").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    // A traversal segment must never resolve (encoded to survive routing).
    let res = send(
        &app,
        "GET",
        &format!("/assets/{id}/%2e%2e"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn assets_migration_roundtrip_to_v4() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v4", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 4).await.expect("undo to version 4");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'assets'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 0);

    migrator.run(&db).await.expect("re-apply");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'assets'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 1);
}
