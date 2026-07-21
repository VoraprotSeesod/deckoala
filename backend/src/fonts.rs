//! Font manager: upload + Google Fonts (served locally), fonts.css (BRIEF-0007).
//!
//! The ONLY external network call in the whole app is the server-side Google
//! Fonts fetch in `google()`. Everything served to viewers is same-origin
//! `/fonts/...` (the no-external-CDN invariant, CLAUDE.md §2).

use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assets::safe_segment;
use crate::decks::{AdminUser, AuthUser};
use crate::{json_error, now_rfc3339, AppState};

const MAX_FONT_BYTES: usize = 5 * 1024 * 1024;
const MAX_CSS_BYTES: usize = 512 * 1024;
const MAX_WEIGHTS: usize = 18;
const MAX_GOOGLE_FILES: usize = 60;
const GSTATIC_HOST: &str = "fonts.gstatic.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
struct FontRow {
    id: String,
    family: String,
    weight: String,
    style: String,
    #[serde(skip)]
    unicode_range: String,
    #[serde(skip)]
    filename: String,
    format: String,
    source: String,
    created_at: String,
}

/// CSS-identifier-safe family (letters, digits, spaces, hyphens) — because it
/// is emitted verbatim into the public fonts.css; rejects quotes/braces/etc.
fn valid_family(family: &str) -> bool {
    let trimmed = family.trim();
    (1..=100).contains(&trimmed.chars().count())
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-'))
}

fn valid_weight(weight: &str) -> bool {
    weight.len() == 3
        && weight.starts_with(['1', '2', '3', '4', '5', '6', '7', '8', '9'])
        && weight.ends_with("00")
}

fn valid_style(style: &str) -> bool {
    matches!(style, "normal" | "italic")
}

/// Keep only the characters a CSS `unicode-range` value legitimately uses (hex
/// digits, `U`, `+`, `-`, `,`, whitespace). This value is parsed verbatim from
/// the Google css2 response and later emitted into the public fonts.css, so it
/// is stripped to a safe charset: even if that one trusted response were ever
/// tampered with, no stray `/*`/`}`/`;` can survive to inject CSS.
fn sanitize_unicode_range(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || matches!(c, 'U' | 'u' | '+' | '-' | ',' | ' '))
        .collect()
}

fn sniff_font(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    // (format, ext)
    if bytes.starts_with(b"wOF2") {
        Some(("woff2", "woff2"))
    } else if bytes.starts_with(b"wOFF") {
        Some(("woff", "woff"))
    } else if bytes.starts_with(b"OTTO") {
        Some(("opentype", "otf"))
    } else if bytes.starts_with(&[0x00, 0x01, 0x00, 0x00])
        || bytes.starts_with(b"true")
        || bytes.starts_with(b"ttcf")
    {
        Some(("truetype", "ttf"))
    } else {
        None
    }
}

fn content_type_for(format: &str) -> &'static str {
    match format {
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "opentype" => "font/otf",
        _ => "font/ttf",
    }
}

// --- list ---

