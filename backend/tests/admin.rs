mod common;

use axum::http::StatusCode;
use common::{send, signup, test_app};
use serde_json::json;

const ORIGIN: &str = "http://localhost:8080";

fn ai_body(enabled: bool, key: Option<&str>, remove: bool) -> serde_json::Value {
    let mut body = json!({
        "enabled": enabled,
        "provider": "anthropic",
        "baseUrl": "https://api.anthropic.com",
        "model": "claude-sonnet-4-6",
        "removeApiKey": remove,
    });
    if let Some(k) = key {
        body["apiKey"] = json!(k);
    }
    body
}

#[tokio::test]
async fn settings_are_admin_only() {
    let app = test_app("admin-scope").await;
    let admin = signup(&app, "admin").await; // first user = admin
    let user = signup(&app, "user").await;

    // Anonymous → 401; non-admin → 403; admin → 200.
    assert_eq!(
        send(&app, "GET", "/api/admin/settings", None, None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app, "GET", "/api/admin/settings", None, Some(&user), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, "GET", "/api/admin/settings", None, Some(&admin), None)
            .await
            .status,
        StatusCode::OK
    );
    // A non-admin cannot write either.
    assert_eq!(
        send(
            &app,
            "PUT",
            "/api/admin/settings",
            Some(ai_body(false, Some("sk-secret"), false)),
            Some(&user),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn api_key_is_write_only_and_removable() {
    let app = test_app("admin-key").await;
    let admin = signup(&app, "admin").await;

    // Store a key + enable.
    let put = send(
        &app,
        "PUT",
        "/api/admin/settings",
        Some(ai_body(true, Some("sk-supersecret-1234"), false)),
        Some(&admin),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(put.status, StatusCode::OK);
    // The key itself must NEVER appear in any response.
    assert!(!put.text.contains("supersecret"));
    assert_eq!(put.json["ai"]["apiKeySet"], true);
    assert_eq!(put.json["ai"]["apiKeyLast4"], "1234");
    assert!(put.json["ai"]["apiKey"].is_null());

    let got = send(&app, "GET", "/api/admin/settings", None, Some(&admin), None).await;
    assert!(!got.text.contains("supersecret"));
    assert_eq!(got.json["ai"]["enabled"], true);

    // Omitting the key KEEPS the stored one.
    let kept = send(
        &app,
        "PUT",
        "/api/admin/settings",
        Some(ai_body(true, None, false)),
        Some(&admin),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(kept.json["ai"]["apiKeySet"], true);

    // Explicit removal clears it — and then AI cannot stay enabled.
    let cleared = send(
        &app,
        "PUT",
        "/api/admin/settings",
        Some(ai_body(false, None, true)),
        Some(&admin),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(cleared.status, StatusCode::OK);
    assert_eq!(cleared.json["ai"]["apiKeySet"], false);
    // Enabling without a key is rejected.
    assert_eq!(
        send(
            &app,
            "PUT",
            "/api/admin/settings",
            Some(ai_body(true, None, false)),
            Some(&admin),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn ai_generate_requires_auth_and_configuration() {
    let app = test_app("admin-ai").await;
    let user = signup(&app, "user").await;

    // Anonymous → 401.
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/ai/generate",
            Some(json!({ "prompt": "a deck about koalas" })),
            None,
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    // Signed in but AI unconfigured → 503 (never an outbound call).
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/ai/generate",
            Some(json!({ "prompt": "a deck about koalas" })),
            Some(&user),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn change_password_verifies_current_and_rotates() {
    let app = test_app("admin-pw").await;
    let user = signup(&app, "someone").await; // helper registers with "password123"

    // Wrong current password → 401.
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/auth/password",
            Some(json!({ "currentPassword": "wrong-one", "newPassword": "brand-new-pw" })),
            Some(&user),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    // Too-short new password → 422.
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/auth/password",
            Some(json!({ "currentPassword": "password123", "newPassword": "short" })),
            Some(&user),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Correct → 204.
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/auth/password",
            Some(json!({ "currentPassword": "password123", "newPassword": "brand-new-pw" })),
            Some(&user),
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );

    // The OLD password no longer works; the new one does.
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/auth/login",
            Some(json!({ "username": "someone", "password": "password123" })),
            None,
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/auth/login",
            Some(json!({ "username": "someone", "password": "brand-new-pw" })),
            None,
            Some(ORIGIN)
        )
        .await
        .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn instance_endpoint_never_leaks_the_root_password_flag() {
    let app = test_app("admin-instance").await;
    // /api/instance is ANONYMOUS — the default-password flag must not be there
    // at all (absent, not false), or it becomes a scannable beacon.
    let anon = send(&app, "GET", "/api/instance", None, None, None).await;
    assert_eq!(anon.status, StatusCode::OK);
    assert!(!anon.text.contains("rootPasswordIsDefault"));
    assert!(anon.json.get("rootPasswordIsDefault").is_none());
}

#[tokio::test]
async fn me_exposes_flags_only_to_a_signed_in_user() {
    let app = test_app("admin-me").await;
    let admin = signup(&app, "admin").await;
    let me = send(&app, "GET", "/api/auth/me", None, Some(&admin), None).await;
    assert_eq!(me.status, StatusCode::OK);
    // Present for a signed-in caller; AI is off on a fresh instance.
    assert_eq!(me.json["aiEnabled"], false);
    assert!(me.json.get("rootPasswordIsDefault").is_some());
    // Register/login response shapes stay unchanged (no flags leaked there).
    let login = send(
        &app,
        "POST",
        "/api/auth/login",
        Some(json!({ "username": "admin", "password": "password123" })),
        None,
        Some(ORIGIN),
    )
    .await;
    assert!(login.json.get("aiEnabled").is_none());
}

#[tokio::test]
async fn seed_root_is_idempotent_and_flags_only_the_builtin_default() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-seedroot", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();

    // First seed creates root and records that the built-in default was used.
    assert!(deckoala_server::auth::seed_root(&db, "Admin123456@", true)
        .await
        .unwrap());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = 'root'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let flag: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'root_password_is_default'")
            .fetch_optional(&db)
            .await
            .unwrap();
    assert_eq!(flag.as_deref(), Some("true"));

    // Re-running never creates a second root and never resets the password.
    assert!(!deckoala_server::auth::seed_root(&db, "Different!", true)
        .await
        .unwrap());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn custom_root_password_sets_no_default_warning() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-seedcustom", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();

    assert!(
        deckoala_server::auth::seed_root(&db, "an-operator-supplied-pw", false)
            .await
            .unwrap()
    );
    let flag: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'root_password_is_default'")
            .fetch_optional(&db)
            .await
            .unwrap();
    assert!(flag.is_none(), "custom password must not raise the warning");
}

#[tokio::test]
async fn settings_migration_roundtrip_to_v7() {
    let data_dir =
        std::env::temp_dir().join(format!("deckoala-test-{}-migrate-v7", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 7).await.expect("undo to version 7");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 0);

    migrator.run(&db).await.expect("re-apply");
    let table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(table, 1);
}
