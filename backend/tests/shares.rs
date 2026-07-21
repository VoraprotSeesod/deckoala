mod common;

use axum::http::{header, StatusCode};
use common::{
    multipart_file, send, send_raw, signup, test_app, test_app_full, tiny_png, TestResponse,
};
use serde_json::json;

const ORIGIN: &str = "http://localhost:8080";

async fn create_deck(app: &axum::Router, cookie: &str, markdown: &str) -> String {
    let r = send(
        app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Deck", "markdown": markdown })),
        Some(cookie),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "deck create failed");
    r.json["id"].as_str().unwrap().to_owned()
}

async fn mint(
    app: &axum::Router,
    cookie: &str,
    deck_id: &str,
    permission: &str,
    expires_at: Option<&str>,
) -> TestResponse {
    let mut body = json!({ "permission": permission });
    if let Some(e) = expires_at {
        body["expiresAt"] = json!(e);
    }
    send(
        app,
        "POST",
        &format!("/api/decks/{deck_id}/shares"),
        Some(body),
        Some(cookie),
        Some(ORIGIN),
    )
    .await
}

#[tokio::test]
async fn owner_only_management_and_mint_list_revoke() {
    let app = test_app("shares-manage").await;
    let owner = signup(&app, "owner").await;
    let other = signup(&app, "other").await;
    let deck = create_deck(&app, &owner, "# Hi").await;

    // A non-owner can neither mint, list, nor revoke on someone else's deck.
    assert_eq!(
        mint(&app, &other, &deck, "view", None).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/decks/{deck}/shares"),
            None,
            Some(&other),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // Owner mints a view + an edit link.
    let v = mint(&app, &owner, &deck, "view", None).await;
    assert_eq!(v.status, StatusCode::CREATED);
    assert_eq!(v.json["permission"], "view");
    assert_eq!(v.json["status"], "active");
    let vtoken = v.json["token"].as_str().unwrap().to_owned();
    let vid = v.json["id"].as_str().unwrap().to_owned();
    assert!(v.json["url"].as_str().unwrap().contains(&vtoken));
    assert_eq!(
        mint(&app, &owner, &deck, "edit", None).await.status,
        StatusCode::CREATED
    );

    // Invalid permission → 422.
    assert_eq!(
        mint(&app, &owner, &deck, "admin", None).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // List shows both links (owner only).
    let list = send(
        &app,
        "GET",
        &format!("/api/decks/{deck}/shares"),
        None,
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(list.json.as_array().unwrap().len(), 2);

    // Revoke the view link → 204, idempotent, and the token now 404s.
    let del = send(
        &app,
        "DELETE",
        &format!("/api/decks/{deck}/shares/{vid}"),
        None,
        Some(&owner),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/decks/{deck}/shares/{vid}"),
            None,
            Some(&owner),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/decks/{deck}/shares/nope"),
            None,
            Some(&owner),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, "GET", &format!("/api/s/{vtoken}"), None, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn cross_tenant_revoke_is_blocked() {
    let app = test_app("shares-xtenant").await;
    let a = signup(&app, "alice").await;
    let b = signup(&app, "bob").await;
    let deck_a = create_deck(&app, &a, "# A").await;
    let deck_b = create_deck(&app, &b, "# B").await;
    let share_b = mint(&app, &b, &deck_b, "view", None).await;
    let share_b_id = share_b.json["id"].as_str().unwrap().to_owned();

    // Alice owns deck_a but not share_b: revoking it via her own deck scope 404s
    // (double-scoped id AND deck_id), and via deck_b she fails the owner check.
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/decks/{deck_a}/shares/{share_b_id}"),
            None,
            Some(&a),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/decks/{deck_b}/shares/{share_b_id}"),
            None,
            Some(&a),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    // Bob's link is still active.
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/s/{}", share_b.json["token"].as_str().unwrap()),
            None,
            None,
            None
        )
        .await
        .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn view_reads_edit_writes_and_snapshots() {
    // revision_min_secs = 0 so the first markdown change snapshots immediately.
    let (app, _db) = test_app_full("shares-rw", true, 0).await;
    let owner = signup(&app, "owner").await;
    let deck = create_deck(&app, &owner, "# original").await;
    let vtoken = mint(&app, &owner, &deck, "view", None).await.json["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let etoken = mint(&app, &owner, &deck, "edit", None).await.json["token"]
        .as_str()
        .unwrap()
        .to_owned();

    // View GET returns the full deck shape (id + updatedAt drive the editor).
    let got = send(&app, "GET", &format!("/api/s/{vtoken}"), None, None, None).await;
    assert_eq!(got.status, StatusCode::OK);
    assert_eq!(got.json["id"], deck);
    assert_eq!(got.json["permission"], "view");
    assert!(got.json["markdown"].as_str().unwrap().contains("original"));
    assert!(got.json["updatedAt"].is_string());

    // View token cannot write.
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/s/{vtoken}"),
            Some(json!({ "markdown": "# hacked" })),
            None,
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    // Edit token writes, and the pre-edit content is snapshotted as a revision.
    let patched = send(
        &app,
        "PATCH",
        &format!("/api/s/{etoken}"),
        Some(json!({ "markdown": "# edited via share" })),
        None,
        Some(ORIGIN),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK);
    assert!(patched.json["markdown"]
        .as_str()
        .unwrap()
        .contains("edited via share"));

    let revs = send(
        &app,
        "GET",
        &format!("/api/s/{etoken}/revisions"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(revs.status, StatusCode::OK);
    assert_eq!(revs.json.as_array().unwrap().len(), 1);

    // Markdown export via token returns the current content.
    let md = send(
        &app,
        "GET",
        &format!("/api/s/{etoken}/export"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(md.status, StatusCode::OK);
    assert!(md.headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/markdown"));
    assert!(md.text.contains("edited via share"));
}

#[tokio::test]
async fn token_authorizes_only_its_own_deck() {
    let (app, _db) = test_app_full("shares-isolation", true, 0).await;
    let owner = signup(&app, "owner").await;
    let deck_a = create_deck(&app, &owner, "# A").await;
    let deck_b = create_deck(&app, &owner, "# B one").await;

    // Produce a revision on deck B (owner edit, min_secs=0 snapshots B one).
    send(
        &app,
        "PATCH",
        &format!("/api/decks/{deck_b}"),
        Some(json!({ "markdown": "# B two" })),
        Some(&owner),
        Some(ORIGIN),
    )
    .await;
    let rev_b = send(
        &app,
        "GET",
        &format!("/api/decks/{deck_b}/revisions"),
        None,
        Some(&owner),
        None,
    )
    .await;
    let rev_b_id = rev_b.json[0]["id"].as_str().unwrap().to_owned();

    // An edit token for deck A must not reach deck B's revision (cross-deck 404).
    let etoken_a = mint(&app, &owner, &deck_a, "edit", None).await.json["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/s/{etoken_a}/revisions/{rev_b_id}"),
            None,
            None,
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/s/{etoken_a}/revisions/{rev_b_id}/restore"),
            None,
            None,
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn expiry_is_canonicalized_and_fails_closed() {
    let app = test_app("shares-expiry").await;
    let owner = signup(&app, "owner").await;
    let deck = create_deck(&app, &owner, "# Hi").await;

    // Naive (offsetless) expiry is rejected — the control can't silently fail open.
    assert_eq!(
        mint(&app, &owner, &deck, "view", Some("2027-01-01T00:00:00"))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // A past instant expressed with an offset (2000-01-01T07:00+07:00 = 2000…Z)
    // is stored as its true UTC time → the link is already expired → 404.
    let past = mint(
        &app,
        &owner,
        &deck,
        "view",
        Some("2000-01-01T07:00:00+07:00"),
    )
    .await;
    assert_eq!(past.status, StatusCode::CREATED);
    assert_eq!(past.json["status"], "expired");
    let ptoken = past.json["token"].as_str().unwrap().to_owned();
    assert_eq!(
        send(&app, "GET", &format!("/api/s/{ptoken}"), None, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // A clearly-future expiry stays active.
    let fut = mint(&app, &owner, &deck, "edit", Some("2999-01-01T00:00:00Z")).await;
    assert_eq!(fut.json["status"], "active");
    let ftoken = fut.json["token"].as_str().unwrap().to_owned();
    assert_eq!(
        send(&app, "GET", &format!("/api/s/{ftoken}"), None, None, None)
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn deleted_deck_makes_tokens_404() {
    let app = test_app("shares-deleted").await;
    let owner = signup(&app, "owner").await;
    let deck = create_deck(&app, &owner, "# Hi").await;
    let token = mint(&app, &owner, &deck, "edit", None).await.json["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        send(&app, "GET", &format!("/api/s/{token}"), None, None, None)
            .await
            .status,
        StatusCode::OK
    );
    // Soft-delete the deck → its tokens all resolve 404.
    send(
        &app,
        "DELETE",
        &format!("/api/decks/{deck}"),
        None,
        Some(&owner),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(
        send(&app, "GET", &format!("/api/s/{token}"), None, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn share_cookie_authorizes_asset_and_revocation_cuts_it() {
    let app = test_app("shares-asset").await;
    let owner = signup(&app, "owner").await;
    let deck = create_deck(&app, &owner, "# Hi").await;

    // Owner uploads an image to the deck.
    let (ct, body) = multipart_file("pic.png", "image/png", &tiny_png());
    let up = send_raw(
        &app,
        "POST",
        &format!("/api/decks/{deck}/assets"),
        &ct,
        body,
        Some(&owner),
    )
    .await;
    assert_eq!(up.status, StatusCode::CREATED);
    let url = up.json["url"].as_str().unwrap().to_owned(); // /assets/{deck}/{file}

    // Anonymous, no cookie → 404.
    assert_eq!(
        send(&app, "GET", &url, None, None, None).await.status,
        StatusCode::NOT_FOUND
    );

    // With the per-deck share cookie for an active view link → 200.
    let view = mint(&app, &owner, &deck, "view", None).await;
    let vtoken = view.json["token"].as_str().unwrap().to_owned();
    let vid = view.json["id"].as_str().unwrap().to_owned();
    let cookie = format!("deckoala_share_{deck}={vtoken}");
    assert_eq!(
        send(&app, "GET", &url, None, Some(&cookie), None)
            .await
            .status,
        StatusCode::OK
    );

    // Revoke the link → the same cookie no longer authorizes (immediate).
    send(
        &app,
        "DELETE",
        &format!("/api/decks/{deck}/shares/{vid}"),
        None,
        Some(&owner),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(
        send(&app, "GET", &url, None, Some(&cookie), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn shares_migration_roundtrip_to_v6() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v6", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 6).await.expect("undo to version 6");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'share_links'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 0);

    migrator.run(&db).await.expect("re-apply");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'share_links'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 1);
}