pub async fn list(State(state): State<AppState>, _user: AuthUser) -> Response {
    match sqlx::query_as::<_, FontRow>(
        "SELECT id, family, weight, style, unicode_range, filename, format, source, created_at \
         FROM fonts ORDER BY family, weight, style",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => Json(rows).into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

// --- upload ---

pub async fn upload(
    State(state): State<AppState>,
    _admin: AdminUser,
    mut multipart: Multipart,
) -> Response {
    let mut family = String::new();
    let mut weight = "400".to_owned();
    let mut style = "normal".to_owned();
    let mut file_bytes: Option<Vec<u8>> = None;

    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => match field.name() {
                Some("family") => family = field.text().await.unwrap_or_default(),
                Some("weight") => weight = field.text().await.unwrap_or_default(),
                Some("style") => style = field.text().await.unwrap_or_default(),
                Some("file") => {
                    file_bytes = match field.bytes().await {
                        Ok(b) => Some(b.to_vec()),
                        Err(_) => {
                            return json_error(StatusCode::PAYLOAD_TOO_LARGE, "font too large")
                        }
                    };
                }
                _ => {}
            },
            Ok(None) => break,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "malformed upload"),
        }
    }

    let family = family.trim().to_owned();
    if !valid_family(&family) {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "family must be 1-100 letters, digits, spaces or hyphens",
        );
    }
    if !valid_weight(&weight) || !valid_style(&style) {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid weight or style");
    }
    let Some(bytes) = file_bytes else {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "no font file");
    };
    if bytes.len() > MAX_FONT_BYTES {
        return json_error(StatusCode::PAYLOAD_TOO_LARGE, "font exceeds 5 MB");
    }
    let Some((format, ext)) = sniff_font(&bytes) else {
        return json_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "only WOFF2, WOFF, TTF or OTF fonts are allowed",
        );
    };

    let filename = format!("{}.{ext}", Uuid::new_v4());
    let dir = state.data_dir.join("fonts");
    if tokio::fs::create_dir_all(&dir).await.is_err()
        || tokio::fs::write(dir.join(&filename), &bytes).await.is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error");
    }

    let inserted = sqlx::query(
        "INSERT INTO fonts \
         (id, family, weight, style, unicode_range, filename, format, source, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&family)
    .bind(&weight)
    .bind(&style)
    .bind("")
    .bind(&filename)
    .bind(format)
    .bind("upload")
    .bind(now_rfc3339())
    .execute(&state.db)
    .await;
    match inserted {
        Ok(_) => (
            StatusCode::CREATED,
            list(State(state), AuthUser(String::new())).await,
        )
            .into_response(),
        Err(err)
            if err
                .as_database_error()
                .is_some_and(|e| e.is_unique_violation()) =>
        {
            let _ = tokio::fs::remove_file(dir.join(&filename)).await;
            json_error(
                StatusCode::CONFLICT,
                "that font variant is already installed",
            )
        }
        Err(_) => {
            let _ = tokio::fs::remove_file(dir.join(&filename)).await;
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

// --- Google Fonts install ---

#[derive(Deserialize)]
pub struct GoogleRequest {
    family: String,
    #[serde(default)]
    weights: Vec<String>,
}

/// One parsed `@font-face` from the css2 response.
struct FontFace {
    weight: String,
    style: String,
    unicode_range: String,
    url: String,
    format: String,
}

fn css_value(block: &str, key: &str) -> Option<String> {
    let start = block.find(key)? + key.len();
    let rest = &block[start..];
    let colon = rest.find(':')?;
    let end = rest.find(';').unwrap_or(rest.len());
    if colon >= end {
        return None;
    }
    Some(rest[colon + 1..end].trim().to_owned())
}

fn parse_font_faces(css: &str) -> Vec<FontFace> {
    let mut faces = Vec::new();
    for chunk in css.split("@font-face").skip(1) {
        let Some(open) = chunk.find('{') else {
            continue;
        };
        let Some(close) = chunk[open..].find('}') else {
            continue;
        };
        let block = &chunk[open + 1..open + close];
        let weight = css_value(block, "font-weight").unwrap_or_else(|| "400".into());
        let style = css_value(block, "font-style").unwrap_or_else(|| "normal".into());
        let unicode_range = css_value(block, "unicode-range").unwrap_or_default();
        // src: url(...) format('woff2')
        let Some(src) = css_value(block, "src") else {
            continue;
        };
        let Some(url_start) = src.find("url(") else {
            continue;
        };
        let after = &src[url_start + 4..];
        let Some(url_end) = after.find(')') else {
            continue;
        };
        let url = after[..url_end].trim().trim_matches(['"', '\'']).to_owned();
        let format = if src.contains("woff2") {
            "woff2"
        } else {
            "woff"
        }
        .to_owned();
        faces.push(FontFace {
            weight: weight
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>(),
            style,
            unicode_range,
            url,
            format,
        });
    }
    faces
}

/// Exactly `https://fonts.gstatic.com/...` (parsed host equality, not contains).
fn is_gstatic(url: &str) -> bool {
    match reqwest::Url::parse(url) {
        Ok(parsed) => parsed.scheme() == "https" && parsed.host_str() == Some(GSTATIC_HOST),
        Err(_) => false,
    }
}

fn no_redirect_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
}

/// Read a response body fully, returning `None` past `cap` bytes OR if the
/// stream errors mid-body. A mid-stream error (connection reset, timeout) must
/// NOT be mistaken for a clean end-of-body: accepting the partial bytes would
/// store a truncated, corrupt font that browsers and the PDF renderer fail to
/// decode while the DB claims it is installed.
async fn read_capped(mut resp: reqwest::Response, cap: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if buf.len() + chunk.len() > cap {
                    return None;
                }
                buf.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(buf),
            Err(_) => return None,
        }
    }
}

