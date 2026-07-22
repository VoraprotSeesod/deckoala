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

/// Extractor: a signed-in ADMIN (401 without a session, 403 when not admin).
/// Instance-level resources (fonts) are admin-managed (Q5 [STD]).
pub struct AdminUser(pub String);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(user_id) = AuthUser::from_request_parts(parts, state).await?;
        let is_admin: Option<i64> = sqlx::query_scalar("SELECT is_admin FROM users WHERE id = ?1")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
        if is_admin == Some(1) {
            Ok(AdminUser(user_id))
        } else {
            Err(json_error(StatusCode::FORBIDDEN, "admin only"))
        }
    }
}

// `pub(crate)` (with pub(crate) fields) so the data helpers below can be
// pub(crate) too — a pub(crate) fn returning a module-private type trips
// `private_interfaces`, which `-D warnings` turns into a build failure.
#[derive(sqlx::FromRow)]
pub(crate) struct DeckRow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) markdown: String,
    pub(crate) theme: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(sqlx::FromRow)]
pub(crate) struct DeckMetaRow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) theme: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

/// Typed failure for the data-level helpers. Validation lives WITH the data
/// operation so every caller — the HTTP handlers, share-token routes and the
/// MCP tools — enforces the same bounds; only the presentation differs.
pub(crate) enum DeckError {
    NotFound,
    NothingToUpdate,
    TitleEmpty,
    TitleTooLong,
    TitleControlChars,
    MarkdownTooLarge,
    Db,
}

impl DeckError {
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::NotFound => "not found",
            Self::NothingToUpdate => "nothing to update",
            Self::TitleEmpty => "title must not be empty",
            Self::TitleTooLong => "title must be at most 200 characters",
            Self::TitleControlChars => "title must not contain control characters",
            Self::MarkdownTooLarge => "markdown too large (max 1 MB)",
            Self::Db => "database error",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Db => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

impl IntoResponse for DeckError {
    fn into_response(self) -> Response {
        json_error(self.status(), self.message())
    }
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

const DEFAULT_THEME: &str = "deckoala";

/// Derive the deck's theme from the first active `theme:` frontmatter line, so
/// the stored `theme` column always agrees with what renders (BRIEF-0009c). A
/// commented or indented `theme:` is ignored; quotes and trailing comments are
/// stripped. Falls back to the default when there is no frontmatter directive.
fn theme_from_markdown(md: &str) -> String {
    let mut lines = md.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return DEFAULT_THEME.to_owned();
    }
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            break; // end of frontmatter
        }
        // Active top-level key only: no leading whitespace, not a comment.
        if line.starts_with(char::is_whitespace) || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("theme:") {
            let mut value = rest.trim();
            if let Some(idx) = value.find(" #") {
                value = value[..idx].trim();
            }
            value = value.trim_matches(|c| c == '"' || c == '\'').trim();
            if value.is_empty() {
                break;
            }
            return value.to_owned();
        }
    }
    DEFAULT_THEME.to_owned()
}

/// New decks start as standard Marp Markdown (ADR-0002 durable contract).
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
    match list_decks_data(&state, &user_id).await {
        Ok(rows) => Json(rows.into_iter().map(DeckMeta::from).collect::<Vec<_>>()).into_response(),
        Err(e) => e.into_response(),
    }
}

/// The caller's decks, newest-updated first. Shared by the HTTP list and MCP.
pub(crate) async fn list_decks_data(
    state: &AppState,
    owner_id: &str,
) -> Result<Vec<DeckMetaRow>, DeckError> {
    sqlx::query_as::<_, DeckMetaRow>(
        "SELECT id, title, theme, created_at, updated_at FROM decks \
         WHERE owner_id = ?1 AND deleted_at IS NULL ORDER BY updated_at DESC",
    )
    .bind(owner_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| DeckError::Db)
}

