//! Auth handlers: register / login / logout / me (BRIEF-0001).

use argon2::password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

use crate::{json_error, now_rfc3339, AppState};

pub(crate) const SESSION_USER_KEY: &str = "user_id";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: String,
    pub username: String,
    pub is_admin: bool,
    pub created_at: String,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
    is_admin: i64,
    created_at: String,
}

impl From<&UserRow> for UserDto {
    fn from(row: &UserRow) -> Self {
        Self {
            id: row.id.clone(),
            username: row.username.clone(),
            is_admin: row.is_admin != 0,
            created_at: row.created_at.clone(),
        }
    }
}

#[derive(Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

fn valid_username(username: &str) -> bool {
    (3..=32).contains(&username.len())
        && username
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Argon2id with explicitly pinned parameters (OWASP minimum: m=19456 KiB,
/// t=2, p=1) so the security posture never silently drifts with crate
/// defaults. Raise them here when hardware moves on.
fn argon2() -> Argon2<'static> {
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(19_456, 2, 1, None).expect("valid argon2 params"),
    )
}

/// CPU-bound, so run blocking.
async fn hash_password(password: String) -> Result<String, Response> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        argon2()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
    })
    .await
    .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "hashing task failed"))?
    .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed"))
}

async fn verify_password(password: String, password_hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&password_hash)
            .map(|parsed| {
                argon2()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

async fn start_session(session: &Session, user_id: &str) -> Result<(), Response> {
    // Cycle the id on privilege change (login/register) to prevent fixation.
    session
        .cycle_id()
        .await
        .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "session error"))?;
    session
        .insert(SESSION_USER_KEY, user_id.to_owned())
        .await
        .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "session error"))
}

/// Seed the bootstrap admin `root` when the instance has no users yet
/// (BRIEF-0010). Returns true when a row was actually created.
///
/// Called ONLY from `main.rs` startup — deliberately NOT from `init_db()`, so
/// the integration suite (which calls `init_db` and then relies on "the first
/// signup is the admin") keeps working unchanged.
///
/// `is_builtin_default` records whether the built-in default password was used,
/// so an operator who supplied `DECKOALA_ROOT_PASSWORD` never sees a bogus
/// "still on the default password" warning.
pub async fn seed_root(
    db: &sqlx::SqlitePool,
    password: &str,
    is_builtin_default: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let password_hash = hash_password(password.to_owned())
        .await
        .map_err(|_| "failed to hash the root password")?;
    // Single statement: never overwrites an existing account, and a concurrent
    // start can't produce two roots.
    let done = sqlx::query(
        "INSERT INTO users (id, username, password_hash, is_admin, created_at) \
         SELECT ?1, 'root', ?2, 1, ?3 WHERE (SELECT COUNT(*) FROM users) = 0",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&password_hash)
    .bind(now_rfc3339())
    .execute(db)
    .await?;
    let created = done.rows_affected() > 0;
    if created && is_builtin_default {
        crate::settings::set(db, crate::settings::ROOT_PW_DEFAULT, "true").await?;
    }
    Ok(created)
}

pub async fn register(
    State(state): State<AppState>,
    session: Session,
    Json(creds): Json<Credentials>,
) -> Response {
    let username = creds.username.trim().to_lowercase();
    if !valid_username(&username) {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "username must be 3-32 characters: a-z, 0-9, _ or -",
        );
    }
    if creds.password.chars().count() < 8 {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "password must be at least 8 characters",
        );
    }
    // Reject oversized input BEFORE hashing — unbounded input into argon2
    // is a denial-of-service vector (BRIEF-0001 business rules).
    if creds.password.len() > 128 {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "password must be at most 128 bytes",
        );
    }

    let password_hash = match hash_password(creds.password).await {
        Ok(hash) => hash,
        Err(response) => return response,
    };
    let id = Uuid::new_v4().to_string();
    let created_at = now_rfc3339();

    // Single-statement inserts keep the "first user is admin" decision atomic
    // (no separate COUNT read that a concurrent register could invalidate).
    let result = if state.allow_signup {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, is_admin, created_at) \
             VALUES (?1, ?2, ?3, (SELECT CASE WHEN COUNT(*) = 0 THEN 1 ELSE 0 END FROM users), ?4)",
        )
        .bind(&id)
        .bind(&username)
        .bind(&password_hash)
        .bind(&created_at)
        .execute(&state.db)
        .await
    } else {
        // Signup disabled: only the first-run bootstrap account may register.
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, is_admin, created_at) \
             SELECT ?1, ?2, ?3, 1, ?4 WHERE (SELECT COUNT(*) FROM users) = 0",
        )
        .bind(&id)
        .bind(&username)
        .bind(&password_hash)
        .bind(&created_at)
        .execute(&state.db)
        .await
    };

    match result {
        Ok(done) if done.rows_affected() == 0 => {
            json_error(StatusCode::FORBIDDEN, "signup is disabled on this instance")
        }
        Ok(_) => {
            let row: UserRow = match sqlx::query_as("SELECT * FROM users WHERE id = ?1")
                .bind(&id)
                .fetch_one(&state.db)
                .await
            {
                Ok(row) => row,
                Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
            };
            if let Err(response) = start_session(&session, &id).await {
                return response;
            }
            (StatusCode::CREATED, Json(UserDto::from(&row))).into_response()
        }
        Err(err) if is_unique_violation(&err) => {
            json_error(StatusCode::CONFLICT, "username already taken")
        }
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .map(|db_err| db_err.is_unique_violation())
        .unwrap_or(false)
}

