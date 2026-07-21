mod common;

use axum::http::{header, StatusCode};
use common::{send, signup, test_app, test_app_with_db};
use serde_json::json;

async fn create_deck(
    app: &axum::Router,
    cookie: &str,
    body: serde_json::Value,
) -> common::TestResponse {
    send(app, "POST", "/api/decks", Some(body), Some(cookie), None).await
}

#[tokio::test]
async fn deck_endpoints_require_auth() {
    let app = test_app("decks-auth").await;
    let cases = [
        ("GET", "/api/decks"),
        ("POST", "/api/decks"),
        ("GET", "/api/decks/some-id"),
        ("PATCH", "/api/decks/some-id"),
        ("DELETE", "/api/decks/some-id"),
        ("POST", "/api/decks/some-id/duplicate"),
        ("GET", "/api/decks/some-id/export"),
    ];
    for (method, uri) in cases {
        let body = matches!(method, "POST" | "PATCH").then(|| json!({}));
        let response = send(&app, method, uri, body, None, None).await;
        assert_eq!(
            response.status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must require auth"
        );
    }
}

#[tokio::test]
async fn create_defaults_and_lean_list() {
    let app = test_app("decks-create").await;
    let cookie = signup(&app, "owner").await;

    let created = create_deck(&app, &cookie, json!({})).await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.json["title"], "Untitled deck");
    assert_eq!(created.json["theme"], "deckoala");
    assert!(created.json["markdown"]
        .as_str()
        .unwrap()
        .contains("marp: true"));

    let list = send(&app, "GET", "/api/decks", None, Some(&cookie), None).await;
    assert_eq!(list.status, StatusCode::OK);
    let items = list.json.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        !items[0].as_object().unwrap().contains_key("markdown"),
        "list items must not carry markdown"
    );
}

