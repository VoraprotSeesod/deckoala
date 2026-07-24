mod common;

use axum::http::StatusCode;
use common::{multipart_file, send, send_raw, signup, test_app, tiny_png};
use serde_json::json;

const ORIGIN: &str = "http://localhost:8080";

async fn upload_doc(
    app: &axum::Router,
    cookie: &str,
    filename: &str,
    mime: &str,
    data: &[u8],
) -> common::TestResponse {
    let (content_type, body) = multipart_file(filename, mime, data);
    send_raw(
        app,
        "POST",
        "/api/research",
        &content_type,
        body,
        Some(cookie),
    )
    .await
}

#[tokio::test]
async fn text_upload_is_extracted_listed_and_previewable() {
    let app = test_app("research-text").await;
    let user = signup(&app, "owner").await;

    let created = upload_doc(
        &app,
        &user,
        "notes.txt",
        "text/plain",
        b"Findings\n\n\nOur study shows a 42% improvement.\n",
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    assert_eq!(created.json["originalName"], "notes.txt");
    assert!(created.json["charCount"].as_i64().unwrap() > 0);
    let id = created.json["id"].as_str().unwrap().to_owned();

    let list = send(&app, "GET", "/api/research", None, Some(&user), None).await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.json.as_array().unwrap().len(), 1);

    let preview = send(
        &app,
        "GET",
        &format!("/api/research/{id}/preview"),
        None,
        Some(&user),
        None,
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK);
    assert!(preview.json["snippet"]
        .as_str()
        .unwrap()
        .contains("42% improvement"));
}

