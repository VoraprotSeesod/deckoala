//! Research library (BRIEF-0014): per-user source documents whose EXTRACTED
//! TEXT feeds AI slide generation.
//!
//! Extraction is entirely local — PDF text via `pdf-extract`, plain text as-is
//! — so this adds NO outbound call (CLAUDE.md §2 unchanged). Only the extracted
//! text is stored (`<data>/research/<user_id>/<id>.txt`); the original binary is
//! not kept or re-served. Everything is owner-scoped by `user_id`: a document
//! belonging to another user is indistinguishable from a missing one.

use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use std::path::PathBuf;
use uuid::Uuid;

use crate::assets::safe_segment;
use crate::decks::AuthUser;
use crate::{json_error, now_rfc3339, AppState};

/// Upload cap for the raw file (a research PDF is usually a few MB).
const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
/// Stored extracted text cap, so one pathological document can't fill the disk
/// or dominate every future prompt.
const MAX_TEXT_CHARS: usize = 500_000;
/// Bytes of text returned by the preview endpoint.
const PREVIEW_BYTES: usize = 2 * 1024;
/// Library cap per user.
const MAX_DOCS_PER_USER: i64 = 50;
/// Figures kept per document — a PDF is untrusted input, so the count, the
/// per-image size and a minimum size are all bounded (BRIEF-0014 §D).
const MAX_FIGURES_PER_DOC: usize = 40;
const MAX_FIGURE_BYTES: usize = 5 * 1024 * 1024;
/// Below this, an image is almost always an icon/rule rather than a figure.
const MIN_FIGURE_PX: u32 = 64;

#[derive(sqlx::FromRow)]
struct DocRow {
    id: String,
    original_name: String,
    mime: String,
    char_count: i64,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocView {
    id: String,
    original_name: String,
    mime: String,
    char_count: i64,
    created_at: String,
}

impl From<DocRow> for DocView {
    fn from(row: DocRow) -> Self {
        Self {
            id: row.id,
            original_name: row.original_name,
            mime: row.mime,
            char_count: row.char_count,
            created_at: row.created_at,
        }
    }
}

fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not found")
}

/// Filename for display: strip control/path characters, cap the length.
fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\'))
        .collect();
    let capped: String = cleaned.trim().chars().take(255).collect();
    if capped.is_empty() {
        "document".to_owned()
    } else {
        capped
    }
}

/// Truncate to a char boundary so the stored text is always valid UTF-8.
fn cap_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max).collect()
}

/// Truncate to a BYTE budget on a char boundary. Prompt budgets are byte-sized
/// (a model's limit is not per-character), and Thai/CJK cost ~3 bytes per char
/// — capping by chars would silently send ~3x the intended payload.
fn cap_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Collapse the runs of whitespace PDF extraction leaves behind, so the model
/// budget is spent on words rather than layout artifacts.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
            out.push('\n');
        } else {
            blank_run = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    out.trim().to_owned()
}

/// Why an upload could not become source text. Kept small (a `Response` here
/// would trip clippy's `result_large_err`); the handler maps it to a status.
#[derive(Debug, PartialEq, Eq)]
enum ExtractError {
    /// The PDF could not be parsed at all.
    Unreadable,
    /// Parsed, but produced no text — almost always a scanned/image-only PDF.
    NoText,
    /// Neither a PDF nor UTF-8 text.
    Unsupported,
}

impl ExtractError {
    fn into_response(self) -> Response {
        match self {
            Self::Unreadable => {
                json_error(StatusCode::UNPROCESSABLE_ENTITY, "could not read this PDF")
            }
            Self::NoText => json_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "no text found in this PDF — is it a scan? Upload a text PDF or paste the text as .txt",
            ),
            Self::Unsupported => json_error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "only PDF or text (.txt/.md) files are supported",
            ),
        }
    }
}

/// Extract text from an uploaded document. PDF is parsed locally; UTF-8 text is
/// taken as-is. Anything else is rejected — the declared Content-Type is
/// untrusted, so this sniffs the content itself.
fn extract(bytes: &[u8]) -> Result<(&'static str, String), ExtractError> {
    if bytes.starts_with(b"%PDF-") {
        // pdf-extract can panic on malformed input; contain it so a bad upload
        // returns 422 instead of taking down the worker.
        let parsed = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes));
        let text = match parsed {
            Ok(Ok(text)) => tidy(&text),
            _ => return Err(ExtractError::Unreadable),
        };
        if text.trim().is_empty() {
            return Err(ExtractError::NoText);
        }
        return Ok(("application/pdf", text));
    }

    match std::str::from_utf8(bytes) {
        Ok(text) if !text.trim().is_empty() && !text.contains('\0') => {
            Ok(("text/plain", tidy(text)))
        }
        _ => Err(ExtractError::Unsupported),
    }
}

