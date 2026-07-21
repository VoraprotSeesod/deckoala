//! Per-user API tokens for the MCP endpoint (BRIEF-0011).
//!
//! Only a SHA-256 hash is stored: these are 256-bit random secrets, so a fast
//! hash is the right tool (argon2 exists for low-entropy passwords) and lookup
//! is one indexed match. The plaintext is returned by exactly ONE endpoint,
//! once, at creation — never listed, never logged.

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::decks::AuthUser;
use crate::{json_error, now_rfc3339, AppState};

const TOKEN_PREFIX: &str = "dko_";
const MAX_TOKENS_PER_USER: i64 = 20;
const MAX_NAME_CHARS: usize = 60;

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// `dko_` + 256 bits of randomness (64 hex chars).
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let mut out = String::with_capacity(TOKEN_PREFIX.len() + 64);
    out.push_str(TOKEN_PREFIX);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// --- extractor -------------------------------------------------------------

/// Resolves `Authorization: Bearer <token>` to the owning user id.
///
/// Deliberately returns a plain JSON 401 with NO `WWW-Authenticate: Bearer`
/// header — that header sends MCP clients into OAuth discovery, and those
/// `/.well-known/*` probes would otherwise be answered by the SPA fallback
/// with an HTML 200, which confuses the client badly (BRIEF-0011 review).
pub struct McpUser(pub String);

fn unauthorized() -> Response {
    json_error(StatusCode::UNAUTHORIZED, "invalid or missing API token")
}

impl FromRequestParts<AppState> for McpUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let presented = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or_else(unauthorized)?;

        let hash = hash_token(presented);
        let user_id: Option<String> = sqlx::query_scalar(
            "SELECT user_id FROM api_tokens WHERE token_hash = ?1 AND revoked_at IS NULL",
        )
        .bind(&hash)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let user_id = user_id.ok_or_else(unauthorized)?;

        // Best-effort usage stamp; never fail the request over it.
        let _ = sqlx::query("UPDATE api_tokens SET last_used_at = ?1 WHERE token_hash = ?2")
            .bind(now_rfc3339())
            .bind(&hash)
            .execute(&state.db)
            .await;

        Ok(McpUser(user_id))
    }
}

// --- owner API (session-authenticated: a token can never mint another) ------

#[derive(sqlx::FromRow)]
struct TokenRow {
    id: String,
    name: String,
    created_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenView {
    id: String,
    name: String,
    created_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
}

impl From<TokenRow> for TokenView {
    fn from(row: TokenRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
            revoked_at: row.revoked_at,
        }
    }
}

/// The ONLY shape that ever carries the plaintext token.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedToken {
    id: String,
    name: String,
    created_at: String,
    token: String,
}

#[derive(Deserialize)]
pub struct CreateToken {
    name: String,
}

fn valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_NAME_CHARS
        && !trimmed.chars().any(char::is_control)
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<CreateToken>,
) -> Response {
    let name = body.name.trim().to_owned();
    if !valid_name(&name) {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "name must be 1-60 characters without control characters",
        );
    }
    // Keep the table bounded per user.
    let active: i64 = match sqlx::query_scalar(
        "SELECT COUNT(*) FROM api_tokens WHERE user_id = ?1 AND revoked_at IS NULL",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(count) => count,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    if active >= MAX_TOKENS_PER_USER {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "too many active tokens — revoke one first",
        );
    }

    let id = Uuid::new_v4().to_string();
    let token = generate_token();
    let created_at = now_rfc3339();
    if sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&id)
    .bind(&user_id)
    .bind(&name)
    .bind(hash_token(&token))
    .bind(&created_at)
    .execute(&state.db)
    .await
    .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    (
        StatusCode::CREATED,
        Json(CreatedToken {
            id,
            name,
            created_at,
            token,
        }),
    )
        .into_response()
}

pub async fn list(State(state): State<AppState>, AuthUser(user_id): AuthUser) -> Response {
    match sqlx::query_as::<_, TokenRow>(
        "SELECT id, name, created_at, last_used_at, revoked_at FROM api_tokens \
         WHERE user_id = ?1 ORDER BY created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => Json(rows.into_iter().map(TokenView::from).collect::<Vec<_>>()).into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

pub async fn revoke(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    // Double-scoped (id AND user_id) so one user can never revoke another's
    // token by guessing an id.
    let exists: Option<i64> =
        match sqlx::query_scalar("SELECT 1 FROM api_tokens WHERE id = ?1 AND user_id = ?2")
            .bind(&id)
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
        {
            Ok(row) => row,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
        };
    if exists.is_none() {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    if sqlx::query(
        "UPDATE api_tokens SET revoked_at = ?1 \
         WHERE id = ?2 AND user_id = ?3 AND revoked_at IS NULL",
    )
    .bind(now_rfc3339())
    .bind(&id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use super::{generate_token, hash_token, valid_name, TOKEN_PREFIX};

    #[test]
    fn tokens_are_prefixed_and_high_entropy() {
        let a = generate_token();
        let b = generate_token();
        assert!(a.starts_with(TOKEN_PREFIX));
        assert_eq!(a.len(), TOKEN_PREFIX.len() + 64);
        assert_ne!(a, b, "tokens must not repeat");
    }

    #[test]
    fn hashing_is_stable_and_not_the_token() {
        let token = generate_token();
        let hash = hash_token(&token);
        assert_eq!(hash, hash_token(&token));
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains(&token));
    }

    #[test]
    fn name_validation() {
        assert!(valid_name("Claude Desktop"));
        assert!(!valid_name(""));
        assert!(!valid_name("   "));
        assert!(!valid_name("bad\nname"));
        assert!(!valid_name(&"x".repeat(61)));
        assert!(valid_name(&"x".repeat(60)));
    }
}
