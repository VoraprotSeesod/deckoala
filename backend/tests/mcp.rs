mod common;

use axum::http::StatusCode;
use common::{send, send_bearer, signup, test_app, test_app_full};
use serde_json::json;

const ORIGIN: &str = "http://localhost:8080";

/// Mint an API token for the session holder.
async fn mint_token(app: &axum::Router, cookie: &str, name: &str) -> String {
    let r = send(
        app,
        "POST",
        "/api/tokens",
        Some(json!({ "name": name })),
        Some(cookie),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "token create failed");
    r.json["token"].as_str().unwrap().to_owned()
}

async fn rpc(
    app: &axum::Router,
    token: Option<&str>,
    body: serde_json::Value,
) -> common::TestResponse {
    send_bearer(app, "POST", "/mcp", Some(body), token).await
}

/// Call a tool and return its text content (tools/call wraps results in
/// `content: [{ type: "text", text }]`).
async fn call_tool(
    app: &axum::Router,
    token: &str,
    name: &str,
    args: serde_json::Value,
) -> (bool, String) {
    let r = rpc(
        app,
        Some(token),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": args } }),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let result = &r.json["result"];
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    (is_error, text)
}

#[tokio::test]
async fn mcp_requires_a_valid_active_token() {
    let app = test_app("mcp-auth").await;
    let user = signup(&app, "user").await;
    let token = mint_token(&app, &user, "cli").await;
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });

    // No token, garbage token → 401.
    assert_eq!(
        rpc(&app, None, init.clone()).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        rpc(&app, Some("dko_nonsense"), init.clone()).await.status,
        StatusCode::UNAUTHORIZED
    );
    // Valid token works.
    assert_eq!(
        rpc(&app, Some(&token), init.clone()).await.status,
        StatusCode::OK
    );

    // Revoked → 401 immediately.
    let list = send(&app, "GET", "/api/tokens", None, Some(&user), None).await;
    let id = list.json[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/tokens/{id}"),
            None,
            Some(&user),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        rpc(&app, Some(&token), init).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn token_value_is_returned_once_and_never_listed() {
    let app = test_app("mcp-token-once").await;
    let user = signup(&app, "user").await;
    let created = send(
        &app,
        "POST",
        "/api/tokens",
        Some(json!({ "name": "claude" })),
        Some(&user),
        Some(ORIGIN),
    )
    .await;
    let token = created.json["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("dko_"));

    // The list must never carry the value (nor a hash).
    let list = send(&app, "GET", "/api/tokens", None, Some(&user), None).await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(!list.text.contains(&token));
    assert!(!list.text.contains("token_hash"));
    assert!(list.json[0]["token"].is_null());

    // Name validation.
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/tokens",
            Some(json!({ "name": "  " })),
            Some(&user),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn one_user_cannot_revoke_or_see_anothers_token() {
    let app = test_app("mcp-token-scope").await;
    let alice = signup(&app, "alice").await;
    let bob = signup(&app, "bob").await;
    mint_token(&app, &bob, "bob-cli").await;

    let bob_list = send(&app, "GET", "/api/tokens", None, Some(&bob), None).await;
    let bob_token_id = bob_list.json[0]["id"].as_str().unwrap().to_owned();

    // Alice sees none of Bob's tokens and cannot revoke one by id.
    let alice_list = send(&app, "GET", "/api/tokens", None, Some(&alice), None).await;
    assert_eq!(alice_list.json.as_array().unwrap().len(), 0);
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/tokens/{bob_token_id}"),
            None,
            Some(&alice),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn initialize_tools_list_and_notifications() {
    let app = test_app("mcp-protocol").await;
    let user = signup(&app, "user").await;
    let token = mint_token(&app, &user, "cli").await;

    let init = rpc(
        &app,
        Some(&token),
        json!({ "jsonrpc": "2.0", "id": "a", "method": "initialize",
                "params": { "protocolVersion": "2025-06-18" } }),
    )
    .await;
    assert_eq!(init.json["id"], "a");
    assert_eq!(init.json["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init.json["result"]["serverInfo"]["name"], "deckoala");

    let tools = rpc(
        &app,
        Some(&token),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await;
    let names: Vec<&str> = tools.json["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["list_decks", "get_deck", "create_deck", "update_deck"]
    );

    // A notification (no `id`) must get no JSON-RPC reply at all.
    let note = rpc(
        &app,
        Some(&token),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(note.status, StatusCode::ACCEPTED);
    assert!(note.text.is_empty());

    // Unknown method → -32601.
    let unknown = rpc(
        &app,
        Some(&token),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "nope/nope" }),
    )
    .await;
    assert_eq!(unknown.json["error"]["code"], -32601);
}

#[tokio::test]
async fn tools_create_get_update_and_snapshot_a_revision() {
    // revision_min_secs = 0 so the first markdown change snapshots.
    let (app, _db) = test_app_full("mcp-tools", true, 0).await;
    let user = signup(&app, "user").await;
    let token = mint_token(&app, &user, "cli").await;

    let (err, created) = call_tool(
        &app,
        &token,
        "create_deck",
        json!({ "title": "From MCP", "markdown": "# From MCP\n" }),
    )
    .await;
    assert!(!err, "create_deck failed: {created}");
    let deck_id = serde_json::from_str::<serde_json::Value>(&created).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // It shows up in list_decks and get_deck.
    let (_, listed) = call_tool(&app, &token, "list_decks", json!({})).await;
    assert!(listed.contains(&deck_id));
    let (_, got) = call_tool(&app, &token, "get_deck", json!({ "deckId": &deck_id })).await;
    assert!(got.contains("From MCP"));

    // Update replaces content and snapshots the previous version.
    let (err, _) = call_tool(
        &app,
        &token,
        "update_deck",
        json!({ "deckId": &deck_id, "markdown": "# Edited by MCP\n" }),
    )
    .await;
    assert!(!err);
    let revs = send(
        &app,
        "GET",
        &format!("/api/decks/{deck_id}/revisions"),
        None,
        Some(&user),
        None,
    )
    .await;
    assert_eq!(
        revs.json.as_array().unwrap().len(),
        1,
        "update must snapshot"
    );
}

#[tokio::test]
async fn a_token_cannot_touch_another_users_deck() {
    let app = test_app("mcp-cross-user").await;
    let alice = signup(&app, "alice").await;
    let bob = signup(&app, "bob").await;
    let bob_token = mint_token(&app, &bob, "bob-cli").await;

    // Alice owns a deck.
    let deck = send(
        &app,
        "POST",
        "/api/decks",
        Some(json!({ "title": "Alice", "markdown": "# Alice\n" })),
        Some(&alice),
        Some(ORIGIN),
    )
    .await;
    let alice_deck = deck.json["id"].as_str().unwrap().to_owned();

    // Bob's token can neither read nor write it — indistinguishable from missing.
    let (err, text) = call_tool(
        &app,
        &bob_token,
        "get_deck",
        json!({ "deckId": &alice_deck }),
    )
    .await;
    assert!(err, "cross-user read must fail");
    assert!(text.contains("not found"));
    let (err, _) = call_tool(
        &app,
        &bob_token,
        "update_deck",
        json!({ "deckId": &alice_deck, "markdown": "# pwned\n" }),
    )
    .await;
    assert!(err, "cross-user write must fail");
    // Alice's deck is untouched.
    let still = send(
        &app,
        "GET",
        &format!("/api/decks/{alice_deck}"),
        None,
        Some(&alice),
        None,
    )
    .await;
    assert!(still.json["markdown"].as_str().unwrap().contains("# Alice"));
    assert!(!still.text.contains("pwned"));
}

#[tokio::test]
async fn mcp_writes_enforce_the_same_bounds_as_http() {
    // The data helpers own validation, so MCP cannot write rows the HTTP path
    // would reject (BRIEF-0011 review finding).
    let app = test_app("mcp-bounds").await;
    let user = signup(&app, "user").await;
    let token = mint_token(&app, &user, "cli").await;

    let huge = "x".repeat(1_000_001);
    let (err, text) = call_tool(&app, &token, "create_deck", json!({ "markdown": huge })).await;
    assert!(err, "oversized markdown must be rejected");
    assert!(text.contains("too large"));

    let (err, text) = call_tool(
        &app,
        &token,
        "create_deck",
        json!({ "title": "bad\ntitle", "markdown": "# ok\n" }),
    )
    .await;
    assert!(err, "control-char title must be rejected");
    assert!(text.contains("control characters"));

    let (err, text) = call_tool(
        &app,
        &token,
        "create_deck",
        json!({ "title": "x".repeat(201), "markdown": "# ok\n" }),
    )
    .await;
    assert!(err, "over-long title must be rejected");
    assert!(text.contains("200 characters"));
}

#[tokio::test]
async fn agent_written_markdown_keeps_the_deckoala_theme() {
    // An external model has no reason to know the frontmatter contract; without
    // a backfill the deck silently renders in marp-core's stock theme (and Thai
    // text loses its font fallback).
    let app = test_app("mcp-frontmatter").await;
    let user = signup(&app, "user").await;
    let token = mint_token(&app, &user, "cli").await;

    let (_, created) = call_tool(
        &app,
        &token,
        "create_deck",
        json!({ "title": "No frontmatter", "markdown": "# สวัสดี\n\n---\n\n## Slide two\n" }),
    )
    .await;
    let deck_id = serde_json::from_str::<serde_json::Value>(&created).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, got) = call_tool(&app, &token, "get_deck", json!({ "deckId": &deck_id })).await;
    assert!(got.contains("theme: deckoala"), "theme must be preserved");
    assert!(got.contains("marp: true"));

    // An owner's custom frontmatter (e.g. a font override) survives an update.
    let custom = "---\nmarp: true\ntheme: deckoala\nstyle: |\n  section { font-family: 'Sarabun'; }\n---\n\n# Kept\n";
    let (err, _) = call_tool(
        &app,
        &token,
        "update_deck",
        json!({ "deckId": &deck_id, "markdown": custom }),
    )
    .await;
    assert!(!err);
    let (_, after) = call_tool(&app, &token, "get_deck", json!({ "deckId": &deck_id })).await;
    assert!(
        after.contains("Sarabun"),
        "custom frontmatter must not be rewritten"
    );
    assert_eq!(
        after.matches("marp: true").count(),
        1,
        "frontmatter must not be duplicated"
    );
}

#[tokio::test]
async fn mcp_writes_are_budgeted_but_reads_are_not() {
    // A runaway agent loop must not be able to fill the volume with decks.
    let app = test_app("mcp-budget").await;
    let user = signup(&app, "user").await;
    let token = mint_token(&app, &user, "cli").await;

    let mut rejected = 0;
    for _ in 0..62 {
        let (err, text) = call_tool(&app, &token, "create_deck", json!({ "title": "spam" })).await;
        if err {
            assert!(text.contains("slow down"), "unexpected failure: {text}");
            rejected += 1;
        }
    }
    assert!(
        rejected >= 2,
        "the write window must close (rejected {rejected})"
    );

    // Reads stay available once the write budget is spent.
    let (err, _) = call_tool(&app, &token, "list_decks", json!({})).await;
    assert!(!err, "reads must not be rate limited");
}

#[tokio::test]
async fn well_known_probes_do_not_get_the_spa_shell() {
    let app = test_app("mcp-wellknown").await;
    // An OAuth-discovery probe must 404, not fall through to the SPA.
    let probe = send(
        &app,
        "GET",
        "/.well-known/oauth-authorization-server",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(probe.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tokens_migration_roundtrip_to_v8() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v8", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 8).await.expect("undo to version 8");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'api_tokens'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 0);

    migrator.run(&db).await.expect("re-apply");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'api_tokens'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 1);
}