/// Decode ASCII85 (`/ASCII85Decode`). PDF streams may end with `~>`.
fn ascii85_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len());
    let mut tuple = [0u8; 5];
    let mut count = 0;
    for byte in input.iter().copied() {
        match byte {
            b'~' => break, // end marker
            b if b.is_ascii_whitespace() => continue,
            b'z' if count == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                tuple[count] = byte - b'!';
                count += 1;
                if count == 5 {
                    let mut value: u32 = 0;
                    for digit in tuple {
                        value = value.checked_mul(85)?.checked_add(u32::from(digit))?;
                    }
                    out.extend_from_slice(&value.to_be_bytes());
                    count = 0;
                }
            }
            _ => return None, // not ASCII85
        }
    }
    if count > 0 {
        // Partial group: pad with 'u' (84) and keep count-1 bytes.
        let mut value: u32 = 0;
        for (i, digit) in tuple.iter().enumerate() {
            let digit = if i < count { *digit } else { 84 };
            value = value.checked_mul(85)?.checked_add(u32::from(digit))?;
        }
        out.extend_from_slice(&value.to_be_bytes()[..count - 1]);
    }
    Some(out)
}

fn flate_decode(input: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(input)
        .take(MAX_FIGURE_BYTES as u64)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

/// Undo the transport filters that PRECEDE an image filter in a PDF filter
/// chain, e.g. `[/ASCII85Decode /DCTDecode]` — lopdf refuses to touch DCTDecode
/// (it is an image codec, not compression), so the raw stream bytes are still
/// ASCII85 text and would be an invalid JPEG if written straight to disk.
/// Returns the JPEG payload, or None when the chain isn't one we can unwrap.
fn jpeg_payload(filters: &[Vec<u8>], raw: &[u8]) -> Option<Vec<u8>> {
    let dct_at = filters.iter().position(|f| f == b"DCTDecode")?;
    let mut data = raw.to_vec();
    for filter in &filters[..dct_at] {
        data = match filter.as_slice() {
            b"ASCII85Decode" => ascii85_decode(&data)?,
            b"FlateDecode" => flate_decode(&data)?,
            _ => return None, // an unsupported pre-filter — skip this figure
        };
        if data.len() > MAX_FIGURE_BYTES {
            return None;
        }
    }
    // Trust the bytes, not the dictionary: a real JPEG starts with SOI.
    data.starts_with(&[0xFF, 0xD8, 0xFF]).then_some(data)
}

/// A raster figure pulled from a PDF page.
pub(crate) struct Figure {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub ext: &'static str,
    pub width: i64,
    pub height: i64,
    pub page: i64,
}

/// Pull embedded raster images (image XObjects) out of a PDF, page by page.
///
/// Best effort by design: a paper's figures are a bonus, so any parse failure
/// yields an empty list rather than failing the upload. Only formats we can
/// serve directly are kept — DCTDecode is a JPEG stream and JPXDecode/raw
/// bitmaps are skipped rather than re-encoded (no image codec dependency).
pub(crate) fn extract_figures(bytes: &[u8]) -> Vec<Figure> {
    let Ok(doc) = lopdf::Document::load_mem(bytes) else {
        return Vec::new();
    };
    let mut figures = Vec::new();

    // The same XObject (a logo, a shared chart) is usually referenced from many
    // pages; extract each object at most once.
    let mut seen: std::collections::HashSet<lopdf::ObjectId> = std::collections::HashSet::new();

    for (page_number, page_id) in doc.get_pages() {
        if figures.len() >= MAX_FIGURES_PER_DOC {
            break;
        }
        // get_page_resources returns (inline dict, indirect resource ids). Most
        // real PDFs put /Resources behind an indirect reference, so consulting
        // only the inline half finds nothing on them.
        let Ok((inline, indirect_ids)) = doc.get_page_resources(page_id) else {
            continue;
        };
        let mut resource_dicts: Vec<&lopdf::Dictionary> = Vec::new();
        if let Some(dict) = inline {
            resource_dicts.push(dict);
        }
        for id in indirect_ids {
            if let Ok(dict) = doc.get_dictionary(id) {
                resource_dicts.push(dict);
            }
        }

        for resources in resource_dicts {
            let xobjects = resources
                .get(b"XObject")
                .ok()
                .and_then(|obj| doc.dereference(obj).ok())
                .and_then(|(_, obj)| obj.as_dict().ok());
            let Some(xobjects) = xobjects else { continue };

            for (_, value) in xobjects.iter() {
                if figures.len() >= MAX_FIGURES_PER_DOC {
                    break;
                }
                // Dedup by object id when the reference is indirect.
                if let lopdf::Object::Reference(oid) = value {
                    if !seen.insert(*oid) {
                        continue;
                    }
                }
                let Ok(stream) = doc
                    .dereference(value)
                    .ok()
                    .map(|(_, obj)| obj)
                    .ok_or(())
                    .and_then(|obj| obj.as_stream().map_err(|_| ()))
                else {
                    continue;
                };
                let dict = &stream.dict;
                if dict.get(b"Subtype").and_then(|s| s.as_name()).ok() != Some(b"Image".as_ref()) {
                    continue;
                }
                let width = dict.get(b"Width").and_then(|w| w.as_i64()).unwrap_or(0);
                let height = dict.get(b"Height").and_then(|h| h.as_i64()).unwrap_or(0);
                if width < MIN_FIGURE_PX as i64 || height < MIN_FIGURE_PX as i64 {
                    continue; // icon / rule / spacer
                }

                // The filter tells us what the raw stream bytes actually are.
                let filters = dict
                    .get(b"Filter")
                    .map(|f| match f {
                        lopdf::Object::Name(name) => vec![name.clone()],
                        lopdf::Object::Array(items) => items
                            .iter()
                            .filter_map(|i| i.as_name().ok().map(<[u8]>::to_vec))
                            .collect(),
                        _ => Vec::new(),
                    })
                    .unwrap_or_default();
                if stream.content.len() > MAX_FIGURE_BYTES || stream.content.is_empty() {
                    continue;
                }
                // Only JPEG-bearing chains are kept: a raw/flate bitmap would need
                // re-encoding (and an image codec) to be servable — documented
                // limitation. jpeg_payload also unwraps ASCII85/Flate transport
                // filters that precede DCTDecode, and verifies the SOI magic.
                let Some(data) = jpeg_payload(&filters, &stream.content) else {
                    continue;
                };

                figures.push(Figure {
                    bytes: data,
                    mime: "image/jpeg",
                    ext: "jpg",
                    width,
                    height,
                    page: page_number as i64,
                });
            }
        }
    }

    figures
}

fn text_path(state: &AppState, user_id: &str, id: &str) -> Option<PathBuf> {
    (safe_segment(user_id) && safe_segment(id)).then(|| {
        state
            .data_dir
            .join("research")
            .join(user_id)
            .join(format!("{id}.txt"))
    })
}

pub async fn upload(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    mut multipart: Multipart,
) -> Response {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM research_docs WHERE user_id = ?1")
        .bind(&user_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    if count >= MAX_DOCS_PER_USER {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "research library is full — delete a document first",
        );
    }

    let field = loop {
        match multipart.next_field().await {
            Ok(Some(field)) if field.name() == Some("file") => break field,
            Ok(Some(_)) => continue,
            Ok(None) => return json_error(StatusCode::UNPROCESSABLE_ENTITY, "no file field"),
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "malformed upload"),
        }
    };
    let original_name = sanitize_name(field.file_name().unwrap_or("document"));
    let bytes = match field.bytes().await {
        Ok(bytes) => bytes,
        Err(_) => return json_error(StatusCode::PAYLOAD_TOO_LARGE, "upload too large"),
    };
    if bytes.len() > MAX_UPLOAD_BYTES {
        return json_error(StatusCode::PAYLOAD_TOO_LARGE, "file exceeds 10 MB");
    }

    // PDF parsing is CPU-bound and can take seconds on a big paper — run it on
    // the blocking pool so it never stalls the async runtime for other users.
    // spawn_blocking also isolates a panic to this task.
    let parse_bytes = bytes.to_vec();
    let parsed = tokio::task::spawn_blocking(move || {
        extract(&parse_bytes).map(|(mime, text)| (mime, text, parse_bytes))
    })
    .await;
    let (mime, text, bytes) = match parsed {
        Ok(Ok(triple)) => triple,
        Ok(Err(err)) => return err.into_response(),
        // The worker panicked despite catch_unwind (or was cancelled).
        Err(_) => return ExtractError::Unreadable.into_response(),
    };
    let text = cap_chars(&text, MAX_TEXT_CHARS);
    let char_count = text.chars().count() as i64;

    let id = Uuid::new_v4().to_string();
    let Some(path) = text_path(&state, &user_id, &id) else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error");
    };
    if let Some(dir) = path.parent() {
        if tokio::fs::create_dir_all(dir).await.is_err() {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error");
        }
    }
    if tokio::fs::write(&path, text.as_bytes()).await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error");
    }

    let created_at = now_rfc3339();
    if sqlx::query(
        "INSERT INTO research_docs (id, user_id, original_name, mime, char_count, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(&user_id)
    .bind(&original_name)
    .bind(mime)
    .bind(char_count)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .is_err()
    {
        let _ = tokio::fs::remove_file(&path).await;
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }

    // Figures are best-effort: a paper's charts are a bonus, so a parse failure
    // must never fail the upload (the text is what matters).
    if mime == "application/pdf" {
        store_figures(&state, &user_id, &id, &bytes, &created_at).await;
    }

    (
        StatusCode::CREATED,
        Json(DocView {
            id,
            original_name,
            mime: mime.to_owned(),
            char_count,
            created_at,
        }),
    )
        .into_response()
}