pub async fn login(
    State(state): State<AppState>,
    session: Session,
    Json(creds): Json<Credentials>,
) -> Response {
    let username = creds.username.trim().to_lowercase();
    // Same cap as register, enforced before any argon2 work (DoS vector).
    // Uniform 401 body: the limit is public and the check is
    // username-independent, so this adds no enumeration oracle.
    if creds.password.len() > 128 {
        return json_error(StatusCode::UNAUTHORIZED, "invalid username or password");
    }
    let row: Option<UserRow> = match sqlx::query_as("SELECT * FROM users WHERE username = ?1")
        .bind(&username)
        .fetch_optional(&state.db)
        .await
    {
        Ok(row) => row,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };

    // Uniform failure path: verify against a dummy hash when the user is
    // unknown so response timing does not leak which usernames exist.
    const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$oPRRfevfWm6t7ZdKq/EPzKPCLBcXjcLoWEXfwXCfj9E";
    let verified = match &row {
        Some(user) => verify_password(creds.password, user.password_hash.clone()).await,
        None => {
            let _ = verify_password(creds.password, DUMMY_HASH.to_owned()).await;
            false
        }
    };

    match (verified, row) {
        (true, Some(user)) => {
            if let Err(response) = start_session(&session, &user.id).await {
                return response;
            }
            Json(UserDto::from(&user)).into_response()
        }
        _ => json_error(StatusCode::UNAUTHORIZED, "invalid username or password"),
    }
}

pub async fn logout(session: Session) -> Response {
    match session.flush().await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "session error"),
    }
}

/// `/me` only — keeping these off `UserDto` leaves the register/login response
/// shapes untouched, and keeps both flags strictly session-gated (they must
/// NEVER reach the anonymous `/api/instance`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeDto {
    #[serde(flatten)]
    user: UserDto,
    /// True while `root` still uses the built-in default password.
    root_password_is_default: bool,
    /// The ONLY AI signal a non-admin may read — provider/baseUrl/model stay
    /// admin-only because they can name internal hosts.
    ai_enabled: bool,
}

pub async fn me(State(state): State<AppState>, session: Session) -> Response {
    let user_id: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
    let Some(user_id) = user_id else {
        return json_error(StatusCode::UNAUTHORIZED, "not signed in");
    };
    match sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ?1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(row)) => {
            // Only an admin (who can act on it) learns that root still has the
            // default password — it is not useful to, and not for, other users.
            let is_admin = row.is_admin != 0;
            let root_password_is_default = is_admin
                && crate::settings::flag(&state.db, crate::settings::ROOT_PW_DEFAULT).await;
            Json(MeDto {
                user: UserDto::from(&row),
                root_password_is_default,
                ai_enabled: crate::settings::ai_is_usable(&state.db).await,
            })
            .into_response()
        }
        Ok(None) => json_error(StatusCode::UNAUTHORIZED, "not signed in"),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePassword {
    current_password: String,
    new_password: String,
}

/// Change the signed-in user's password. Lives here to reuse the private
/// argon2 helpers. Rotates the current session id; other sessions for the same
/// user stay valid until they expire (tower-sessions offers no per-user
/// enumeration — tracked as a deferred hardening).
pub async fn change_password(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<ChangePassword>,
) -> Response {
    let user_id: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
    let Some(user_id) = user_id else {
        return json_error(StatusCode::UNAUTHORIZED, "not signed in");
    };
    // Same bounds as register: reject oversized input BEFORE argon2.
    if body.current_password.len() > 128 || body.new_password.len() > 128 {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "password too long");
    }
    // Count CHARACTERS, matching register — a byte check would let a 3-glyph
    // Thai password through (3 bytes each) while register rejects it.
    if body.new_password.chars().count() < 8 {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "password must be at least 8 characters",
        );
    }

    let row = match sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ?1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return json_error(StatusCode::UNAUTHORIZED, "not signed in"),
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    if !verify_password(body.current_password, row.password_hash.clone()).await {
        return json_error(StatusCode::UNAUTHORIZED, "current password is incorrect");
    }

    let new_hash = match hash_password(body.new_password).await {
        Ok(hash) => hash,
        Err(response) => return response,
    };
    if sqlx::query("UPDATE users SET password_hash = ?1 WHERE id = ?2")
        .bind(&new_hash)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    // The default-password warning is about `root` specifically.
    if row.username.eq_ignore_ascii_case("root") {
        let _ = crate::settings::remove(&state.db, crate::settings::ROOT_PW_DEFAULT).await;
    }
    let _ = session.cycle_id().await;
    StatusCode::NO_CONTENT.into_response()
}
