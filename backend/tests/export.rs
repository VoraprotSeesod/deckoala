mod common;

use axum::http::StatusCode;
use common::{multipart_file, send, send_raw, signup, test_app, tiny_png};
use deckoala_server::export::mint_print_token;
use serde_json::json;

// The test AppState uses this fixed print secret (see tests/common/mod.rs).
const TEST_SECRET: [u8; 32] = [7u8; 32];

async fn make_deck(app: &axum::Router, cookie: &str, title: &str) -> String {
    let created = send(
        app,
        "POST",
        "/api/decks",
        Some(json!({ "title": title, "markdown": "# Hi\n" })),
        Some(cookie),
        None,
    )
    .await;
    created.json["id"].as_str().unwrap().to_owned()
}

// --- export POST: only the non-Chromium paths (auth + owner) are unit-tested;
// the real render is verified at runtime where Chromium exists. ---

#[tokio::test]
async fn export_pdf_requires_auth() {
    let app = test_app("export-auth").await;
    let res = send(
        &app,
        "POST",
        "/api/decks/whatever/export/pdf",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn export_pdf_foreign_deck_is_404_before_chromium() {
    let app = test_app("export-foreign").await;
    let owner = signup(&app, "owner").await;
    let stranger = signup(&app, "stranger").await;
    let id = make_deck(&app, &owner, "Mine").await;

    // A stranger's export must 404 at the owner check — never launching a
    // browser (which isn't present in the test build anyway).
    let res = send(
        &app,
        "POST",
        &format!("/api/decks/{id}/export/pdf"),
        None,
        Some(&stranger),
        None,
    )
    .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

// --- /api/print: print-cookie authorization ---

#[tokio::test]
async fn print_data_needs_a_matching_print_cookie() {
    let app = test_app("print-data").await;
    let owner = signup(&app, "owner").await;
    let id = make_deck(&app, &owner, "Deck").await;
    let other = make_deck(&app, &owner, "Other").await;

    // No cookie → 404.
    let none = send(&app, "GET", &format!("/api/print/{id}"), None, None, None).await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);

    // Valid cookie for THIS deck → 200 with the markdown (no session needed).
    let token = mint_print_token(&TEST_SECRET, &id, 120);
    let ok = send(
        &app,
        "GET",
        &format!("/api/print/{id}"),
        None,
        Some(&format!("deckoala_print={token}")),
        None,
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.json["markdown"], "# Hi\n");
    assert_eq!(ok.json["theme"], "deckoala");

    // A token for a DIFFERENT deck must not read this one.
    let wrong = mint_print_token(&TEST_SECRET, &other, 120);
    let mismatched = send(
        &app,
        "GET",
        &format!("/api/print/{id}"),
        None,
        Some(&format!("deckoala_print={wrong}")),
        None,
    )
    .await;
    assert_eq!(mismatched.status, StatusCode::NOT_FOUND);
}

// --- asset serve: a print cookie for the deck authorizes image reads ---

#[tokio::test]
async fn assets_served_to_a_matching_print_cookie() {
    let app = test_app("print-assets").await;
    let owner = signup(&app, "owner").await;
    let id = make_deck(&app, &owner, "Deck").await;
    let other = make_deck(&app, &owner, "Other").await;

    // Upload an image as the owner.
    let (ct, body) = multipart_file("p.png", "image/png", &tiny_png());
    let uploaded = send_raw(
        &app,
        "POST",
        &format!("/api/decks/{id}/assets"),
        &ct,
        body,
        Some(&owner),
    )
    .await;
    let url = uploaded.json["url"].as_str().unwrap().to_owned();

    // A print cookie for THIS deck (no session) can read the asset.
    let token = mint_print_token(&TEST_SECRET, &id, 120);
    let served = send(
        &app,
        "GET",
        &url,
        None,
        Some(&format!("deckoala_print={token}")),
        None,
    )
    .await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.bytes, tiny_png());

    // A print cookie for a DIFFERENT deck cannot.
    let wrong = mint_print_token(&TEST_SECRET, &other, 120);
    let denied = send(
        &app,
        "GET",
        &url,
        None,
        Some(&format!("deckoala_print={wrong}")),
        None,
    )
    .await;
    assert_eq!(denied.status, StatusCode::NOT_FOUND);
}
