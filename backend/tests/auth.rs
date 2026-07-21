mod common;

use axum::http::StatusCode;
use common::{send, session_cookie, test_app, test_app_with};
use serde_json::json;

fn creds(username: &str, password: &str) -> serde_json::Value {
    json!({ "username": username, "password": password })
}

#[tokio::test]
async fn register_first_user_is_admin_with_session() {
    let app = test_app("reg-admin").await;
    let response = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", "hunter2hunter2")),
        None,
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::CREATED);
    assert_eq!(response.json["username"], "koala");
    assert_eq!(response.json["isAdmin"], true);
    let cookie = session_cookie(&response).expect("register must set a session cookie");

    let me = send(&app, "GET", "/api/auth/me", None, Some(&cookie), None).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json["username"], "koala");
}

#[tokio::test]
async fn second_register_is_not_admin() {
    let app = test_app("reg-second").await;
    send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("first", "password123")),
        None,
        None,
    )
    .await;
    let second = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("second", "password123")),
        None,
        None,
    )
    .await;
    assert_eq!(second.status, StatusCode::CREATED);
    assert_eq!(second.json["isAdmin"], false);
}

#[tokio::test]
async fn duplicate_username_conflict_case_insensitive() {
    let app = test_app("reg-dup").await;
    let first = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", "password123")),
        None,
        None,
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED);
    let duplicate = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("KoAlA", "password123")),
        None,
        None,
    )
    .await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn login_failures_are_uniform() {
    let app = test_app("login-fail").await;
    send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", "password123")),
        None,
        None,
    )
    .await;

    let wrong_password = send(
        &app,
        "POST",
        "/api/auth/login",
        Some(creds("koala", "wrong-password")),
        None,
        None,
    )
    .await;
    let unknown_user = send(
        &app,
        "POST",
        "/api/auth/login",
        Some(creds("nobody", "wrong-password")),
        None,
        None,
    )
    .await;
    assert_eq!(wrong_password.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown_user.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong_password.json, unknown_user.json,
        "login failures must not reveal whether the username exists"
    );
}

#[tokio::test]
async fn logout_invalidates_session() {
    let app = test_app("logout").await;
    let registered = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", "password123")),
        None,
        None,
    )
    .await;
    let cookie = session_cookie(&registered).unwrap();

    let logout = send(&app, "POST", "/api/auth/logout", None, Some(&cookie), None).await;
    assert_eq!(logout.status, StatusCode::NO_CONTENT);

    let me = send(&app, "GET", "/api/auth/me", None, Some(&cookie), None).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signup_disabled_allows_only_first_user() {
    let app = test_app_with("signup-off", false).await;
    let first = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("admin", "password123")),
        None,
        None,
    )
    .await;
    assert_eq!(
        first.status,
        StatusCode::CREATED,
        "first-run bootstrap must work even with signup disabled"
    );
    assert_eq!(first.json["isAdmin"], true);

    let second = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("intruder", "password123")),
        None,
        None,
    )
    .await;
    assert_eq!(second.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cross_origin_mutation_rejected() {
    let app = test_app("origin").await;
    let foreign = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", "password123")),
        None,
        Some("http://evil.example"),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::FORBIDDEN);

    let same_origin = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", "password123")),
        None,
        Some(&format!("http://{}", common::TEST_HOST)),
    )
    .await;
    assert_eq!(same_origin.status, StatusCode::CREATED);
}

#[tokio::test]
async fn password_too_long_rejected_before_hashing() {
    let app = test_app("pw-max").await;
    let long_password = "x".repeat(129);
    let response = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", &long_password)),
        None,
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn login_with_oversized_password_gets_uniform_401() {
    let app = test_app("login-pw-max").await;
    send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("koala", "password123")),
        None,
        None,
    )
    .await;
    let long_password = "x".repeat(4096);
    let response = send(
        &app,
        "POST",
        "/api/auth/login",
        Some(creds("koala", &long_password)),
        None,
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.json["error"], "invalid username or password");
}

#[tokio::test]
async fn concurrent_first_registrations_yield_exactly_one_admin() {
    let (app, db) = common::test_app_with_db("race-admin", true).await;
    let register_a = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("racer-a", "password123")),
        None,
        None,
    );
    let register_b = send(
        &app,
        "POST",
        "/api/auth/register",
        Some(creds("racer-b", "password123")),
        None,
        None,
    );
    let (a, b) = tokio::join!(register_a, register_b);
    assert_eq!(a.status, StatusCode::CREATED);
    assert_eq!(b.status, StatusCode::CREATED);

    let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(admins, 1, "exactly one of the racers may become admin");
}

#[tokio::test]
async fn migrations_down_up_roundtrip() {
    let data_dir = std::env::temp_dir().join(format!(
        "deckoala-test-{}-migrate-roundtrip",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&data_dir);
    let db = deckoala_server::init_db(&data_dir).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");

    migrator.undo(&db, 1).await.expect("undo to version 1");
    let users_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'users'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(users_table, 0, "down migration must drop the users table");

    migrator.run(&db).await.expect("re-apply migrations");
    let users_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'users'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(users_table, 1, "up migration must recreate the users table");
}