#[tokio::test]
async fn get_own_deck_includes_markdown() {
    let app = test_app("decks-get").await;
    let cookie = signup(&app, "owner").await;
    let created = create_deck(&app, &cookie, json!({ "title": "Mine" })).await;
    let id = created.json["id"].as_str().unwrap();

    let fetched = send(
        &app,
        "GET",
        &format!("/api/decks/{id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(fetched.status, StatusCode::OK);
    assert_eq!(fetched.json["title"], "Mine");
    assert!(fetched.json["markdown"].is_string());
}

#[tokio::test]
async fn foreign_decks_are_indistinguishable_from_missing() {
    let app = test_app("decks-scope").await;
    let owner = signup(&app, "owner").await;
    let stranger = signup(&app, "stranger").await;
    let created = create_deck(&app, &owner, json!({ "title": "Private" })).await;
    let id = created.json["id"].as_str().unwrap();

    let attempts = [
        ("GET", format!("/api/decks/{id}"), None),
        (
            "PATCH",
            format!("/api/decks/{id}"),
            Some(json!({ "title": "hacked" })),
        ),
        ("DELETE", format!("/api/decks/{id}"), None),
        ("POST", format!("/api/decks/{id}/duplicate"), None),
        ("GET", format!("/api/decks/{id}/export"), None),
    ];
    for (method, uri, body) in attempts {
        let response = send(&app, method, &uri, body, Some(&stranger), None).await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "{method} {uri} must 404 for non-owners"
        );
    }

    // And the deck is untouched for its owner.
    let still_there = send(
        &app,
        "GET",
        &format!("/api/decks/{id}"),
        None,
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(still_there.status, StatusCode::OK);
    assert_eq!(still_there.json["title"], "Private");
}

#[tokio::test]
async fn rename_bumps_updated_at_and_validates() {
    let app = test_app("decks-rename").await;
    let cookie = signup(&app, "owner").await;
    let created = create_deck(&app, &cookie, json!({})).await;
    let id = created.json["id"].as_str().unwrap().to_owned();
    let original_updated = created.json["updatedAt"].as_str().unwrap().to_owned();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let renamed = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "title": "  My Deck  " })),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.json["title"], "My Deck", "title must be trimmed");
    assert_ne!(
        renamed.json["updatedAt"].as_str().unwrap(),
        original_updated
    );

    let empty = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "title": "   " })),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(empty.status, StatusCode::UNPROCESSABLE_ENTITY);

    let nothing = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({})),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(nothing.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn oversized_markdown_rejected() {
    let app = test_app("decks-size").await;
    let cookie = signup(&app, "owner").await;
    let big = "x".repeat(1_000_001);

    let created = create_deck(&app, &cookie, json!({ "markdown": big })).await;
    assert_eq!(created.status, StatusCode::UNPROCESSABLE_ENTITY);

    let ok = create_deck(&app, &cookie, json!({})).await;
    let id = ok.json["id"].as_str().unwrap();
    let big = "x".repeat(1_000_001);
    let patched = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "markdown": big })),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(patched.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn duplicate_copies_content() {
    let app = test_app("decks-dup").await;
    let cookie = signup(&app, "owner").await;
    let created = create_deck(
        &app,
        &cookie,
        json!({ "title": "Original", "markdown": "# One\n" }),
    )
    .await;
    let id = created.json["id"].as_str().unwrap();

    let copy = send(
        &app,
        "POST",
        &format!("/api/decks/{id}/duplicate"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(copy.status, StatusCode::CREATED);
    assert_eq!(copy.json["title"], "Original (copy)");
    assert_eq!(copy.json["markdown"], "# One\n");
    assert_ne!(copy.json["id"], created.json["id"]);
}

#[tokio::test]
async fn soft_delete_hides_but_preserves() {
    let (app, db) = test_app_with_db("decks-softdel", true).await;
    let cookie = signup(&app, "owner").await;
    let created = create_deck(&app, &cookie, json!({ "title": "Doomed" })).await;
    let id = created.json["id"].as_str().unwrap().to_owned();

    let deleted = send(
        &app,
        "DELETE",
        &format!("/api/decks/{id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    let fetched = send(
        &app,
        "GET",
        &format!("/api/decks/{id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(fetched.status, StatusCode::NOT_FOUND);

    let list = send(&app, "GET", "/api/decks", None, Some(&cookie), None).await;
    assert_eq!(list.json.as_array().unwrap().len(), 0);

    let deleted_at: Option<String> =
        sqlx::query_scalar("SELECT deleted_at FROM decks WHERE id = ?1")
            .bind(&id)
            .fetch_one(&db)
            .await
            .expect("row must still exist (soft delete)");
    assert!(deleted_at.is_some(), "deleted_at must be set");

    // The tombstone must be dead on EVERY route, including the write paths.
    let attempts = [
        (
            "PATCH",
            format!("/api/decks/{id}"),
            Some(json!({ "title": "zombie" })),
        ),
        ("POST", format!("/api/decks/{id}/duplicate"), None),
        ("GET", format!("/api/decks/{id}/export"), None),
        ("DELETE", format!("/api/decks/{id}"), None),
    ];
    for (method, uri, body) in attempts {
        let response = send(&app, method, &uri, body, Some(&cookie), None).await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "{method} {uri} must 404 on a soft-deleted deck"
        );
    }
}

#[tokio::test]
async fn export_downloads_markdown_with_safe_headers() {
    let app = test_app("decks-export").await;
    let cookie = signup(&app, "owner").await;
    let created = create_deck(
        &app,
        &cookie,
        json!({ "title": "สไลด์ของฉัน", "markdown": "# สวัสดี\n" }),
    )
    .await;
    let id = created.json["id"].as_str().unwrap();

    let exported = send(
        &app,
        "GET",
        &format!("/api/decks/{id}/export"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(exported.status, StatusCode::OK);
    assert_eq!(exported.text, "# สวัสดี\n");
    let content_type = exported.headers[header::CONTENT_TYPE].to_str().unwrap();
    assert!(content_type.starts_with("text/markdown"));
    let disposition = exported.headers[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap();
    assert!(disposition.starts_with("attachment"));
    assert!(disposition.contains("filename*=UTF-8''%E0%B8%AA"));
}

#[tokio::test]
async fn export_disposition_safe_for_quote_and_emoji_titles() {
    let app = test_app("decks-export-hostile").await;
    let cookie = signup(&app, "owner").await;
    let created = create_deck(
        &app,
        &cookie,
        json!({ "title": "my \"deck\" 🐨", "markdown": "# hi\n" }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.json["id"].as_str().unwrap();

    let exported = send(
        &app,
        "GET",
        &format!("/api/decks/{id}/export"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(exported.status, StatusCode::OK);
    let disposition = exported.headers[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap();
    assert!(
        disposition.contains("filename=\"my deck.md\""),
        "{disposition}"
    );
    assert!(!disposition.contains("\r") && !disposition.contains("\n"));
}

#[tokio::test]
async fn duplicate_respects_title_cap() {
    let app = test_app("decks-dup-cap").await;
    let cookie = signup(&app, "owner").await;
    let long_title = "t".repeat(200);
    let created = create_deck(&app, &cookie, json!({ "title": long_title })).await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.json["id"].as_str().unwrap();

    let copy = send(
        &app,
        "POST",
        &format!("/api/decks/{id}/duplicate"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(copy.status, StatusCode::CREATED);
    let copy_title = copy.json["title"].as_str().unwrap();
    assert!(
        copy_title.chars().count() <= 200,
        "cap must survive duplicate"
    );
    assert!(copy_title.ends_with(" (copy)"));
}

#[tokio::test]
async fn control_characters_in_title_rejected() {
    let app = test_app("decks-ctl").await;
    let cookie = signup(&app, "owner").await;

    let created = create_deck(&app, &cookie, json!({ "title": "bad\ntitle" })).await;
    assert_eq!(created.status, StatusCode::UNPROCESSABLE_ENTITY);

    let ok = create_deck(&app, &cookie, json!({})).await;
    let id = ok.json["id"].as_str().unwrap();
    let patched = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "title": "bell\u{7}" })),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(patched.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_with_markdown_stores_verbatim() {
    let app = test_app("decks-import").await;
    let cookie = signup(&app, "owner").await;
    let markdown = "---\nmarp: true\n---\n\n# Imported\n";
    let created = create_deck(
        &app,
        &cookie,
        json!({ "title": "Imported", "markdown": markdown }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.json["markdown"], markdown);
}

#[tokio::test]
async fn migrations_down_to_v2_and_back() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v2", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 2).await.expect("undo to version 2");
    let decks_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'decks'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(decks_table, 0, "down migration must drop decks");

    migrator.run(&db).await.expect("re-apply migrations");
    let decks_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'decks'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(decks_table, 1, "up migration must recreate decks");
}
