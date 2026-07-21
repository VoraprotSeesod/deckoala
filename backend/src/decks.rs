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
pub struct UpdateDeck {
    title: Option<String>,
    markdown: Option<String>,
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
    let updated = sqlx::query(
        "UPDATE decks SET title = COALESCE(?1, title), markdown = COALESCE(?2, markdown), \
         updated_at = ?3 WHERE id = ?4 AND owner_id = ?5 AND deleted_at IS NULL",
    )
    .bind(&title)
    .bind(&body.markdown)
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.db)
    .await;
    match updated {
        Ok(done) if done.rows_affected() == 0 => not_found(),
        Ok(_) => match fetch_deck(&state, &id, &user_id).await {
            Ok(Some(row)) => Json(DeckFull::from(row)).into_response(),
            _ => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
        },
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
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
