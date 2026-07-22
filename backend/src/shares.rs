//! Share links: view/edit tokens granting anonymous access to ONE deck (BRIEF-0008).
//!
//! An owner mints a `share_links` row for a deck they own; anyone holding the
//! token opens `/s/{token}` with no account. A token authorizes EXACTLY its
//! `deck_id` — every token-side query is scoped to that deck and re-checks the
//! share is active (not revoked, not expired) and the deck is live.

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;

use crate::decks::{self, AuthUser, UpdateDeck};
use crate::{json_error, now_rfc3339, AppState};

/// Per-deck share cookie name prefix: `deckoala_share_{deckId}`. Scoping the
/// name (and Path) to the deck lets a viewer hold cookies for several shared
/// decks at once without one overwriting another (BRIEF-0008 review).
pub(crate) const SHARE_COOKIE_PREFIX: &str = "deckoala_share_";

fn not_found() -> Response {
    // Unknown/revoked/expired tokens are indistinguishable from nonexistent
    // ones — no existence oracle.
    json_error(StatusCode::NOT_FOUND, "not found")
}

/// 256-bit URL-safe token (64 hex chars).
fn gen_token() -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Parse a client `expiresAt` and re-serialize it to canonical fixed-width UTC
/// (`now_rfc3339()` format) so the `expires_at > now` active-check compares as
/// TEXT soundly. RFC3339 REQUIRES an offset, so naive/offsetless input fails to
/// parse and is rejected — the expiry control cannot silently fail open.
fn canonicalize_expiry(raw: &str) -> Option<String> {
    let dt = OffsetDateTime::parse(raw.trim(), &Rfc3339).ok()?;
    let utc = dt.to_offset(UtcOffset::UTC);
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        utc.year(),
        u8::from(utc.month()),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second(),
        utc.microsecond()
    ))
}