pub async fn google(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<GoogleRequest>,
) -> Response {
    let family = body.family.trim().to_owned();
    if !valid_family(&family) {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid family");
    }
    let mut weights = body.weights;
    if weights.is_empty() {
        weights = vec!["400".into(), "700".into()];
    }
    if weights.len() > MAX_WEIGHTS || !weights.iter().all(|w| valid_weight(w)) {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid weights");
    }

    // Build the URL ourselves; only validated values are interpolated.
    let family_q = family.replace(' ', "+");
    let url = format!(
        "https://fonts.googleapis.com/css2?family={family_q}:wght@{}",
        weights.join(";")
    );

    let Ok(client) = no_redirect_client(Duration::from_secs(10)) else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "client error");
    };
    let resp = match client.get(&url).header(header::USER_AGENT, UA).send().await {
        Ok(resp) => resp,
        Err(_) => return json_error(StatusCode::BAD_GATEWAY, "could not reach Google Fonts"),
    };
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return json_error(StatusCode::NOT_FOUND, "font family not found");
    }
    if !resp.status().is_success() {
        return json_error(StatusCode::BAD_GATEWAY, "Google Fonts error");
    }
    let Some(css_bytes) = read_capped(resp, MAX_CSS_BYTES).await else {
        return json_error(
            StatusCode::BAD_GATEWAY,
            "Google Fonts response was incomplete or too large",
        );
    };
    let css = String::from_utf8_lossy(&css_bytes);
    let faces = parse_font_faces(&css);
    if faces.is_empty() {
        return json_error(StatusCode::NOT_FOUND, "no fonts found for that family");
    }
    if faces.len() > MAX_GOOGLE_FILES {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "too many font files");
    }

    let dir = state.data_dir.join("fonts");
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error");
    }

    // One transaction: any failure rolls the rows back; temp files are cleaned.
    let mut written: Vec<PathBuf> = Vec::new();
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    for face in &faces {
        if !is_gstatic(&face.url) {
            continue; // never download a non-gstatic URL
        }
        let bytes = match client
            .get(&face.url)
            .header(header::USER_AGENT, UA)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => match read_capped(r, MAX_FONT_BYTES).await {
                Some(b) if !b.is_empty() => b,
                _ => continue,
            },
            _ => continue,
        };
        let filename = format!("{}.{}", Uuid::new_v4(), face.format);
        let path = dir.join(&filename);
        if tokio::fs::write(&path, &bytes).await.is_err() {
            break;
        }
        let weight = if face.weight.is_empty() {
            "400".to_owned()
        } else {
            face.weight.clone()
        };
        // Both style and unicode_range come verbatim from the Google response;
        // clamp/strip them before they can reach the public fonts.css.
        let style = if valid_style(&face.style) {
            face.style.clone()
        } else {
            "normal".to_owned()
        };
        let unicode_range = sanitize_unicode_range(&face.unicode_range);
        let inserted = sqlx::query(
            "INSERT INTO fonts \
             (id, family, weight, style, unicode_range, filename, format, source, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT (family, weight, style, unicode_range) DO NOTHING",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&family)
        .bind(&weight)
        .bind(&style)
        .bind(&unicode_range)
        .bind(&filename)
        .bind(&face.format)
        .bind("google")
        .bind(now_rfc3339())
        .execute(&mut *tx)
        .await;
        match inserted {
            // New row: track the file so a later failure can roll it back.
            Ok(result) if result.rows_affected() > 0 => written.push(path),
            // ON CONFLICT DO NOTHING: this variant already exists, so the file
            // just downloaded is a duplicate — delete it now instead of leaving
            // an orphan under /data/fonts that no row references.
            Ok(_) => {
                let _ = tokio::fs::remove_file(&path).await;
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&path).await;
                let _ = tx.rollback().await;
                for p in &written {
                    let _ = tokio::fs::remove_file(p).await;
                }
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
            }
        }
    }
    if tx.commit().await.is_err() {
        for p in &written {
            let _ = tokio::fs::remove_file(p).await;
        }
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    (
        StatusCode::CREATED,
        list(State(state), AuthUser(String::new())).await,
    )
        .into_response()
}

