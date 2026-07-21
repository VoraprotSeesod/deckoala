mod common;

use axum::http::StatusCode;
use common::{send, signup, test_app_full};
use serde_json::json;

/// App that snapshots on every markdown change (interval 0).
async fn eager_app(name: &str) -> (axum::Router, sqlx::SqlitePool) {
    test_app_full(name, true, 0).await
}

/// App with the production 300s throttle.
async fn throttled_app(name: &str) -> (axum::Router, sqlx::SqlitePool) {
    test_app_full(name, true, 300).await
}

async fn make_deck(app: &axum::Router, cookie: &str) -> String {
    let created = send(
        app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Rev Deck", "markdown": "# v0\n" })),
        Some(cookie),
        None,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    created.json["id"].as_str().unwrap().to_owned()
}

async fn patch_markdown(
    app: &axum::Router,
    cookie: &str,
    id: &str,
    markdown: &str,
) -> common::TestResponse {
    send(
        app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "markdown": markdown })),
        Some(cookie),
        None,
    )
    .await
}

async fn revision_list(app: &axum::Router, cookie: &str, id: &str) -> common::TestResponse {
    send(
        app,
        "GET",
        &format!("/api/decks/{id}/revisions"),
        None,
        Some(cookie),
        None,
    )
    .await
}

#[tokio::test]
async fn first_edit_snapshots_original_content() {
    let (app, _db) = throttled_app("rev-first").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    let patched = patch_markdown(&app, &cookie, &id, "# v1\n").await;
    assert_eq!(patched.status, StatusCode::OK);

    let list = revision_list(&app, &cookie, &id).await;
    let items = list.json.as_array().unwrap();
    assert_eq!(items.len(), 1, "first edit must snapshot the original");
    assert!(
        !items[0].as_object().unwrap().contains_key("markdown"),
        "list must be lean"
    );
    assert!(items[0]["sizeBytes"].as_i64().unwrap() > 0);

    // The snapshot holds the PRE-update content.
    let rev_id = items[0]["id"].as_str().unwrap();
    let full = send(
        &app,
        "GET",
        &format!("/api/decks/{id}/revisions/{rev_id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(full.status, StatusCode::OK);
    assert_eq!(full.json["markdown"], "# v0\n");
}

#[tokio::test]
async fn interval_suppresses_but_elapsed_interval_snapshots() {
    let (app, db) = throttled_app("rev-interval").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    patch_markdown(&app, &cookie, &id, "# v1\n").await;
    patch_markdown(&app, &cookie, &id, "# v2\n").await;
    let list = revision_list(&app, &cookie, &id).await;
    assert_eq!(
        list.json.as_array().unwrap().len(),
        1,
        "second edit within 300s must not snapshot"
    );

    // Age the newest revision beyond the interval, then edit again.
    sqlx::query(
        "UPDATE revisions SET created_at = '2020-01-01T00:00:00.000000Z' WHERE deck_id = ?1",
    )
    .bind(&id)
    .execute(&db)
    .await
    .unwrap();
    patch_markdown(&app, &cookie, &id, "# v3\n").await;
    let list = revision_list(&app, &cookie, &id).await;
    let items = list.json.as_array().unwrap();
    assert_eq!(items.len(), 2, "elapsed interval must snapshot again");
    // Newest first; the fresh snapshot holds v2 (pre-update content).
    let newest_id = items[0]["id"].as_str().unwrap();
    let newest = send(
        &app,
        "GET",
        &format!("/api/decks/{id}/revisions/{newest_id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(newest.json["markdown"], "# v2\n");
}

#[tokio::test]
async fn title_only_and_identical_markdown_do_not_snapshot() {
    let (app, _db) = eager_app("rev-noop").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    let renamed = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "title": "Renamed" })),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK);

    let same = patch_markdown(&app, &cookie, &id, "# v0\n").await;
    assert_eq!(same.status, StatusCode::OK);

    let list = revision_list(&app, &cookie, &id).await;
    assert_eq!(
        list.json.as_array().unwrap().len(),
        0,
        "neither title-only nor identical-markdown PATCH may snapshot"
    );
}