async fn owner_owns_deck(state: &AppState, deck_id: &str, user_id: &str) -> Result<bool, Response> {
    sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM decks WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(deck_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map(|row| row.is_some())
    .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
}

// --- owner: mint / list / revoke ---

#[derive(sqlx::FromRow)]
struct ShareRow {
    id: String,
    token: String,
    permission: String,
    created_at: String,
    expires_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareDto {
    id: String,
    token: String,
    permission: String,
    /// Convenience for the owner UI (`/s/{token}`).
    url: String,
    created_at: String,
    expires_at: Option<String>,
    revoked_at: Option<String>,
    /// `active` | `revoked` | `expired`, computed against the current time.
    status: String,
}

fn share_dto(row: ShareRow, now: &str) -> ShareDto {
    let status = if row.revoked_at.is_some() {
        "revoked"
    } else if row.expires_at.as_deref().is_some_and(|e| e <= now) {
        "expired"
    } else {
        "active"
    };
    ShareDto {
        url: format!("/s/{}", row.token),
        id: row.id,
        token: row.token,
        permission: row.permission,
        created_at: row.created_at,
        expires_at: row.expires_at,
        revoked_at: row.revoked_at,
        status: status.to_owned(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateShare {
    permission: String,
    #[serde(default)]
    expires_at: Option<String>,
}

pub async fn create_share(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(deck_id): Path<String>,
    Json(body): Json<CreateShare>,
) -> Response {
    match owner_owns_deck(&state, &deck_id, &user_id).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(response) => return response,
    }
    if body.permission != "view" && body.permission != "edit" {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "permission must be 'view' or 'edit'",
        );
    }
    let expires_at = match body.expires_at.as_deref() {
        None | Some("") => None,
        Some(raw) => match canonicalize_expiry(raw) {
            Some(ts) => Some(ts),
            None => {
                return json_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "expiresAt must be an RFC3339 timestamp with a timezone offset",
                )
            }
        },
    };

    let id = Uuid::new_v4().to_string();
    let token = gen_token();
    let now = now_rfc3339();
    let inserted = sqlx::query(
        "INSERT INTO share_links (id, deck_id, token, permission, created_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(&deck_id)
    .bind(&token)
    .bind(&body.permission)
    .bind(&now)
    .bind(&expires_at)
    .execute(&state.db)
    .await;
    if inserted.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    let dto = share_dto(
        ShareRow {
            id,
            token,
            permission: body.permission,
            created_at: now.clone(),
            expires_at,
            revoked_at: None,
        },
        &now,
    );
    (StatusCode::CREATED, Json(dto)).into_response()
}

pub async fn list_shares(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(deck_id): Path<String>,
) -> Response {
    match owner_owns_deck(&state, &deck_id, &user_id).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(response) => return response,
    }
    let now = now_rfc3339();
    match sqlx::query_as::<_, ShareRow>(
        "SELECT id, token, permission, created_at, expires_at, revoked_at \
         FROM share_links WHERE deck_id = ?1 ORDER BY created_at DESC",
    )
    .bind(&deck_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| share_dto(row, &now))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

pub async fn revoke_share(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path((deck_id, share_id)): Path<(String, String)>,
) -> Response {
    match owner_owns_deck(&state, &deck_id, &user_id).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(response) => return response,
    }
    // Double-scoped (id AND deck_id) so revoke can only touch a link on a deck
    // the caller owns — never another owner's link by guessed id.
    let exists: Option<i64> =
        match sqlx::query_scalar("SELECT 1 FROM share_links WHERE id = ?1 AND deck_id = ?2")
            .bind(&share_id)
            .bind(&deck_id)
            .fetch_optional(&state.db)
            .await
        {
            Ok(row) => row,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
        };
    if exists.is_none() {
        return not_found();
    }
    // Idempotent: already-revoked links stay revoked and still return 204.
    if sqlx::query(
        "UPDATE share_links SET revoked_at = ?1 \
         WHERE id = ?2 AND deck_id = ?3 AND revoked_at IS NULL",
    )
    .bind(now_rfc3339())
    .bind(&share_id)
    .bind(&deck_id)
    .execute(&state.db)
    .await
    .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    StatusCode::NO_CONTENT.into_response()
}

// --- token resolution ---

struct ActiveShare {
    deck_id: String,
    permission: String,
}

/// Resolve a token to its ACTIVE share: not revoked, not past its (canonical)
/// expiry, and the target deck still live. Anything else → None (uniform 404).
async fn resolve_active(state: &AppState, token: &str) -> Result<Option<ActiveShare>, Response> {
    let now = now_rfc3339();
    sqlx::query_as::<_, (String, String)>(
        "SELECT s.deck_id, s.permission FROM share_links s \
         JOIN decks d ON d.id = s.deck_id \
         WHERE s.token = ?1 AND s.revoked_at IS NULL \
           AND (s.expires_at IS NULL OR s.expires_at > ?2) \
           AND d.deleted_at IS NULL",
    )
    .bind(token)
    .bind(&now)
    .fetch_optional(&state.db)
    .await
    .map(|opt| {
        opt.map(|(deck_id, permission)| ActiveShare {
            deck_id,
            permission,
        })
    })
    .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
}

/// Whether `token` is an active share for exactly `deck_id` (used by the asset
/// serve route's share-cookie authorizer).
pub(crate) async fn token_grants_deck(state: &AppState, token: &str, deck_id: &str) -> bool {
    matches!(
        resolve_active(state, token).await,
        Ok(Some(share)) if share.deck_id == deck_id
    )
}

/// Resolve a token that must grant EDIT: None → 404, view → 403, edit → deck_id.
async fn resolve_editable(state: &AppState, token: &str) -> Result<String, Response> {
    match resolve_active(state, token).await {
        Ok(Some(share)) if share.permission == "edit" => Ok(share.deck_id),
        Ok(Some(_)) => Err(json_error(
            StatusCode::FORBIDDEN,
            "this share link is view-only",
        )),
        Ok(None) => Err(not_found()),
        Err(response) => Err(response),
    }
}

// --- token-side deck access (anonymous) ---

#[derive(sqlx::FromRow)]
struct SharedDeckRow {
    id: String,
    title: String,
    theme: String,
    markdown: String,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedDeck {
    id: String,
    title: String,
    theme: String,
    markdown: String,
    created_at: String,
    updated_at: String,
    permission: String,
}

/// Set the per-deck share cookie, scoped to this deck's asset path so the
/// browser sends it only for `/assets/{deckId}/*` (used by the serve route to
/// authorize an anonymous viewer's images).
fn set_share_cookie(response: &mut Response, deck_id: &str, token: &str, secure: bool) {
    let mut value = format!(
        "{SHARE_COOKIE_PREFIX}{deck_id}={token}; Path=/assets/{deck_id}; HttpOnly; SameSite=Lax"
    );
    if secure {
        value.push_str("; Secure");
    }
    // deck_id (uuid) and token (hex) contain no header-special bytes.
    if let Ok(header) = HeaderValue::from_str(&value) {
        response.headers_mut().append(header::SET_COOKIE, header);
    }
}

pub async fn shared_get(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let share = match resolve_active(&state, &token).await {
        Ok(Some(share)) => share,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    let deck: Option<SharedDeckRow> = match sqlx::query_as(
        "SELECT id, title, theme, markdown, created_at, updated_at \
         FROM decks WHERE id = ?1 AND deleted_at IS NULL",
    )
    .bind(&share.deck_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(deck) => deck,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    let Some(deck) = deck else {
        return not_found();
    };
    let mut response = Json(SharedDeck {
        id: deck.id,
        title: deck.title,
        theme: deck.theme,
        markdown: deck.markdown,
        created_at: deck.created_at,
        updated_at: deck.updated_at,
        permission: share.permission,
    })
    .into_response();
    set_share_cookie(&mut response, &share.deck_id, &token, state.secure_cookie);
    response
}

pub async fn shared_update(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<UpdateDeck>,
) -> Response {
    let deck_id = match resolve_editable(&state, &token).await {
        Ok(deck_id) => deck_id,
        Err(response) => return response,
    };
    let owner = match decks::live_deck_owner(&state, &deck_id).await {
        Ok(Some(owner)) => owner,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    decks::update_deck_core(&state, &deck_id, &owner, body).await
}

pub async fn shared_revisions_list(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    let deck_id = match resolve_editable(&state, &token).await {
        Ok(deck_id) => deck_id,
        Err(response) => return response,
    };
    let owner = match decks::live_deck_owner(&state, &deck_id).await {
        Ok(Some(owner)) => owner,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    decks::revisions_list_core(&state, &deck_id, &owner).await
}

pub async fn shared_revision_get(
    State(state): State<AppState>,
    Path((token, rev_id)): Path<(String, String)>,
) -> Response {
    let deck_id = match resolve_editable(&state, &token).await {
        Ok(deck_id) => deck_id,
        Err(response) => return response,
    };
    let owner = match decks::live_deck_owner(&state, &deck_id).await {
        Ok(Some(owner)) => owner,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    decks::revision_get_core(&state, &deck_id, &owner, &rev_id).await
}

pub async fn shared_revision_restore(
    State(state): State<AppState>,
    Path((token, rev_id)): Path<(String, String)>,
) -> Response {
    let deck_id = match resolve_editable(&state, &token).await {
        Ok(deck_id) => deck_id,
        Err(response) => return response,
    };
    let owner = match decks::live_deck_owner(&state, &deck_id).await {
        Ok(Some(owner)) => owner,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    decks::revision_restore_core(&state, &deck_id, &owner, &rev_id).await
}

pub async fn shared_asset_upload(
    State(state): State<AppState>,
    Path(token): Path<String>,
    multipart: Multipart,
) -> Response {
    let deck_id = match resolve_editable(&state, &token).await {
        Ok(deck_id) => deck_id,
        Err(response) => return response,
    };
    crate::assets::store_asset(&state, &deck_id, multipart).await
}

/// `GET /api/s/{token}/assets` — list the deck's assets over an EDIT share
/// (view-only → 403, unknown/expired → 404), for the image picker's reuse grid.
pub async fn shared_asset_list(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    let deck_id = match resolve_editable(&state, &token).await {
        Ok(deck_id) => deck_id,
        Err(response) => return response,
    };
    crate::assets::list_assets(&state, &deck_id).await
}

pub async fn shared_export_pdf(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    // PDF export is available to view AND edit links.
    let deck_id = match resolve_active(&state, &token).await {
        Ok(Some(share)) => share.deck_id,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    // Anonymous exports use their own tighter semaphore so a leaked edit link's
    // cache-busting renders cannot starve owner exports.
    crate::export::render_deck_pdf(&state, &deck_id, &state.share_export_sem).await
}

pub async fn shared_export_md(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    let deck_id = match resolve_active(&state, &token).await {
        Ok(Some(share)) => share.deck_id,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    let owner = match decks::live_deck_owner(&state, &deck_id).await {
        Ok(Some(owner)) => owner,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    decks::md_export_core(&state, &deck_id, &owner).await
}

#[cfg(test)]
mod tests {
    use super::canonicalize_expiry;

    #[test]
    fn canonicalizes_offset_expiry_to_utc() {
        // +07:00 18:00 is 11:00Z; must normalize so TEXT comparison is sound.
        let out = canonicalize_expiry("2026-07-21T18:00:00+07:00").unwrap();
        assert_eq!(out, "2026-07-21T11:00:00.000000Z");
        // A `Z` value round-trips to the canonical fixed-width form.
        assert_eq!(
            canonicalize_expiry("2026-07-21T11:00:00Z").unwrap(),
            "2026-07-21T11:00:00.000000Z"
        );
    }

    #[test]
    fn rejects_naive_and_garbage_expiry() {
        // No offset → RFC3339 parse fails → rejected (cannot fail open).
        assert!(canonicalize_expiry("2026-07-21T18:00:00").is_none());
        assert!(canonicalize_expiry("tomorrow").is_none());
        assert!(canonicalize_expiry("").is_none());
    }
}