pub async fn list(State(state): State<AppState>, AuthUser(user_id): AuthUser) -> Response {
    match sqlx::query_as::<_, DocRow>(
        "SELECT id, original_name, mime, char_count, created_at FROM research_docs \
         WHERE user_id = ?1 ORDER BY created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => Json(rows.into_iter().map(DocView::from).collect::<Vec<_>>()).into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewView {
    id: String,
    original_name: String,
    snippet: String,
    char_count: i64,
}

/// A bounded snippet — the full text is never returned in bulk by the API.
pub async fn preview(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    let row = match sqlx::query_as::<_, DocRow>(
        "SELECT id, original_name, mime, char_count, created_at FROM research_docs \
         WHERE id = ?1 AND user_id = ?2",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };

    let Some(path) = text_path(&state, &user_id, &id) else {
        return not_found();
    };
    let text = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut end = PREVIEW_BYTES.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    Json(PreviewView {
        id: row.id,
        original_name: row.original_name,
        snippet: text[..end].to_owned(),
        char_count: row.char_count,
    })
    .into_response()
}

pub async fn delete_doc(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    // CHILDREN FIRST: `research_figures.doc_id` REFERENCES `research_docs(id)`
    // and the pool enables `foreign_keys(true)`, so deleting the parent first
    // would fail with a FK violation and the document could never be removed.
    // Both statements are double-scoped by id AND user_id, so one user can
    // never delete another's.
    if sqlx::query("DELETE FROM research_figures WHERE doc_id = ?1 AND user_id = ?2")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    let deleted = sqlx::query("DELETE FROM research_docs WHERE id = ?1 AND user_id = ?2")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.db)
        .await;
    match deleted {
        Ok(result) if result.rows_affected() > 0 => {
            if let Some(path) = text_path(&state, &user_id, &id) {
                let _ = tokio::fs::remove_file(path).await;
            }
            // Figures live in <data>/research/<user>/<doc_id>/ — drop the whole
            // directory so no file is orphaned.
            if safe_segment(&user_id) && safe_segment(&id) {
                let dir = state.data_dir.join("research").join(&user_id).join(&id);
                let _ = tokio::fs::remove_dir_all(dir).await;
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(_) => not_found(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

// --- figures ---------------------------------------------------------------

/// Extract and persist a PDF's figures. Best-effort: every failure is swallowed
/// so a malformed image can never fail an otherwise-good upload.
async fn store_figures(
    state: &AppState,
    user_id: &str,
    doc_id: &str,
    pdf: &[u8],
    created_at: &str,
) {
    // Also CPU-bound, and lopdf can panic on malformed input — keep both off
    // the async runtime and contained.
    let pdf = pdf.to_vec();
    let figures = match tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(|| extract_figures(&pdf)).unwrap_or_default()
    })
    .await
    {
        Ok(figures) => figures,
        Err(_) => return, // best-effort: never fail the upload over figures
    };
    if figures.is_empty() {
        return;
    }
    let Some(dir) = figure_path(state, user_id, doc_id, "x.jpg")
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
    else {
        return;
    };
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return;
    }

    for (index, figure) in figures.into_iter().enumerate() {
        let id = Uuid::new_v4().to_string();
        let filename = format!("fig-{index}-{id}.{}", figure.ext);
        if tokio::fs::write(dir.join(&filename), &figure.bytes)
            .await
            .is_err()
        {
            continue;
        }
        let inserted = sqlx::query(
            "INSERT INTO research_figures \
             (id, doc_id, user_id, filename, mime, width, height, page, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&id)
        .bind(doc_id)
        .bind(user_id)
        .bind(&filename)
        .bind(figure.mime)
        .bind(figure.width)
        .bind(figure.height)
        .bind(figure.page)
        .bind(created_at)
        .execute(&state.db)
        .await;
        if inserted.is_err() {
            let _ = tokio::fs::remove_file(dir.join(&filename)).await;
        }
    }
}

// `pub(crate)` (with pub(crate) fields) so the pub(crate) data helpers can
// return it — a pub(crate) fn returning a module-private type trips
// `private_interfaces`, which `-D warnings` turns into a build failure.
#[derive(sqlx::FromRow)]
pub(crate) struct FigureRow {
    pub(crate) id: String,
    pub(crate) doc_id: String,
    pub(crate) filename: String,
    pub(crate) mime: String,
    pub(crate) width: i64,
    pub(crate) height: i64,
    pub(crate) page: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FigureView {
    id: String,
    url: String,
    mime: String,
    width: i64,
    height: i64,
    page: i64,
}

fn figure_path(state: &AppState, user_id: &str, doc_id: &str, filename: &str) -> Option<PathBuf> {
    (safe_segment(user_id) && safe_segment(doc_id) && safe_segment(filename)).then(|| {
        state
            .data_dir
            .join("research")
            .join(user_id)
            .join(doc_id)
            .join(filename)
    })
}

/// Figures for one of the caller's documents (owner-scoped).
pub(crate) async fn figures_data(
    state: &AppState,
    user_id: &str,
    doc_id: &str,
) -> Result<Vec<FigureRow>, ()> {
    sqlx::query_as::<_, FigureRow>(
        "SELECT id, doc_id, filename, mime, width, height, page FROM research_figures \
         WHERE doc_id = ?1 AND user_id = ?2 ORDER BY page, filename",
    )
    .bind(doc_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| ())
}

pub async fn figures(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(doc_id): Path<String>,
) -> Response {
    // A doc that isn't ours is indistinguishable from a missing one.
    let owns: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM research_docs WHERE id = ?1 AND user_id = ?2")
            .bind(&doc_id)
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    if owns.is_none() {
        return not_found();
    }
    match figures_data(&state, &user_id, &doc_id).await {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|r| FigureView {
                    url: format!("/research/{user_id}/{}/{}", r.doc_id, r.filename),
                    id: r.id,
                    mime: r.mime,
                    width: r.width,
                    height: r.height,
                    page: r.page,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

/// Serve a figure file to its owner only (session-scoped, like `/assets`).
pub async fn serve_figure(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path((owner, doc_id, filename)): Path<(String, String, String)>,
) -> Response {
    if owner != user_id {
        return not_found();
    }
    let Some(path) = figure_path(&state, &user_id, &doc_id, &filename) else {
        return not_found();
    };
    let mime: Option<String> = sqlx::query_scalar(
        "SELECT mime FROM research_figures WHERE user_id = ?1 AND doc_id = ?2 AND filename = ?3",
    )
    .bind(&user_id)
    .bind(&doc_id)
    .bind(&filename)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some(mime) = mime else { return not_found() };
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return not_found();
    };
    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, mime),
            (
                axum::http::header::CACHE_CONTROL,
                "private, max-age=31536000, immutable".to_owned(),
            ),
            (
                axum::http::header::HeaderName::from_static("x-content-type-options"),
                "nosniff".to_owned(),
            ),
        ],
        bytes,
    )
        .into_response()
}

/// Copy a research figure into a deck's assets so it can be used as a slide
/// image. Both the deck AND the figure must belong to the caller.
pub(crate) async fn attach_figure_data(
    state: &AppState,
    user_id: &str,
    deck_id: &str,
    figure_id: &str,
) -> Result<crate::assets::StoredAsset, StatusCode> {
    let owns_deck: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM decks WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(deck_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    if owns_deck.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let figure: Option<FigureRow> = sqlx::query_as::<_, FigureRow>(
        "SELECT id, doc_id, filename, mime, width, height, page FROM research_figures \
         WHERE id = ?1 AND user_id = ?2",
    )
    .bind(figure_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some(figure) = figure else {
        return Err(StatusCode::NOT_FOUND);
    };

    let Some(path) = figure_path(state, user_id, &figure.doc_id, &figure.filename) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return Err(StatusCode::NOT_FOUND);
    };

    crate::assets::store_bytes(state, deck_id, &figure.filename, &bytes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn attach_figure(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path((deck_id, figure_id)): Path<(String, String)>,
) -> Response {
    match attach_figure_data(&state, &user_id, &deck_id, &figure_id).await {
        Ok(asset) => (StatusCode::CREATED, Json(asset)).into_response(),
        Err(StatusCode::NOT_FOUND) => not_found(),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error"),
    }
}

/// (id, name, char_count) for the caller's documents — the MCP `list_research`
/// shape, owner-scoped.
pub(crate) async fn docs_data(
    state: &AppState,
    user_id: &str,
) -> Result<Vec<(String, String, i64)>, ()> {
    sqlx::query_as::<_, (String, String, i64)>(
        "SELECT id, original_name, char_count FROM research_docs \
         WHERE user_id = ?1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| ())
}

/// (id, page, width, height) for one of the caller's documents. An unowned or
/// unknown doc yields Err — indistinguishable from missing.
pub(crate) async fn figures_meta(
    state: &AppState,
    user_id: &str,
    doc_id: &str,
) -> Result<Vec<(String, i64, i64, i64)>, ()> {
    let owns: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM research_docs WHERE id = ?1 AND user_id = ?2")
            .bind(doc_id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| ())?;
    if owns.is_none() {
        return Err(());
    }
    sqlx::query_as::<_, (String, i64, i64, i64)>(
        "SELECT id, page, width, height FROM research_figures \
         WHERE doc_id = ?1 AND user_id = ?2 ORDER BY page, filename",
    )
    .bind(doc_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| ())
}

/// Load the extracted text for the caller's OWN documents, in the order given,
/// budgeted to `budget` chars in total. Ids that are not the caller's are
/// silently skipped — another user's research can never enter a prompt.
pub(crate) async fn context_for(
    state: &AppState,
    user_id: &str,
    ids: &[String],
    budget: usize,
) -> String {
    if ids.is_empty() || budget == 0 {
        return String::new();
    }
    // Bound how many ids one request may name: each costs a query + a file
    // read, so an authenticated client must not be able to amplify a single
    // generate into thousands of them.
    let ids = &ids[..ids.len().min(MAX_DOCS_PER_USER as usize)];
    let per_doc = budget / ids.len().max(1);
    let mut out = String::new();
    for id in ids.iter().take(MAX_DOCS_PER_USER as usize) {
        let row = sqlx::query_as::<_, DocRow>(
            "SELECT id, original_name, mime, char_count, created_at FROM research_docs \
             WHERE id = ?1 AND user_id = ?2",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await;
        let Ok(Some(row)) = row else { continue }; // not ours → skip silently
        let Some(path) = text_path(state, user_id, id) else {
            continue;
        };
        let Ok(text) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let slice = cap_bytes(&text, per_doc.max(1));
        out.push_str(&format!(
            "--- Source: {} ---\n{}\n\n",
            row.original_name, slice
        ));
    }
    cap_bytes(&out, budget)
}

#[cfg(test)]
mod tests {
    use super::{cap_bytes, cap_chars, extract, sanitize_name, tidy};

    #[test]
    fn plain_text_is_accepted_and_tidied() {
        let (mime, text) = extract(b"Hello\n\n\n\n  world  \n").unwrap();
        assert_eq!(mime, "text/plain");
        // Runs of blank lines collapse to ONE — the paragraph break is kept.
        assert_eq!(text, "Hello\n\nworld");
    }

    #[test]
    fn binary_that_is_not_pdf_is_rejected() {
        // A PNG must not sneak in as "text".
        assert_eq!(
            extract(&[0x89, 0x50, 0x4E, 0x47, 0x00, 0x01]).unwrap_err(),
            super::ExtractError::Unsupported
        );
        assert_eq!(extract(b"").unwrap_err(), super::ExtractError::Unsupported);
    }

    #[test]
    fn an_unparseable_pdf_is_reported_as_unreadable_not_a_crash() {
        // Starts with the PDF magic but is garbage — must be an error, not a panic.
        let err = extract(b"%PDF-1.7\nnot really a pdf").unwrap_err();
        assert!(matches!(
            err,
            super::ExtractError::Unreadable | super::ExtractError::NoText
        ));
    }

    #[test]
    fn caps_on_a_char_boundary_with_thai() {
        let thai = "สวัสดีชาวโลก";
        let capped = cap_chars(thai, 5);
        assert_eq!(capped.chars().count(), 5);
        assert!(thai.starts_with(&capped));
    }

    #[test]
    fn byte_cap_respects_the_budget_for_thai_not_the_char_count() {
        // Thai is ~3 bytes/char: a char-based cap would send ~3x the budget.
        let thai = "สวัสดีชาวโลก".repeat(50);
        let capped = cap_bytes(&thai, 30);
        assert!(capped.len() <= 30, "byte budget exceeded: {}", capped.len());
        assert!(thai.starts_with(&capped)); // still a valid UTF-8 prefix
                                            // Short text is returned untouched.
        assert_eq!(cap_bytes("abc", 100), "abc");
    }

    #[test]
    fn sanitizes_the_display_name() {
        assert_eq!(sanitize_name("../../etc/passwd"), "....etcpasswd");
        assert_eq!(sanitize_name("   "), "document");
        assert_eq!(sanitize_name("paper.pdf"), "paper.pdf");
    }

    #[test]
    fn ascii85_roundtrip_and_jpeg_chain() {
        use super::{ascii85_decode, jpeg_payload};
        // "sure." encodes to "F*2M7/c" in ASCII85 (verified against Python's
        // base64.a85encode) — the trailing partial group must decode correctly.
        assert_eq!(ascii85_decode(b"F*2M7/c~>").unwrap(), b"sure.");
        // Non-ASCII85 input is rejected rather than producing garbage.
        assert!(ascii85_decode(&[0xFF, 0x00]).is_none());

        // A [/ASCII85Decode /DCTDecode] chain must yield the real JPEG bytes —
        // writing the RAW stream would produce an invalid .jpg on disk.
        let jpeg = [0xFFu8, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let encoded = ascii85_encode(&jpeg);
        let filters = vec![b"ASCII85Decode".to_vec(), b"DCTDecode".to_vec()];
        assert_eq!(jpeg_payload(&filters, encoded.as_bytes()).unwrap(), jpeg);

        // A chain without DCTDecode (a raw bitmap) is skipped.
        assert!(jpeg_payload(&[b"FlateDecode".to_vec()], &jpeg).is_none());
        // DCTDecode alone: the raw content already IS the JPEG.
        assert_eq!(jpeg_payload(&[b"DCTDecode".to_vec()], &jpeg).unwrap(), jpeg);
        // Bytes that don't start with the JPEG SOI marker are refused.
        assert!(jpeg_payload(&[b"DCTDecode".to_vec()], b"not a jpeg").is_none());
    }

    /// Minimal ASCII85 encoder, test-only, to build the chain fixture above.
    fn ascii85_encode(data: &[u8]) -> String {
        let mut out = String::new();
        for chunk in data.chunks(4) {
            let mut buf = [0u8; 4];
            buf[..chunk.len()].copy_from_slice(chunk);
            let value = u32::from_be_bytes(buf);
            let mut digits = [0u8; 5];
            let mut v = value;
            for i in (0..5).rev() {
                digits[i] = (v % 85) as u8;
                v /= 85;
            }
            for d in digits.iter().take(chunk.len() + 1) {
                out.push((d + b'!') as char);
            }
        }
        out.push_str("~>");
        out
    }

    #[test]
    fn tidy_collapses_blank_runs_to_one() {
        // Paragraph breaks survive; runs of 2+ blanks collapse to a single one.
        assert_eq!(tidy("a\n\n\n\nb"), "a\n\nb");
        assert_eq!(tidy("a\nb"), "a\nb");
        assert_eq!(tidy("  a  \n\n  b  "), "a\n\nb");
    }
}
