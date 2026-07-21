//! PDF export via headless Chromium (BRIEF-0006).
//!
//! The export POST is session-owner-scoped and drives the container's Chromium
//! to print a token-authorized `/print/{id}` view of the app itself, so the PDF
//! is rendered by the SAME Marp pipeline as the editor/preview (ADR-0002/0003).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::assets::safe_segment;
use crate::decks::AuthUser;
use crate::{content_disposition, json_error, AppState};

pub(crate) const PRINT_COOKIE: &str = "deckoala_print";
const TOKEN_TTL_SECS: u64 = 120;
const READY_TIMEOUT_SECS: u64 = 20;

type HmacSha256 = Hmac<Sha256>;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn sign(secret: &[u8; 32], deck_id: &str, expiry: &str) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(deck_id.as_bytes());
    mac.update(b"|");
    mac.update(expiry.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Stateless bearer authorizing print-time read of ONE deck for a short window.
pub fn mint_print_token(secret: &[u8; 32], deck_id: &str, ttl_secs: u64) -> String {
    let expiry = (now_unix() + ttl_secs).to_string();
    let sig = hex(&sign(secret, deck_id, &expiry));
    format!("{deck_id}.{expiry}.{sig}")
}

/// Returns the token's deck_id iff the HMAC verifies (constant-time) and it
/// has not expired. deck ids are uuids (no dots), so a 3-field split is exact.
pub fn verify_print_token(secret: &[u8; 32], token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let (deck_id, expiry, sig_hex) = (parts[0], parts[1], parts[2]);
    let expiry_secs: u64 = expiry.parse().ok()?;
    if now_unix() > expiry_secs {
        return None;
    }
    let provided = hex_decode(sig_hex)?;
    let mut mac = HmacSha256::new_from_slice(secret).ok()?;
    mac.update(deck_id.as_bytes());
    mac.update(b"|");
    mac.update(expiry.as_bytes());
    mac.verify_slice(&provided).ok()?; // constant-time
    Some(deck_id.to_owned())
}

/// Read a single cookie value from the `Cookie` header.
pub(crate) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

#[derive(sqlx::FromRow)]
struct DeckExport {
    title: String,
    markdown: String,
    theme: String,
}

fn pdf_response(bytes: Vec<u8>, title: &str) -> Response {
    let mut response = (StatusCode::OK, bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    if let Ok(value) = HeaderValue::from_str(&content_disposition(title, "pdf")) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

pub async fn export_pdf(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(id): Path<String>,
) -> Response {
    // Defense-in-depth: never build a filesystem path from an unvalidated
    // segment (mirrors assets.rs; the owner-scoped DB match is the real gate).
    if !safe_segment(&id) {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    // Owner check BEFORE any Chromium work (so auth tests never launch a browser).
    let owns: Option<i64> = match sqlx::query_scalar(
        "SELECT 1 FROM decks WHERE id = ?1 AND owner_id = ?2 AND deleted_at IS NULL",
    )
    .bind(&id)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(owns) => owns,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    if owns.is_none() {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    render_deck_pdf(&state, &id, &state.export_sem).await
}

/// Render (or serve the cached) PDF for a deck the caller has ALREADY been
/// authorized to export (owner check or share-token resolve). `sem` bounds
/// concurrency: the owner path passes `export_sem`, the anonymous share-token
/// path a separate, tighter `share_export_sem` so a leaked edit link's
/// cache-busting exports cannot starve owner exports (BRIEF-0008).
pub(crate) async fn render_deck_pdf(
    state: &AppState,
    id: &str,
    sem: &std::sync::Arc<tokio::sync::Semaphore>,
) -> Response {
    if !safe_segment(id) {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    let deck: Option<DeckExport> = match sqlx::query_as(
        "SELECT title, markdown, theme FROM decks WHERE id = ?1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(deck) => deck,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    let Some(deck) = deck else {
        return json_error(StatusCode::NOT_FOUND, "not found");
    };

    // Cache: identical (markdown+theme) → identical PDF, keyed by content hash.
    let mut hasher = Sha256::new();
    hasher.update(deck.markdown.as_bytes());
    hasher.update(b"\n");
    hasher.update(deck.theme.as_bytes());
    let hash = hex(&hasher.finalize());
    let dir = state.data_dir.join("exports").join(id);
    let cache_path = dir.join(format!("{hash}.pdf"));
    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        return pdf_response(bytes, &deck.title);
    }

    // Acquire the permit BEFORE minting the token, so the 120s TTL covers only
    // the browser drive, never the queue wait.
    let _permit = match sem.clone().acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "export unavailable"),
    };
    let token = mint_print_token(&state.print_secret, id, TOKEN_TTL_SECS);

    match render_pdf(&state.local_addr, id, &token).await {
        Ok(bytes) => {
            // Atomic cache write: a temp file + rename, so an interrupted or
            // concurrent write can never leave a truncated PDF at cache_path
            // that later exports would serve as a valid download.
            let _ = tokio::fs::create_dir_all(&dir).await;
            let tmp = dir.join(format!("{hash}.{}.tmp", uuid::Uuid::new_v4()));
            if tokio::fs::write(&tmp, &bytes).await.is_ok() {
                let _ = tokio::fs::rename(&tmp, &cache_path).await;
            }
            pdf_response(bytes, &deck.title)
        }
        Err(err) => {
            tracing::error!("pdf export failed: {err}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "pdf generation failed")
        }
    }
}

/// Read-only deck data for the print view; authorized ONLY by a valid print
/// cookie whose token deck_id matches the path (never the session).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrintDeck {
    title: String,
    markdown: String,
    theme: String,
}

pub async fn print_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let authorized = cookie_value(&headers, PRINT_COOKIE)
        .and_then(|token| verify_print_token(&state.print_secret, &token))
        .as_deref()
        == Some(id.as_str());
    if !authorized {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    match sqlx::query_as::<_, DeckExport>(
        "SELECT title, markdown, theme FROM decks WHERE id = ?1 AND deleted_at IS NULL",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(deck)) => Json(PrintDeck {
            title: deck.title,
            markdown: deck.markdown,
            theme: deck.theme,
        })
        .into_response(),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "not found"),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

/// True iff the request carries a valid print cookie for `deck_id` (used by the
/// asset serve route so Chromium can load `/assets/...` images for the PDF).
pub(crate) fn print_cookie_authorizes(
    secret: &[u8; 32],
    headers: &HeaderMap,
    deck_id: &str,
) -> bool {
    cookie_value(headers, PRINT_COOKIE)
        .and_then(|token| verify_print_token(secret, &token))
        .as_deref()
        == Some(deck_id)
}

// ---------------------------------------------------------------------------
// Chromium drive (only exercised at runtime — the cargo build stage has no
// browser). Wrapped so a leaked child can never accumulate.
// ---------------------------------------------------------------------------

/// Kills the browser + aborts its CDP handler on EVERY drop path (timeout,
/// error, panic, cancellation), so the export semaphore's memory bound holds.
struct BrowserGuard {
    browser: Option<chromiumoxide::Browser>,
    handler: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for BrowserGuard {
    fn drop(&mut self) {
        let handler = self.handler.take();
        if let Some(mut browser) = self.browser.take() {
            // Graceful close needs the CDP handler ALIVE to pump the command —
            // abort it first and close()/wait() would block forever. So keep
            // the handler until close/wait finish (bounded), THEN abort; the
            // final drop of `browser` kills any child still alive.
            tokio::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(5), browser.close()).await;
                let _ = tokio::time::timeout(Duration::from_secs(5), browser.wait()).await;
                if let Some(handler) = handler {
                    handler.abort();
                }
                drop(browser);
            });
        } else if let Some(handler) = handler {
            handler.abort();
        }
    }
}

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Time-bound the WHOLE drive (launch included) so no path — a hung Chromium
/// launch, a never-ready page — can pin the export permit. On timeout the
/// inner future is dropped and BrowserGuard::drop tears the browser down.
async fn render_pdf(local_addr: &str, deck_id: &str, token: &str) -> Result<Vec<u8>, BoxErr> {
    tokio::time::timeout(
        Duration::from_secs(READY_TIMEOUT_SECS + 10),
        render_inner(local_addr, deck_id, token),
    )
    .await
    .map_err(|_| -> BoxErr { "pdf render timed out".into() })?
}

async fn render_inner(local_addr: &str, deck_id: &str, token: &str) -> Result<Vec<u8>, BoxErr> {
    use chromiumoxide::browser::{Browser, BrowserConfig};
    use futures::StreamExt;

    let chrome_bin = std::env::var("CHROME_BIN").unwrap_or_else(|_| "chromium".into());
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_bin)
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        // Enforce the loopback-only contract at the browser layer: any non-
        // loopback host fails to resolve, so deck content can't SSRF out.
        .arg("--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1")
        .build()
        .map_err(|e| -> BoxErr { e.into() })?;

    let (browser, mut handler) = Browser::launch(config).await?;
    let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });
    let mut guard = BrowserGuard {
        browser: Some(browser),
        handler: Some(handler_task),
    };
    let browser = guard.browser.as_ref().expect("browser present");

    let pdf = drive(browser, local_addr, deck_id, token).await?;

    // Happy-path teardown: close BEFORE aborting the handler (the guard covers
    // every failure path with the same ordering).
    if let Some(mut browser) = guard.browser.take() {
        let _ = browser.close().await;
        let _ = browser.wait().await;
    }
    if let Some(handler) = guard.handler.take() {
        handler.abort();
    }
    Ok(pdf)
}