/// One deck owned by `owner_id`. A foreign deck is indistinguishable from a
/// missing one.
pub(crate) async fn fetch_deck_data(
    state: &AppState,
    id: &str,
    owner_id: &str,
) -> Result<DeckRow, DeckError> {
    sqlx::query_as::<_, DeckRow>(
        "SELECT id, title, markdown, theme, created_at, updated_at FROM decks \
         WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| DeckError::Db)?
    .ok_or(DeckError::NotFound)
}

/// Validate a title/markdown pair the same way for every caller.
fn check_title(input: Option<String>) -> Result<Option<String>, DeckError> {
    match parse_title(input) {
        TitleInput::Missing => Ok(None),
        TitleInput::Empty => Err(DeckError::TitleEmpty),
        TitleInput::TooLong => Err(DeckError::TitleTooLong),
        TitleInput::HasControlChars => Err(DeckError::TitleControlChars),
        TitleInput::Value(value) => Ok(Some(value)),
    }
}

/// Create a deck for `owner_id`. Validation lives HERE (not in the HTTP
/// wrapper) so MCP cannot write rows the HTTP path would reject.
pub(crate) async fn create_deck_data(
    state: &AppState,
    owner_id: &str,
    title: Option<String>,
    markdown: Option<String>,
) -> Result<DeckRow, DeckError> {
    let title = match parse_title(title) {
        TitleInput::Missing | TitleInput::Empty => DEFAULT_TITLE.to_owned(),
        TitleInput::TooLong => return Err(DeckError::TitleTooLong),
        TitleInput::HasControlChars => return Err(DeckError::TitleControlChars),
        TitleInput::Value(value) => value,
    };
    if markdown_too_large(&markdown) {
        return Err(DeckError::MarkdownTooLarge);
    }
    let markdown = markdown.unwrap_or_else(|| default_template(&title));

    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let theme = theme_from_markdown(&markdown);
    sqlx::query(
        "INSERT INTO decks (id, owner_id, title, markdown, theme, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
    )
    .bind(&id)
    .bind(owner_id)
    .bind(&title)
    .bind(&markdown)
    .bind(&theme)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|_| DeckError::Db)?;
    fetch_deck_data(state, &id, owner_id).await
}