#[tokio::test]
async fn stale_base_forces_snapshot_despite_interval() {
    let (app, _db) = throttled_app("rev-stale").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    // First edit snapshots (no revision yet); grab the fresh updatedAt.
    let first = patch_markdown(&app, &cookie, &id, "# tabA-1\n").await;
    let fresh_updated = first.json["updatedAt"].as_str().unwrap().to_owned();
    assert_eq!(
        revision_list(&app, &cookie, &id)
            .await
            .json
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Same-tab follow-up with the fresh base: throttled, no snapshot.
    let second = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "markdown": "# tabA-2\n", "baseUpdatedAt": fresh_updated })),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(
        revision_list(&app, &cookie, &id)
            .await
            .json
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // A stale tab writes from an outdated baseline: the safety net must
    // fire even inside the throttle window, preserving tabA-2's content.
    let stale = send(
        &app,
        "PATCH",
        &format!("/api/decks/{id}"),
        Some(json!({ "markdown": "# tabB-stale\n", "baseUpdatedAt": fresh_updated })),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(stale.status, StatusCode::OK);
    let list = revision_list(&app, &cookie, &id).await;
    let items = list.json.as_array().unwrap();
    assert_eq!(items.len(), 2, "stale-base write must snapshot");
    let newest_id = items[0]["id"].as_str().unwrap();
    let newest = send(
        &app,
        "GET",
        &format!("/api/decks/{id}/revisions/{newest_id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(
        newest.json["markdown"], "# tabA-2\n",
        "clobbered content must be preserved in the snapshot"
    );
}

#[tokio::test]
async fn concurrent_patches_do_not_error_and_snapshot_once() {
    let (app, db) = throttled_app("rev-concurrent").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    let a = patch_markdown(&app, &cookie, &id, "# race-a\n");
    let b = patch_markdown(&app, &cookie, &id, "# race-b\n");
    let (ra, rb) = tokio::join!(a, b);
    assert_eq!(ra.status, StatusCode::OK, "concurrent PATCH must not 500");
    assert_eq!(rb.status, StatusCode::OK, "concurrent PATCH must not 500");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revisions WHERE deck_id = ?1")
        .bind(&id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "serialized transactions must snapshot exactly once"
    );
}

#[tokio::test]
async fn size_bytes_counts_bytes_not_chars() {
    let (app, _db) = eager_app("rev-thai-size").await;
    let cookie = signup(&app, "owner").await;
    let thai = "# สวัสดีชาวโลก ยินดีต้อนรับ\n";
    let created = send(
        &app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Thai", "markdown": thai })),
        Some(&cookie),
        None,
    )
    .await;
    let id = created.json["id"].as_str().unwrap().to_owned();
    patch_markdown(&app, &cookie, &id, "# changed\n").await; // snapshots the Thai content

    let list = revision_list(&app, &cookie, &id).await;
    let size = list.json[0]["sizeBytes"].as_i64().unwrap();
    let chars = thai.chars().count() as i64;
    let bytes = thai.len() as i64;
    assert_eq!(size, bytes, "sizeBytes must be UTF-8 bytes");
    assert!(size > chars, "Thai bytes must exceed char count");
}

#[tokio::test]
async fn restore_snapshots_current_and_swaps_content() {
    let (app, _db) = eager_app("rev-restore").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    patch_markdown(&app, &cookie, &id, "# v1\n").await; // snapshot: v0
    let list = revision_list(&app, &cookie, &id).await;
    let v0_rev = list.json[0]["id"].as_str().unwrap().to_owned();

    let restored = send(
        &app,
        "POST",
        &format!("/api/decks/{id}/revisions/{v0_rev}/restore"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK);
    assert_eq!(
        restored.json["markdown"], "# v0\n",
        "deck must hold restored content"
    );

    // Restore snapshotted the pre-restore content (v1).
    let list = revision_list(&app, &cookie, &id).await;
    let items = list.json.as_array().unwrap();
    assert_eq!(items.len(), 2);
    let newest_id = items[0]["id"].as_str().unwrap();
    let newest = send(
        &app,
        "GET",
        &format!("/api/decks/{id}/revisions/{newest_id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(newest.json["markdown"], "# v1\n");
}

#[tokio::test]
async fn revision_routes_are_owner_scoped_and_tombstone_aware() {
    let (app, _db) = eager_app("rev-scope").await;
    let owner = signup(&app, "owner").await;
    let stranger = signup(&app, "stranger").await;
    let id = make_deck(&app, &owner).await;
    patch_markdown(&app, &owner, &id, "# v1\n").await;
    let list = revision_list(&app, &owner, &id).await;
    let rev_id = list.json[0]["id"].as_str().unwrap().to_owned();

    let foreign_attempts = [
        ("GET", format!("/api/decks/{id}/revisions")),
        ("GET", format!("/api/decks/{id}/revisions/{rev_id}")),
        (
            "POST",
            format!("/api/decks/{id}/revisions/{rev_id}/restore"),
        ),
    ];
    for (method, uri) in &foreign_attempts {
        let response = send(&app, method, uri, None, Some(&stranger), None).await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "{method} {uri} must 404 for non-owners"
        );
    }

    // Soft-delete the deck: every revision route dies with it.
    let deleted = send(
        &app,
        "DELETE",
        &format!("/api/decks/{id}"),
        None,
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    for (method, uri) in &foreign_attempts {
        let response = send(&app, method, uri, None, Some(&owner), None).await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "{method} {uri} must 404 on a soft-deleted deck"
        );
    }
}

#[tokio::test]
async fn cross_deck_revision_id_is_404() {
    let (app, _db) = eager_app("rev-crossdeck").await;
    let cookie = signup(&app, "owner").await;
    let deck_a = make_deck(&app, &cookie).await;
    patch_markdown(&app, &cookie, &deck_a, "# a1\n").await;
    let rev_a = revision_list(&app, &cookie, &deck_a).await.json[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let deck_b = send(
        &app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Other" })),
        Some(&cookie),
        None,
    )
    .await
    .json["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // deck B + revision-of-deck-A must never resolve — read NOR restore.
    let mismatched_get = send(
        &app,
        "GET",
        &format!("/api/decks/{deck_b}/revisions/{rev_a}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(mismatched_get.status, StatusCode::NOT_FOUND);

    let mismatched_restore = send(
        &app,
        "POST",
        &format!("/api/decks/{deck_b}/revisions/{rev_a}/restore"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(
        mismatched_restore.status,
        StatusCode::NOT_FOUND,
        "restore must never pull a foreign deck's revision"
    );
}

#[tokio::test]
async fn retention_keeps_newest_fifty() {
    let (app, db) = eager_app("rev-retention").await;
    let cookie = signup(&app, "owner").await;
    let id = make_deck(&app, &cookie).await;

    for i in 1..=55 {
        let patched = patch_markdown(&app, &cookie, &id, &format!("# v{i}\n")).await;
        assert_eq!(patched.status, StatusCode::OK);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revisions WHERE deck_id = ?1")
        .bind(&id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 50, "retention must cap revisions at 50");

    // Newest snapshot is the pre-update content of the last patch (v54).
    let list = revision_list(&app, &cookie, &id).await;
    let newest_id = list.json[0]["id"].as_str().unwrap();
    let newest = send(
        &app,
        "GET",
        &format!("/api/decks/{id}/revisions/{newest_id}"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(newest.json["markdown"], "# v54\n");
}

#[tokio::test]
async fn migrations_down_to_v3_and_back() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v3", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 3).await.expect("undo to version 3");
    let revisions_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'revisions'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(revisions_table, 0);

    migrator.run(&db).await.expect("re-apply migrations");
    let revisions_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'revisions'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(revisions_table, 1);
}