async fn drive(
    browser: &chromiumoxide::Browser,
    local_addr: &str,
    deck_id: &str,
    token: &str,
) -> Result<Vec<u8>, BoxErr> {
    use chromiumoxide::cdp::browser_protocol::network::CookieParam;
    use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;

    let base = format!("http://{local_addr}");
    let page = browser.new_page("about:blank").await?;

    // The cookie carries the print token to /api/print AND /assets/{deck}/*.
    let cookie = CookieParam::builder()
        .name(PRINT_COOKIE)
        .value(token)
        .url(base.clone())
        .build()
        .map_err(|e| -> BoxErr { e.into() })?;
    page.set_cookie(cookie).await?;

    page.goto(format!("{base}/print/{deck_id}")).await?;
    page.wait_for_navigation().await?;

    // The print page sets this only after fonts AND images are ready.
    loop {
        let ready: bool = page
            .evaluate("window.__DECKOALA_PRINT_READY === true")
            .await?
            .into_value()
            .unwrap_or(false);
        if ready {
            break;
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }

    let params = PrintToPdfParams::builder()
        .print_background(true)
        .prefer_css_page_size(true)
        .margin_top(0.0)
        .margin_bottom(0.0)
        .margin_left(0.0)
        .margin_right(0.0)
        .build();
    let pdf = page.pdf(params).await?;
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::{cookie_value, mint_print_token, verify_print_token, PRINT_COOKIE};
    use axum::http::{header, HeaderMap, HeaderValue};

    const SECRET: [u8; 32] = [7u8; 32];

    #[test]
    fn round_trips_a_fresh_token() {
        let token = mint_print_token(&SECRET, "deck-abc", 120);
        assert_eq!(
            verify_print_token(&SECRET, &token).as_deref(),
            Some("deck-abc")
        );
    }

    #[test]
    fn rejects_expired_tampered_and_foreign() {
        // Expired (ttl 0, already elapsed relative to "now > expiry").
        let expired = mint_print_token(&SECRET, "deck-abc", 0);
        // now_unix() == expiry is allowed (> check); force clearly-expired:
        let parts: Vec<&str> = expired.split('.').collect();
        let stale = format!("{}.{}.{}", parts[0], "1", parts[2]);
        assert!(verify_print_token(&SECRET, &stale).is_none());

        // Tampered signature.
        let token = mint_print_token(&SECRET, "deck-abc", 120);
        let mut tampered = token.clone();
        tampered.pop();
        tampered.push(if token.ends_with('a') { 'b' } else { 'a' });
        assert!(verify_print_token(&SECRET, &tampered).is_none());

        // Wrong secret.
        assert!(verify_print_token(&[9u8; 32], &token).is_none());

        // Malformed.
        assert!(verify_print_token(&SECRET, "not-a-token").is_none());
    }

    #[test]
    fn extracts_cookie_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("id=abc; deckoala_print=the-token; other=x"),
        );
        assert_eq!(
            cookie_value(&headers, PRINT_COOKIE).as_deref(),
            Some("the-token")
        );
        assert!(cookie_value(&headers, "missing").is_none());
    }
}