/// Apply a title/markdown update inside the BEGIN IMMEDIATE revision-snapshot
/// transaction. The single source for the owner PATCH, share-token edits and
/// MCP `update_deck` — so the snapshot policy can never be bypassed.
pub(crate) async fn update_deck_data(
    state: &AppState,
    id: &str,
    owner_id: &str,
    title: Option<String>,
    markdown: Option<String>,
    base_updated_at: Option<&str>,
) -> Result<DeckRow, DeckError> {
    if title.is_none() && markdown.is_none() {
        return Err(DeckError::NothingToUpdate);
    }
    let title = check_title(title)?;
    if markdown_too_large(&markdown) {
        return Err(DeckError::MarkdownTooLarge);
    }

    let now = now_rfc3339();
    let mut tx = state
        .db
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|_| DeckError::Db)?;
    let outcome = update_in_tx(
        &mut tx,
        state,
        id,
        owner_id,
        &title,
        &markdown,
        base_updated_at,
        &now,
    )
    .await;
    match outcome {
        Ok(Some(row)) => {
            tx.commit().await.map_err(|_| DeckError::Db)?;
            Ok(row)
        }
        Ok(None) => {
            let _ = tx.rollback().await;
            Err(DeckError::NotFound)
        }
        Err(_) => {
            let _ = tx.rollback().await;
            Err(DeckError::Db)
        }
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
    match create_deck_data(&state, &user_id, body.title, body.markdown).await {
        Ok(row) => (StatusCode::CREATED, Json(DeckFull::from(row))).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn get_one(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    get_deck_core(&state, &id, &user_id).await
}

/// Read one deck as `owner_id`. Shared by the owner route and the share-token
/// route (which passes the deck's real owner after the token authorized it).
pub(crate) async fn get_deck_core(state: &AppState, id: &str, owner_id: &str) -> Response {
    match fetch_deck_data(state, id, owner_id).await {
        Ok(row) => Json(DeckFull::from(row)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// The owner of a live (non-deleted) deck, or None if it is gone. Used by the
/// share-token handlers to resolve the owner the reusable cores scope by.
pub(crate) async fn live_deck_owner(
    state: &AppState,
    deck_id: &str,
) -> Result<Option<String>, Response> {
    sqlx::query_scalar::<_, String>(
        "SELECT owner_id FROM decks WHERE id = ?1 AND deleted_at IS NULL",
    )
    .bind(deck_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
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
    update_deck_core(&state, &id, &user_id, body).await
}

/// Apply a deck update as `owner_id` (title/markdown + the revision-snapshot
/// transaction). Shared by the owner PATCH and the share-token edit route.
pub(crate) async fn update_deck_core(
    state: &AppState,
    id: &str,
    owner_id: &str,
    body: UpdateDeck,
) -> Response {
    match update_deck_data(
        state,
        id,
        owner_id,
        body.title,
        body.markdown,
        body.base_updated_at.as_deref(),
    )
    .await
    {
        Ok(row) => Json(DeckFull::from(row)).into_response(),
        Err(e) => e.into_response(),
    }
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

    // Keep the theme column in sync whenever the markdown changes.
    let theme = markdown.as_ref().map(|m| theme_from_markdown(m));
    let done = sqlx::query(
        "UPDATE decks SET title = COALESCE(?1, title), markdown = COALESCE(?2, markdown), \
         theme = COALESCE(?3, theme), updated_at = ?4 \
         WHERE id = ?5 AND owner_id = ?6 AND deleted_at IS NULL",
    )
    .bind(title)
    .bind(markdown)
    .bind(theme)
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
    revisions_list_core(&state, &id, &user_id).await
}

pub(crate) async fn revisions_list_core(state: &AppState, id: &str, owner_id: &str) -> Response {
    match fetch_deck(state, id, owner_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found(),
        Err(response) => return response,
    }
    match sqlx::query_as::<_, RevisionMetaRow>(
        "SELECT id, created_at, LENGTH(CAST(markdown AS BLOB)) AS size_bytes \
         FROM revisions WHERE deck_id = ?1 ORDER BY created_at DESC, rowid DESC",
    )
    .bind(id)
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
    revision_get_core(&state, &id, &user_id, &rev_id).await
}

pub(crate) async fn revision_get_core(
    state: &AppState,
    id: &str,
    owner_id: &str,
    rev_id: &str,
) -> Response {
    match fetch_deck(state, id, owner_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found(),
        Err(response) => return response,
    }
    match fetch_revision(&state.db, id, rev_id).await {
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
    revision_restore_core(&state, &id, &user_id, &rev_id).await
}

pub(crate) async fn revision_restore_core(
    state: &AppState,
    id: &str,
    owner_id: &str,
    rev_id: &str,
) -> Response {
    let now = now_rfc3339();
    let mut tx = match state.db.begin_with("BEGIN IMMEDIATE").await {
        Ok(tx) => tx,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    let outcome = restore_in_tx(&mut tx, id, owner_id, rev_id, &now).await;
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
    md_export_core(&state, &id, &user_id).await
}

/// Markdown download for one deck as `owner_id`. Shared by the owner route and
/// the share-token `.md` export.
pub(crate) async fn md_export_core(state: &AppState, id: &str, owner_id: &str) -> Response {
    let deck = match fetch_deck(state, id, owner_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };

    let disposition = crate::content_disposition(&deck.title, "md");
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

#[cfg(test)]
mod tests {
    use super::{theme_from_markdown, DEFAULT_THEME};

    #[test]
    fn reads_the_active_theme_directive() {
        let md = "---\nmarp: true\ntheme: deckoala-dark\npaginate: true\n---\n\n# Hi\n";
        assert_eq!(theme_from_markdown(md), "deckoala-dark");
    }

    #[test]
    fn falls_back_without_frontmatter_or_directive() {
        assert_eq!(theme_from_markdown("# just a body\n"), DEFAULT_THEME);
        assert_eq!(
            theme_from_markdown("---\nmarp: true\n---\n\n# Hi\n"),
            DEFAULT_THEME
        );
    }

    #[test]
    fn ignores_commented_or_indented_theme() {
        let commented = "---\nmarp: true\n# theme: hidden\n---\n\n# Hi\n";
        assert_eq!(theme_from_markdown(commented), DEFAULT_THEME);
        // An indented `theme:` inside a style block is not the directive.
        let in_block = "---\nmarp: true\nstyle: |\n  theme: not-a-directive\n---\n\n# Hi\n";
        assert_eq!(theme_from_markdown(in_block), DEFAULT_THEME);
    }

    #[test]
    fn strips_quotes_and_trailing_comment() {
        let md = "---\ntheme: \"deckoala-bold\" # nice\n---\n\nx\n";
        assert_eq!(theme_from_markdown(md), "deckoala-bold");
    }
}
