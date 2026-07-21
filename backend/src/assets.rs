//! Image asset upload + owner-scoped serving (BRIEF-0004).

use std::path::PathBuf;

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::SESSION_USER_KEY;
use crate::decks::AuthUser;
use crate::export::print_cookie_authorizes;
use crate::{json_error, now_rfc3339, AppState};

const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// The deck must belong to the caller and not be soft-deleted (404 otherwise),
/// matching the decks/revisions owner-scoping invariant.
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

/// A live (not soft-deleted) deck exists with this id — used for the print
/// path where the token, not ownership, is the authorization.
async fn deck_is_live(state: &AppState, deck_id: &str) -> Result<bool, Response> {
    sqlx::query_scalar::<_, i64>("SELECT 1 FROM decks WHERE id = ?1 AND deleted_at IS NULL")
        .bind(deck_id)
        .fetch_optional(&state.db)
        .await
        .map(|row| row.is_some())
        .map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
}

/// Serve authorization: the session owner, OR a valid print cookie for this
/// deck (so Chromium can load images during PDF export).
async fn authorize_serve(
    state: &AppState,
    session: &Session,
    headers: &HeaderMap,
    deck_id: &str,
) -> Result<bool, Response> {
    let user_id: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
    if let Some(user_id) = user_id {
        if owner_owns_deck(state, deck_id, &user_id).await? {
            return Ok(true);
        }
    }
    if print_cookie_authorizes(&state.print_secret, headers, deck_id) {
        return deck_is_live(state, deck_id).await;
    }
    Ok(false)
}

/// A single path segment is safe: non-empty, only `[A-Za-z0-9._-]`, and not a
/// traversal token. The real traversal defense is the DB row lookup; this is
/// defense-in-depth so a crafted name never reaches the filesystem.
pub(crate) fn safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains("..")
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Sniff the real image type from magic bytes (the declared Content-Type is
/// untrusted). Returns (mime, extension) or None.
fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        // Require both the RIFF prefix AND the WEBP fourcc — RIFF alone also
        // covers WAV/AVI and would wave a polyglot through.
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

/// Sanitize the client's filename for display / alt text only (never used to
/// build a path). Strips separators + control chars, caps length.
fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\'))
        .collect();
    let cleaned = cleaned.trim();
    let capped: String = cleaned.chars().take(255).collect();
    if capped.is_empty() {
        "image".to_owned()
    } else {
        capped
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetDto {
    id: String,
    url: String,
    original_name: String,
    mime: String,
    size_bytes: i64,
}

pub async fn upload(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(deck_id): Path<String>,
    mut multipart: Multipart,
) -> Response {
    // Validate before the path is ever built — even though deck ids are
    // server-minted uuids today, the filesystem path must never be
    // constructed from an unvalidated segment (defense-in-depth, mirrors
    // serve()).
    if !safe_segment(&deck_id) {
        return not_found();
    }
    match owner_owns_deck(&state, &deck_id, &user_id).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(response) => return response,
    }

    // Pull the `file` field.
    let field = loop {
        match multipart.next_field().await {
            Ok(Some(field)) if field.name() == Some("file") => break field,
            Ok(Some(_)) => continue,
            Ok(None) => return json_error(StatusCode::UNPROCESSABLE_ENTITY, "no file field"),
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "malformed upload"),
        }
    };
    let original_name = sanitize_name(field.file_name().unwrap_or("image"));
    let bytes = match field.bytes().await {
        Ok(bytes) => bytes,
        // axum returns this when the body exceeds the route's DefaultBodyLimit.
        Err(_) => return json_error(StatusCode::PAYLOAD_TOO_LARGE, "upload too large"),
    };
    if bytes.len() > MAX_IMAGE_BYTES {
        return json_error(StatusCode::PAYLOAD_TOO_LARGE, "image exceeds 5 MB");
    }
    let Some((mime, ext)) = sniff_image(&bytes) else {
        return json_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "only PNG, JPEG, GIF or WebP images are allowed",
        );
    };

    let id = Uuid::new_v4().to_string();
    let filename = format!("{id}.{ext}");
    let dir: PathBuf = state.data_dir.join("assets").join(&deck_id);
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error");
    }
    if tokio::fs::write(dir.join(&filename), &bytes).await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error");
    }

    let size_bytes = bytes.len() as i64;
    let now = now_rfc3339();
    let inserted = sqlx::query(
        "INSERT INTO assets (id, deck_id, filename, original_name, mime, size_bytes, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(&deck_id)
    .bind(&filename)
    .bind(&original_name)
    .bind(mime)
    .bind(size_bytes)
    .bind(&now)
    .execute(&state.db)
    .await;
    if inserted.is_err() {
        let _ = tokio::fs::remove_file(dir.join(&filename)).await;
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    (
        StatusCode::CREATED,
        Json(AssetDto {
            url: format!("/assets/{deck_id}/{filename}"),
            id,
            original_name,
            mime: mime.to_owned(),
            size_bytes,
        }),
    )
        .into_response()
}

pub async fn serve(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Path((deck_id, filename)): Path<(String, String)>,
) -> Response {
    if !safe_segment(&deck_id) || !safe_segment(&filename) {
        return not_found();
    }
    // Authorized by the session owner OR a valid print cookie for this deck
    // (the latter lets in-container Chromium load images for PDF export).
    match authorize_serve(&state, &session, &headers, &deck_id).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(response) => return response,
    }

    let mime: Option<String> =
        match sqlx::query_scalar("SELECT mime FROM assets WHERE deck_id = ?1 AND filename = ?2")
            .bind(&deck_id)
            .bind(&filename)
            .fetch_optional(&state.db)
            .await
        {
            Ok(mime) => mime,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
        };
    let Some(mime) = mime else {
        return not_found();
    };

    let path = state.data_dir.join("assets").join(&deck_id).join(&filename);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(_) => return not_found(),
    };

    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&mime) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    // Never let a browser content-sniff a polyglot into executable HTML.
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    response
}

fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not found")
}

#[cfg(test)]
mod tests {
    use super::{safe_segment, sanitize_name, sniff_image};

    #[test]
    fn sniffs_real_image_signatures() {
        assert_eq!(
            sniff_image(&[0x89, 0x50, 0x4E, 0x47, 0, 0]).unwrap().0,
            "image/png"
        );
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0]).unwrap().0, "image/jpeg");
        assert_eq!(sniff_image(b"GIF89a...").unwrap().0, "image/gif");
        let mut webp = b"RIFF\0\0\0\0WEBPxxxx".to_vec();
        webp.truncate(16);
        assert_eq!(sniff_image(&webp).unwrap().0, "image/webp");
    }

    #[test]
    fn rejects_non_images_and_riff_without_webp() {
        assert!(sniff_image(b"<html>not a png</html>").is_none());
        assert!(sniff_image(b"RIFF\0\0\0\0WAVExxxx").is_none()); // RIFF but not WEBP
        assert!(sniff_image(b"").is_none());
    }

    #[test]
    fn safe_segment_blocks_traversal() {
        assert!(safe_segment("abc123.png"));
        assert!(!safe_segment(".."));
        assert!(!safe_segment("."));
        assert!(!safe_segment("a/b"));
        assert!(!safe_segment("a..b"));
        assert!(!safe_segment(""));
        assert!(!safe_segment("space name"));
    }

    #[test]
    fn sanitize_name_strips_paths_and_controls() {
        assert_eq!(sanitize_name("../../etc/passwd"), "....etcpasswd");
        assert_eq!(sanitize_name(""), "image");
        assert_eq!(sanitize_name("photo\n.png"), "photo.png");
    }
}