// --- delete ---

pub async fn delete(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Response {
    let filename: Option<String> =
        match sqlx::query_scalar("SELECT filename FROM fonts WHERE id = ?1")
            .bind(&id)
            .fetch_optional(&state.db)
            .await
        {
            Ok(f) => f,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
        };
    let Some(filename) = filename else {
        return json_error(StatusCode::NOT_FOUND, "not found");
    };
    if sqlx::query("DELETE FROM fonts WHERE id = ?1")
        .bind(&id)
        .execute(&state.db)
        .await
        .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    // Only remove the file if no other row still references it.
    let still_used: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fonts WHERE filename = ?1")
        .bind(&filename)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    if still_used == 0 {
        let _ = tokio::fs::remove_file(state.data_dir.join("fonts").join(&filename)).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

// --- fonts.css ---

/// CSS-string-escape a value going inside single quotes.
fn css_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

pub async fn fonts_css(State(state): State<AppState>) -> Response {
    let rows: Vec<FontRow> = sqlx::query_as(
        "SELECT id, family, weight, style, unicode_range, filename, format, source, created_at \
         FROM fonts ORDER BY family, weight, style",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut css = String::from("/* Deckoala installed fonts */\n");
    for row in &rows {
        css.push_str("@font-face {\n");
        css.push_str(&format!("  font-family: '{}';\n", css_escape(&row.family)));
        css.push_str(&format!("  font-weight: {};\n", row.weight));
        css.push_str(&format!("  font-style: {};\n", row.style));
        css.push_str("  font-display: swap;\n");
        if !row.unicode_range.is_empty() {
            css.push_str(&format!("  unicode-range: {};\n", row.unicode_range));
        }
        css.push_str(&format!(
            "  src: url('/fonts/{}') format('{}');\n",
            css_escape(&row.filename),
            css_escape(&row.format)
        ));
        css.push_str("}\n");
    }

    let mut response = (StatusCode::OK, css).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    // Must revalidate so a freshly installed font shows on the next load
    // (distinct from the immutable cache on the uuid-named /fonts/ files).
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

// --- serve /fonts/{filename} (public, session-less for Chromium) ---

pub async fn serve(State(state): State<AppState>, Path(filename): Path<String>) -> Response {
    if !safe_segment(&filename) {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    let format: Option<String> =
        match sqlx::query_scalar("SELECT format FROM fonts WHERE filename = ?1")
            .bind(&filename)
            .fetch_optional(&state.db)
            .await
        {
            Ok(f) => f,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
        };
    let Some(format) = format else {
        return json_error(StatusCode::NOT_FOUND, "not found");
    };
    let bytes = match tokio::fs::read(state.data_dir.join("fonts").join(&filename)).await {
        Ok(b) => b,
        Err(_) => return json_error(StatusCode::NOT_FOUND, "not found"),
    };
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type_for(&format)),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::{
        is_gstatic, parse_font_faces, sanitize_unicode_range, sniff_font, valid_family,
        valid_weight,
    };

    const SAMPLE_CSS: &str = "/* thai */\n@font-face {\n  font-family: 'Sarabun';\n  font-style: normal;\n  font-weight: 400;\n  font-display: swap;\n  src: url(https://fonts.gstatic.com/s/sarabun/thai.woff2) format('woff2');\n  unicode-range: U+0E01-0E5B, U+200C-200D;\n}\n/* latin */\n@font-face {\n  font-family: 'Sarabun';\n  font-style: normal;\n  font-weight: 400;\n  font-display: swap;\n  src: url(https://fonts.gstatic.com/s/sarabun/latin.woff2) format('woff2');\n  unicode-range: U+0000-00FF;\n}\n";

    #[test]
    fn parses_every_subset_block() {
        let faces = parse_font_faces(SAMPLE_CSS);
        assert_eq!(faces.len(), 2, "both thai + latin subsets parse");
        assert_eq!(faces[0].weight, "400");
        assert_eq!(faces[0].style, "normal");
        assert!(faces[0].unicode_range.contains("U+0E01"));
        assert!(faces[0].url.contains("thai.woff2"));
        assert!(faces[1].url.contains("latin.woff2"));
    }

    #[test]
    fn gstatic_host_check_is_exact() {
        assert!(is_gstatic("https://fonts.gstatic.com/s/x.woff2"));
        assert!(!is_gstatic("http://fonts.gstatic.com/s/x.woff2")); // not https
        assert!(!is_gstatic("https://fonts.gstatic.com.evil.com/x")); // lookalike
        assert!(!is_gstatic("https://evil.com/fonts.gstatic.com")); // substring
        assert!(!is_gstatic("not a url"));
    }

    #[test]
    fn family_and_weight_validation() {
        assert!(valid_family("Noto Sans Thai"));
        assert!(!valid_family("evil'; }")); // quotes/braces rejected
        assert!(!valid_family(""));
        assert!(valid_weight("400"));
        assert!(valid_weight("700"));
        assert!(!valid_weight("401"));
        assert!(!valid_weight("40"));
        assert!(!valid_weight("abc"));
        assert!(!valid_weight("400;family=Evil"));
    }

    #[test]
    fn unicode_range_sanitizer_strips_css_injection() {
        // Legit Google value survives byte-for-byte.
        let ok = "U+0E01-0E5B, U+200C-200D, U+25CC";
        assert_eq!(sanitize_unicode_range(ok), ok);
        // A tampered value cannot carry a comment/brace/semicolon/colon through.
        let cleaned = sanitize_unicode_range("U+0-FF;} body{display:none} /* ");
        assert!(!cleaned.contains(['/', '*', '}', '{', ';', ':']));
        assert!(cleaned.starts_with("U+0-FF"));
        assert!(!cleaned.contains("display"));
        assert!(!cleaned.contains("none"));
    }

    #[test]
    fn sniffs_font_magic_bytes() {
        assert_eq!(sniff_font(b"wOF2....").unwrap().0, "woff2");
        assert_eq!(sniff_font(b"wOFF....").unwrap().0, "woff");
        assert_eq!(sniff_font(b"OTTO....").unwrap().0, "opentype");
        assert_eq!(
            sniff_font(&[0x00, 0x01, 0x00, 0x00, 0, 0]).unwrap().0,
            "truetype"
        );
        assert!(sniff_font(b"<html>").is_none());
    }
}
