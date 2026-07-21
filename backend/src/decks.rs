//! Deck CRUD handlers (BRIEF-0002).

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use uuid::Uuid;

use crate::{auth::SESSION_USER_KEY, json_error, now_rfc3339, AppState};

const MAX_MARKDOWN_BYTES: usize = 1_000_000;
const MAX_TITLE_CHARS: usize = 200;
const DEFAULT_TITLE: &str = "Untitled deck";

/// Extractor: the signed-in user's id from the session; 401 without one.
pub struct AuthUser(pub String);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| json_error(StatusCode::UNAUTHORIZED, "not signed in"))?;
        let user_id: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
        user_id
            .map(AuthUser)
            .ok_or_else(|| json_error(StatusCode::UNAUTHORIZED, "not signed in"))
    }
}

#[derive(sqlx::FromRow)]
struct DeckRow {
    id: String,
    title: String,
    markdown: String,
    theme: String,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct DeckMetaRow {
    id: String,
    title: String,
    theme: String,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeckFull {
    id: String,
    title: String,
    theme: String,
    markdown: String,
    created_at: String,
    updated_at: String,
}

impl From<DeckRow> for DeckFull {
    fn from(row: DeckRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            theme: row.theme,
            markdown: row.markdown,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeckMeta {
    id: String,
    title: String,
    theme: String,
    created_at: String,
    updated_at: String,
}

impl From<DeckMetaRow> for DeckMeta {
    fn from(row: DeckMetaRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            theme: row.theme,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// New decks start as standard Marp Markdown (ADR-0002 durable contract).
/// The `deckoala` theme CSS ships with the editor brief; Marp falls back to
/// its default theme until then.
fn default_template(title: &str) -> String {
    format!(
        "---\nmarp: true\ntheme: deckoala\npaginate: true\n---\n\n# {title}\n\n\
         Your first slide. Write Markdown, get slides.\n\n---\n\n\
         ## Math works too\n\n$$ E = mc^2 $$\n"
    )
}

enum TitleInput {
    Missing,
    Empty,
    TooLong,
    HasControlChars,
    Value(String),
}

/// Trim + classify a client-supplied title.
fn parse_title(input: Option<String>) -> TitleInput {
    match input {
        None => TitleInput::Missing,
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                TitleInput::Empty
            } else if trimmed.chars().any(char::is_control) {
                // CR/LF/TAB etc. would otherwise reach response headers
                // (export Content-Disposition) — reject at the source.
                TitleInput::HasControlChars
            } else if trimmed.chars().count() > MAX_TITLE_CHARS {
                TitleInput::TooLong
            } else {
                TitleInput::Value(trimmed.to_owned())
            }
        }
    }
}

fn title_too_long() -> Response {
    json_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "title must be at most 200 characters",
    )
}

fn title_has_controls() -> Response {
    json_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "title must not contain control characters",
    )
}

fn markdown_too_large(markdown: &Option<String>) -> bool {
    markdown
        .as_ref()
        .is_some_and(|m| m.len() > MAX_MARKDOWN_BYTES)
}

async fn fetch_deck(
    state: &AppState,
    id: &str,
    owner_id: &str,
) -> Result<Option<DeckRow>, Response> {
    sqlx::query_as::<_, DeckRow>(
        "SELECT id, title, markdown, theme, created_at, updated_at FROM decks \
         WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
}

fn not_found() -> Response {
    // Foreign decks are indistinguishable from nonexistent ones (BRIEF-0002).
    json_error(StatusCode::NOT_FOUND, "not found")
}

pub async fn list(State(state): State<AppState>, AuthUser(user_id): AuthUser) -> Response {
    match sqlx::query_as::<_, DeckMetaRow>(
        "SELECT id, title, theme, created_at, updated_at FROM decks \
         WHERE owner_id = ?1 AND deleted_at IS NULL ORDER BY updated_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => Json(rows.into_iter().map(DeckMeta::from).collect::<Vec<_>>()).into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

#[derive(Deserialize)]
pub struct CreateDeck {
    title: Option<String>,
    markdown: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<CreateDeck>,
) -> Response {
    let title = match parse_title(body.title) {
        TitleInput::Missing | TitleInput::Empty => DEFAULT_TITLE.to_owned(),
        TitleInput::TooLong => return title_too_long(),
        TitleInput::HasControlChars => return title_has_controls(),
        TitleInput::Value(value) => value,
    };
    if markdown_too_large(&body.markdown) {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "markdown too large (max 1 MB)",
        );
    }
    let markdown = body.markdown.unwrap_or_else(|| default_template(&title));

    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let inserted = sqlx::query(
        "INSERT INTO decks (id, owner_id, title, markdown, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
    )
    .bind(&id)
    .bind(&user_id)
    .bind(&title)
    .bind(&markdown)
    .bind(&now)
    .execute(&state.db)
    .await;
    if inserted.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    match fetch_deck(&state, &id, &user_id).await {
        Ok(Some(row)) => (StatusCode::CREATED, Json(DeckFull::from(row))).into_response(),
        _ => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

pub async fn get_one(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    match fetch_deck(&state, &id, &user_id).await {
        Ok(Some(row)) => Json(DeckFull::from(row)).into_response(),
        Ok(None) => not_found(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDeck {
    title: Option<String>,
    markdown: Option<String>,
    /// The `updatedAt` the client's edit was based on. Last-write-wins still
    /// applies, but a mismatch with changed markdown forces a snapshot even
    /// inside the throttle window — that is exactly the clobber case.
    base_updated_at: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateDeck>,
) -> Response {
    if body.title.is_none() && body.markdown.is_none() {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "nothing to update");
    }
    let title = match parse_title(body.title) {
        TitleInput::Missing => None,
        TitleInput::Empty => {
            return json_error(StatusCode::UNPROCESSABLE_ENTITY, "title must not be empty")
        }
        TitleInput::TooLong => return title_too_long(),
        TitleInput::HasControlChars => return title_has_controls(),
        TitleInput::Value(value) => Some(value),
    };
    if markdown_too_large(&body.markdown) {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "markdown too large (max 1 MB)",
        );
    }

    let now = now_rfc3339();
    // BEGIN IMMEDIATE via sqlx's Transaction API: takes the write lock up
    // front so concurrent autosaves serialize (deferred transactions would
    // hit SQLITE_BUSY_SNAPSHOT or double-snapshot), AND auto-rollbacks on
    // drop — a client disconnect cancelling this future can never leak an
    // open transaction back into the pool (BRIEF-0003).
    let mut tx = match state.db.begin_with("BEGIN IMMEDIATE").await {
        Ok(tx) => tx,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    let outcome = update_in_tx(
        &mut tx,
        &state,
        &id,
        &user_id,
        &title,
        &body.markdown,
        body.base_updated_at.as_deref(),
        &now,
    )
    .await;
    finish_deck_tx(tx, outcome).await
}

/// Commit + 200 on success, rollback + 404/500 otherwise.
async fn finish_deck_tx(
    tx: sqlx::Transaction<'_, sqlx::Sqlite>,
    outcome: Result<Option<DeckRow>, sqlx::Error>,
) -> Response {
    match outcome {
        Ok(Some(row)) => {
            if tx.commit().await.is_err() {
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
            }
            Json(DeckFull::from(row)).into_response()
        }
        Ok(None) => {
            let _ = tx.rollback().await;
            not_found()
        }
        Err(_) => {
            let _ = tx.rollback().await;
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn update_in_tx(
    conn: &mut sqlx::SqliteConnection,
    state: &AppState,
    id: &str,
    user_id: &str,
    title: &Option<String>,
    markdown: &Option<String>,
    base_updated_at: Option<&str>,
    now: &str,
) -> Result<Option<DeckRow>, sqlx::Error> {
    let current: Option<DeckRow> = sqlx::query_as(
        "SELECT id, title, markdown, theme, created_at, updated_at FROM decks \
         WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(current) = current else {
        return Ok(None);
    };

    // Snapshot policy (BRIEF-0003): before applying a real markdown change,
    // snapshot the PRE-update content when the throttle window has elapsed
    // OR the client edited from a stale baseline (the clobber case).
    let markdown_changed = markdown
        .as_ref()
        .is_some_and(|new| *new != current.markdown);
    let stale_base = base_updated_at.is_some_and(|base| base != current.updated_at);
    if markdown_changed
        && (stale_base
            || latest_revision_older_than(&mut *conn, id, state.revision_min_secs).await?)
    {
        insert_revision(&mut *conn, id, &current.markdown, now).await?;
    }

    let done = sqlx::query(
        "UPDATE decks SET title = COALESCE(?1, title), markdown = COALESCE(?2, markdown), \
         updated_at = ?3 WHERE id = ?4 AND owner_id = ?5 AND deleted_at IS NULL",
    )
    .bind(title)
    .bind(markdown)
    .bind(now)
    .bind(id)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    if done.rows_affected() == 0 {
        return Ok(None); // tombstoned mid-flight → rollback, no stray snapshot
    }

    sqlx::query_as(
        "SELECT id, title, markdown, theme, created_at, updated_at FROM decks \
         WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await
}

const MAX_REVISIONS_PER_DECK: i64 = 50;

/// True when the deck's newest revision is older than `min_secs` (or there
/// is none). Unparsable timestamps count as old — snapshotting too often is
/// safer than never.
async fn latest_revision_older_than(
    tx: &mut sqlx::SqliteConnection,
    deck_id: &str,
    min_secs: i64,
) -> Result<bool, sqlx::Error> {
    let latest: Option<String> = sqlx::query_scalar(
        "SELECT created_at FROM revisions WHERE deck_id = ?1 \
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )
    .bind(deck_id)
    .fetch_optional(&mut *tx)
    .await?;
    Ok(match latest {
        None => true,
        Some(ts) => {
            match time::OffsetDateTime::parse(&ts, &time::format_description::well_known::Rfc3339) {
                Ok(parsed) => {
                    time::OffsetDateTime::now_utc() - parsed >= time::Duration::seconds(min_secs)
                }
                Err(_) => true,
            }
        }
    })
}

/// Insert a snapshot and prune to the newest MAX_REVISIONS_PER_DECK rows.
async fn insert_revision(
    tx: &mut sqlx::SqliteConnection,
    deck_id: &str,
    markdown: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO revisions (id, deck_id, markdown, created_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(deck_id)
    .bind(markdown)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM revisions WHERE deck_id = ?1 AND id NOT IN (\
         SELECT id FROM revisions WHERE deck_id = ?1 \
         ORDER BY created_at DESC, rowid DESC LIMIT ?2)",
    )
    .bind(deck_id)
    .bind(MAX_REVISIONS_PER_DECK)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct RevisionMetaRow {
    id: String,
    created_at: String,
    size_bytes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionMeta {
    id: String,
    created_at: String,
    size_bytes: i64,
}

pub async fn revisions_list(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    match fetch_deck(&state, &id, &user_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found(),
        Err(response) => return response,
    }
    match sqlx::query_as::<_, RevisionMetaRow>(
        "SELECT id, created_at, LENGTH(CAST(markdown AS BLOB)) AS size_bytes \
         FROM revisions WHERE deck_id = ?1 ORDER BY created_at DESC, rowid DESC",
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| RevisionMeta {
                    id: row.id,
                    created_at: row.created_at,
                    size_bytes: row.size_bytes,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

#[derive(sqlx::FromRow)]
struct RevisionRow {
    id: String,
    markdown: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionFull {
    id: String,
    created_at: String,
    markdown: String,
}

async fn fetch_revision(
    db: &sqlx::SqlitePool,
    deck_id: &str,
    rev_id: &str,
) -> Result<Option<RevisionRow>, sqlx::Error> {
    sqlx::query_as::<_, RevisionRow>(
        "SELECT id, markdown, created_at FROM revisions WHERE id = ?1 AND deck_id = ?2",
    )
    .bind(rev_id)
    .bind(deck_id)
    .fetch_optional(db)
    .await
}

pub async fn revision_get(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path((id, rev_id)): Path<(String, String)>,
) -> Response {
    match fetch_deck(&state, &id, &user_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found(),
        Err(response) => return response,
    }
    match fetch_revision(&state.db, &id, &rev_id).await {
        Ok(Some(row)) => Json(RevisionFull {
            id: row.id,
            created_at: row.created_at,
            markdown: row.markdown,
        })
        .into_response(),
        Ok(None) => not_found(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

pub async fn revision_restore(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path((id, rev_id)): Path<(String, String)>,
) -> Response {
    let now = now_rfc3339();
    let mut tx = match state.db.begin_with("BEGIN IMMEDIATE").await {
        Ok(tx) => tx,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    let outcome = restore_in_tx(&mut tx, &id, &user_id, &rev_id, &now).await;
    finish_deck_tx(tx, outcome).await
}

async fn restore_in_tx(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
    user_id: &str,
    rev_id: &str,
    now: &str,
) -> Result<Option<DeckRow>, sqlx::Error> {
    let current: Option<DeckRow> = sqlx::query_as(
        "SELECT id, title, markdown, theme, created_at, updated_at FROM decks \
         WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(current) = current else {
        return Ok(None);
    };
    // deck_id bound alongside the revision id — a revision from another deck
    // must never resolve (BRIEF-0003 cross-deck rule).
    let revision: Option<RevisionRow> = sqlx::query_as(
        "SELECT id, markdown, created_at FROM revisions WHERE id = ?1 AND deck_id = ?2",
    )
    .bind(rev_id)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(revision) = revision else {
        return Ok(None);
    };

    // Restore is explicit — always snapshot the current content first.
    insert_revision(&mut *conn, id, &current.markdown, now).await?;
    let done = sqlx::query(
        "UPDATE decks SET markdown = ?1, updated_at = ?2 \
         WHERE id = ?3 AND owner_id = ?4 AND deleted_at IS NULL",
    )
    .bind(&revision.markdown)
    .bind(now)
    .bind(id)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    if done.rows_affected() == 0 {
        return Ok(None); // tombstoned mid-flight → rollback incl. the snapshot
    }

    sqlx::query_as(
        "SELECT id, title, markdown, theme, created_at, updated_at FROM decks \
         WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await
}

pub async fn remove(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    let now = now_rfc3339();
    match sqlx::query(
        "UPDATE decks SET deleted_at = ?1 WHERE id = ?2 AND owner_id = ?3 AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    {
        Ok(done) if done.rows_affected() == 0 => not_found(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

pub async fn duplicate(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    let source = match fetch_deck(&state, &id, &user_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };

    let new_id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    // Truncate the base so title + " (copy)" never breaks the 200-char cap,
    // even through repeated duplication.
    const COPY_SUFFIX: &str = " (copy)";
    let base: String = source
        .title
        .chars()
        .take(MAX_TITLE_CHARS - COPY_SUFFIX.chars().count())
        .collect();
    let title = format!("{}{COPY_SUFFIX}", base.trim_end());
    let inserted = sqlx::query(
        "INSERT INTO decks (id, owner_id, title, markdown, theme, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
    )
    .bind(&new_id)
    .bind(&user_id)
    .bind(&title)
    .bind(&source.markdown)
    .bind(&source.theme)
    .bind(&now)
    .execute(&state.db)
    .await;
    if inserted.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    match fetch_deck(&state, &new_id, &user_id).await {
        Ok(Some(row)) => (StatusCode::CREATED, Json(DeckFull::from(row))).into_response(),
        _ => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

pub async fn export(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    let deck = match fetch_deck(&state, &id, &user_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };

    let disposition = content_disposition(&deck.title);
    let mut response = (StatusCode::OK, deck.markdown).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    response
}

/// ASCII fallback + RFC 5987 `filename*` so non-Latin titles (e.g. Thai)
/// survive. Chars outside the safe set are REMOVED (an all-Thai title falls
/// back to "deck"), and both parts are built only from filtered/encoded
/// bytes, so no header-injection path exists (quotes, CR/LF never emitted).
fn content_disposition(title: &str) -> String {
    let ascii: String = title
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        .collect();
    let ascii = ascii.trim().trim_matches('.').to_owned();
    let ascii = if ascii.is_empty() {
        "deck".to_owned()
    } else {
        ascii
    };
    format!(
        "attachment; filename=\"{ascii}.md\"; filename*=UTF-8''{}.md",
        percent_encode(title)
    )
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{content_disposition, percent_encode};

    #[test]
    fn disposition_is_header_safe_for_hostile_titles() {
        let hostile = "evil\"\r\nSet-Cookie: x=y";
        let disposition = content_disposition(hostile);
        assert!(!disposition.contains('\r'));
        assert!(!disposition.contains('\n'));
        assert!(!disposition.contains("evil\""));
        assert!(disposition.starts_with("attachment; filename=\"evil"));
    }

    #[test]
    fn thai_title_survives_in_filename_star() {
        let disposition = content_disposition("สไลด์ของฉัน");
        assert!(disposition.contains("filename*=UTF-8''%E0%B8%AA"));
        assert!(disposition.contains("filename=\"deck.md\""));
    }

    #[test]
    fn percent_encoding_is_uppercase_hex() {
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("ก"), "%E0%B8%81");
    }
}