#[tokio::test]
async fn thai_research_text_survives_extraction() {
    let app = test_app("research-thai").await;
    let user = signup(&app, "owner").await;
    let created = upload_doc(
        &app,
        &user,
        "งานวิจัย.txt",
        "text/plain",
        "ผลการทดลองแสดงว่าดีขึ้น 42%".as_bytes(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);

    let id = created.json["id"].as_str().unwrap().to_owned();
    let preview = send(
        &app,
        "GET",
        &format!("/api/research/{id}/preview"),
        None,
        Some(&user),
        None,
    )
    .await;
    assert!(preview.json["snippet"]
        .as_str()
        .unwrap()
        .contains("ผลการทดลอง"));
}

#[tokio::test]
async fn non_document_uploads_are_rejected() {
    let app = test_app("research-reject").await;
    let user = signup(&app, "owner").await;

    // A PNG is not research source material.
    let png = upload_doc(&app, &user, "photo.png", "image/png", &tiny_png()).await;
    assert_eq!(png.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // A file whose bytes claim PDF but are garbage → 422, never a crash.
    let fake_pdf = upload_doc(
        &app,
        &user,
        "paper.pdf",
        "application/pdf",
        b"%PDF-1.7 not really a pdf",
    )
    .await;
    assert_eq!(fake_pdf.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn research_is_owner_scoped_end_to_end() {
    let app = test_app("research-scope").await;
    let alice = signup(&app, "alice").await;
    let bob = signup(&app, "bob").await;

    let created = upload_doc(
        &app,
        &alice,
        "alice.txt",
        "text/plain",
        b"Alice secret data",
    )
    .await;
    let id = created.json["id"].as_str().unwrap().to_owned();

    // Bob's list is empty and he can neither preview, list figures, nor delete.
    let bob_list = send(&app, "GET", "/api/research", None, Some(&bob), None).await;
    assert_eq!(bob_list.json.as_array().unwrap().len(), 0);

    for path in [
        format!("/api/research/{id}/preview"),
        format!("/api/research/{id}/figures"),
    ] {
        let res = send(&app, "GET", &path, None, Some(&bob), None).await;
        assert_eq!(
            res.status,
            StatusCode::NOT_FOUND,
            "{path} leaked to a stranger"
        );
    }
    let del = send(
        &app,
        "DELETE",
        &format!("/api/research/{id}"),
        None,
        Some(&bob),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(del.status, StatusCode::NOT_FOUND);

    // Alice's document is untouched.
    let still = send(&app, "GET", "/api/research", None, Some(&alice), None).await;
    assert_eq!(still.json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn owner_can_delete_their_research() {
    let app = test_app("research-delete").await;
    let user = signup(&app, "owner").await;
    let created = upload_doc(&app, &user, "n.txt", "text/plain", b"data").await;
    let id = created.json["id"].as_str().unwrap().to_owned();

    let del = send(
        &app,
        "DELETE",
        &format!("/api/research/{id}"),
        None,
        Some(&user),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);

    let list = send(&app, "GET", "/api/research", None, Some(&user), None).await;
    assert_eq!(list.json.as_array().unwrap().len(), 0);
    // A second delete is a 404 (already gone).
    let again = send(
        &app,
        "DELETE",
        &format!("/api/research/{id}"),
        None,
        Some(&user),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn attaching_a_figure_requires_owning_both_deck_and_figure() {
    let app = test_app("research-attach").await;
    let alice = signup(&app, "alice").await;
    let bob = signup(&app, "bob").await;

    let deck = send(
        &app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Alice deck" })),
        Some(&alice),
        Some(ORIGIN),
    )
    .await;
    let deck_id = deck.json["id"].as_str().unwrap().to_owned();

    // A figure id that doesn't exist → 404, and never a 500.
    let missing = send(
        &app,
        "POST",
        &format!("/api/decks/{deck_id}/figures/does-not-exist"),
        None,
        Some(&alice),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // Bob cannot attach into Alice's deck.
    let foreign = send(
        &app,
        "POST",
        &format!("/api/decks/{deck_id}/figures/whatever"),
        None,
        Some(&bob),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
}

/// A real PDF (text + one embedded JPEG), so the tests exercise the actual
/// parser rather than only the `.txt` shortcut.
const PAPER_PDF: &[u8] = include_bytes!("fixtures/paper.pdf");

#[tokio::test]
async fn pdf_upload_extracts_text_and_embedded_figures() {
    let app = test_app("research-pdf").await;
    let user = signup(&app, "owner").await;

    let created = upload_doc(&app, &user, "paper.pdf", "application/pdf", PAPER_PDF).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    assert_eq!(created.json["mime"], "application/pdf");
    assert!(
        created.json["charCount"].as_i64().unwrap() > 100,
        "expected real extracted text, got {}",
        created.json["charCount"]
    );
    let id = created.json["id"].as_str().unwrap().to_owned();

    let preview = send(
        &app,
        "GET",
        &format!("/api/research/{id}/preview"),
        None,
        Some(&user),
        None,
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK);
    assert!(!preview.json["snippet"].as_str().unwrap().trim().is_empty());

    // Regression guard: the figure lives behind an *indirect* /Resources
    // reference, which is how real PDFs are built. Reading only the inline
    // half of `get_page_resources()` found nothing here.
    let figures = send(
        &app,
        "GET",
        &format!("/api/research/{id}/figures"),
        None,
        Some(&user),
        None,
    )
    .await;
    assert_eq!(figures.status, StatusCode::OK);
    let list = figures.json.as_array().unwrap();
    assert!(!list.is_empty(), "expected at least one extracted figure");
    assert_eq!(list[0]["mime"], "image/jpeg");
    assert!(list[0]["width"].as_i64().unwrap() >= 64);
    assert!(list[0]["height"].as_i64().unwrap() >= 64);

    // Regression guard: the stream is `[/ASCII85Decode, /DCTDecode]`, so the
    // raw bytes are *not* the JPEG. Serving them unwrapped yields a file no
    // browser can decode.
    let figure_url = list[0]["url"].as_str().unwrap().to_owned();
    let raw = send(&app, "GET", &figure_url, None, Some(&user), None).await;
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(
        &raw.bytes[..2],
        &[0xFF, 0xD8],
        "served figure is not a decodable JPEG"
    );
}

#[tokio::test]
async fn deleting_a_document_that_has_figures_succeeds() {
    // Regression guard: figures are FK children of the document. Removing the
    // parent first aborts the whole delete, so a PDF with figures could never
    // be deleted — which the text-only delete test cannot catch.
    let app = test_app("research-delete-figures").await;
    let user = signup(&app, "owner").await;

    let created = upload_doc(&app, &user, "paper.pdf", "application/pdf", PAPER_PDF).await;
    let id = created.json["id"].as_str().unwrap().to_owned();
    let figures = send(
        &app,
        "GET",
        &format!("/api/research/{id}/figures"),
        None,
        Some(&user),
        None,
    )
    .await;
    let figure_url = figures.json[0]["url"].as_str().unwrap().to_owned();
    // The figure really is reachable before the delete, so the check below is
    // a genuine before/after rather than a route that 404s either way.
    let before = send(&app, "GET", &figure_url, None, Some(&user), None).await;
    assert_eq!(before.status, StatusCode::OK);

    let del = send(
        &app,
        "DELETE",
        &format!("/api/research/{id}"),
        None,
        Some(&user),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT, "{}", del.text);

    // The children went with it rather than being orphaned.
    let orphan = send(&app, "GET", &figure_url, None, Some(&user), None).await;
    assert_eq!(orphan.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_pdf_figure_can_be_attached_to_a_deck() {
    let app = test_app("research-attach-real").await;
    let user = signup(&app, "owner").await;

    let deck = send(
        &app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Paper deck" })),
        Some(&user),
        Some(ORIGIN),
    )
    .await;
    let deck_id = deck.json["id"].as_str().unwrap().to_owned();

    let created = upload_doc(&app, &user, "paper.pdf", "application/pdf", PAPER_PDF).await;
    let doc_id = created.json["id"].as_str().unwrap().to_owned();
    let figures = send(
        &app,
        "GET",
        &format!("/api/research/{doc_id}/figures"),
        None,
        Some(&user),
        None,
    )
    .await;
    let figure_id = figures.json[0]["id"].as_str().unwrap().to_owned();

    let attached = send(
        &app,
        "POST",
        &format!("/api/decks/{deck_id}/figures/{figure_id}"),
        None,
        Some(&user),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(attached.status, StatusCode::CREATED, "{}", attached.text);
    let url = attached.json["url"].as_str().unwrap().to_owned();
    assert!(url.starts_with(&format!("/assets/{deck_id}/")));

    // The copy is a first-class deck asset the deck can actually serve.
    let served = send(&app, "GET", &url, None, Some(&user), None).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(&served.bytes[..2], &[0xFF, 0xD8]);
}

#[tokio::test]
async fn research_requires_authentication() {
    let app = test_app("research-auth").await;
    let anon = send(&app, "GET", "/api/research", None, None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn research_migration_roundtrip_to_v9() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v9", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 9).await.expect("undo to version 9");
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name IN ('research_docs', 'research_figures')",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(tables, 0);

    migrator.run(&db).await.expect("re-apply");
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name IN ('research_docs', 'research_figures')",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(tables, 2);
}
