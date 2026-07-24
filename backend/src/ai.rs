//! AI slide generation (BRIEF-0010).
//!
//! This is the SECOND — and only other — permitted outbound call in the app
//! (CLAUDE.md §2, amended): a server-side request to the ADMIN-CONFIGURED LLM
//! endpoint, made only for an authenticated user's explicit request. The
//! browser never talks to the provider, so viewer-facing pages (share links,
//! present, print/PDF) still make zero external requests.

use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::decks::AuthUser;
use crate::fonts::read_capped;
use crate::settings::{self, PROVIDER_ANTHROPIC, PROVIDER_GEMINI};
use crate::{json_error, AppState};

const MAX_PROMPT_BYTES: usize = 4 * 1024;
/// Decks reach 1 MB; echoing one back to the provider is both a cost and a
/// payload bomb, so only a bounded slice of context is ever sent.
const MAX_EXISTING_BYTES: usize = 32 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const TIMEOUT_SECS: u64 = 60;
/// Longest a request may wait for a free generation slot before being told to
/// retry (keeps a backlog from starving other users indefinitely).
const SEM_WAIT_SECS: u64 = 30;
const MAX_TOKENS: u32 = 4000;
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Minimum gap between generations per user — a global semaphore alone would
/// still let one signed-in user burn the instance's provider budget.
const USER_MIN_INTERVAL: Duration = Duration::from_secs(5);

const SYSTEM_PROMPT: &str = "You write presentation decks as standalone Marp Markdown. \
Reply with ONLY the Markdown — no explanation, no surrounding code fences. \
Begin with a YAML frontmatter block delimited by --- containing `marp: true` and `theme: deckoala`. \
Separate every slide with a line containing exactly ---. \
Use # for the title slide and ## for subsequent slide headings. Keep slides concise.";

/// Join an admin-supplied base with a provider path without doubling the API
/// version segment (an admin may paste `https://host/v1`).
fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let base = base.strip_suffix("/v1").unwrap_or(base);
    format!("{base}{path}")
}

/// Gemini's generateContent URL: `{base}/v1beta/models/{model}:generateContent`.
/// The API version segment is `v1beta` (not `/v1`), and the `:generateContent`
/// action must survive, so this does NOT reuse `join_url`. The key is never put
/// here — it goes in the `x-goog-api-key` header.
fn gemini_url(base: &str, model: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/v1beta/models/{model}:generateContent")
}

/// Concatenate the text parts of Gemini's first candidate.
fn gemini_text(value: &serde_json::Value) -> String {
    value["candidates"][0]["content"]["parts"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<String>()
        })
        .unwrap_or_default()
}

/// Models often wrap output in a fenced block despite instructions. Only strip
/// when the fence is actually CLOSED — otherwise a deck that legitimately opens
/// with a code block would lose its opening fence line.
fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed.to_owned();
    };
    // Drop the (optional) language tag on the opening fence.
    let Some((_, body)) = rest.split_once('\n') else {
        return trimmed.to_owned();
    };
    match body.trim_end().strip_suffix("```") {
        Some(inner) => inner.trim().to_owned(),
        None => trimmed.to_owned(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    prompt: String,
    #[serde(default)]
    existing_markdown: Option<String>,
}

#[derive(Serialize)]
struct GenerateResponse {
    markdown: String,
}

/// Per-user throttle. Returns false when the caller is too soon.
async fn allow_user(state: &AppState, user_id: &str) -> bool {
    let mut seen = state.ai_last_call.lock().await;
    let now = Instant::now();
    if let Some(prev) = seen.get(user_id) {
        if now.duration_since(*prev) < USER_MIN_INTERVAL {
            return false;
        }
    }
    // Bound the map so a large user base can't grow it without limit.
    if seen.len() > 10_000 {
        seen.retain(|_, t| now.duration_since(*t) < USER_MIN_INTERVAL);
    }
    seen.insert(user_id.to_owned(), now);
    true
}

pub async fn generate(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<GenerateRequest>,
) -> Response {
    let cfg = settings::ai_config(&state.db).await;
    if !cfg.is_usable() {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "AI is not configured on this instance",
        );
    }

    let prompt = body.prompt.trim().to_owned();
    if prompt.is_empty() {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "prompt is required");
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "prompt is too long");
    }
    // A deck may be up to 1 MB while the editor sends the whole thing as
    // context by default — truncate rather than rejecting the request.
    let mut existing = body.existing_markdown.unwrap_or_default();
    if existing.len() > MAX_EXISTING_BYTES {
        let mut end = MAX_EXISTING_BYTES;
        while end > 0 && !existing.is_char_boundary(end) {
            end -= 1;
        }
        existing.truncate(end);
    }

    if !allow_user(&state, &user_id).await {
        return json_error(
            StatusCode::TOO_MANY_REQUESTS,
            "please wait a moment before generating again",
        );
    }

    let user_content = if existing.trim().is_empty() {
        prompt.clone()
    } else {
        format!("Existing deck:\n\n{existing}\n\n---\n\nRequest: {prompt}")
    };

    let key = cfg.api_key.clone().unwrap_or_default();
    let anthropic = cfg.provider == PROVIDER_ANTHROPIC;
    let gemini = cfg.provider == PROVIDER_GEMINI;
    let (url, payload) = if anthropic {
        (
            join_url(&cfg.base_url, "/v1/messages"),
            serde_json::json!({
                "model": cfg.model,
                "max_tokens": MAX_TOKENS,
                "system": SYSTEM_PROMPT,
                "messages": [{ "role": "user", "content": user_content }],
            }),
        )
    } else if gemini {
        (
            gemini_url(&cfg.base_url, &cfg.model),
            serde_json::json!({
                "system_instruction": { "parts": [{ "text": SYSTEM_PROMPT }] },
                "contents": [{ "role": "user", "parts": [{ "text": user_content }] }],
                "generationConfig": { "maxOutputTokens": MAX_TOKENS },
            }),
        )
    } else {
        (
            join_url(&cfg.base_url, "/v1/chat/completions"),
            serde_json::json!({
                "model": cfg.model,
                "max_tokens": MAX_TOKENS,
                "messages": [
                    { "role": "system", "content": SYSTEM_PROMPT },
                    { "role": "user", "content": user_content },
                ],
            }),
        )
    };

    // Bound concurrency so a burst can't exhaust the instance. The wait itself
    // is time-boxed: the reqwest timeout starts only after the permit is
    // granted, so an unbounded queue would otherwise stall other users forever.
    let _permit = match tokio::time::timeout(
        Duration::from_secs(SEM_WAIT_SECS),
        state.ai_sem.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "ai unavailable"),
        Err(_) => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "the AI queue is busy — try again shortly",
            )
        }
    };

    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build()
    {
        Ok(client) => client,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "client error"),
    };
    // reqwest is built without its `json` feature here — serialize by hand.
    let mut request = client
        .post(&url)
        .header(header::CONTENT_TYPE, "application/json")
        .body(payload.to_string());
    request = if anthropic {
        request
            .header("x-api-key", &key)
            .header("anthropic-version", ANTHROPIC_VERSION)
    } else if gemini {
        // The key travels in a header, NEVER the ?key= query param (keys must
        // never appear in a URL — privacy + our key-hygiene posture).
        request.header("x-goog-api-key", &key)
    } else {
        request.header(header::AUTHORIZATION, format!("Bearer {key}"))
    };

    let response = match request.send().await {
        Ok(response) => response,
        // Never surface the provider error verbatim — it can echo the key.
        Err(_) => return json_error(StatusCode::BAD_GATEWAY, "could not reach the AI provider"),
    };
    let status = response.status();
    let Some(bytes) = read_capped(response, MAX_RESPONSE_BYTES).await else {
        return json_error(
            StatusCode::BAD_GATEWAY,
            "AI response was incomplete or too large",
        );
    };
    if !status.is_success() {
        tracing::error!("ai provider returned {}", status.as_u16());
        return json_error(
            StatusCode::BAD_GATEWAY,
            "the AI provider rejected the request",
        );
    }

    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return json_error(StatusCode::BAD_GATEWAY, "unreadable AI response");
    };
    let text: String = if anthropic {
        value["content"]
            .as_array()
            .and_then(|parts| {
                parts.iter().find_map(|part| {
                    if part["type"] == "text" {
                        part["text"].as_str()
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default()
            .to_owned()
    } else if gemini {
        gemini_text(&value)
    } else {
        value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };

    let markdown = strip_code_fence(&text);
    if markdown.is_empty() {
        return json_error(StatusCode::BAD_GATEWAY, "the AI returned no content");
    }
    Json(GenerateResponse { markdown }).into_response()
}

#[cfg(test)]
mod tests {
    use super::{gemini_text, gemini_url, join_url, strip_code_fence};

    #[test]
    fn gemini_url_targets_generatecontent_and_never_carries_the_key() {
        let url = gemini_url(
            "https://generativelanguage.googleapis.com",
            "gemini-2.0-flash",
        );
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent"
        );
        // The API key must never appear in the URL.
        assert!(!url.contains("key="));
        // A trailing slash on the base is tolerated, and :generateContent survives.
        assert_eq!(
            gemini_url("https://host/", "m"),
            "https://host/v1beta/models/m:generateContent"
        );
    }

    #[test]
    fn gemini_text_concatenates_candidate_parts() {
        let value = serde_json::json!({
            "candidates": [{ "content": { "parts": [{ "text": "# Hi\n" }, { "text": "## More" }] } }]
        });
        assert_eq!(gemini_text(&value), "# Hi\n## More");
        // Missing/empty shape → empty string, which the caller turns into a 502.
        assert_eq!(gemini_text(&serde_json::json!({})), "");
    }

    #[test]
    fn join_url_never_doubles_the_version_segment() {
        assert_eq!(
            join_url("https://api.anthropic.com", "/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
        // An admin pasting a base that already ends in /v1 must not double it.
        assert_eq!(
            join_url("https://api.openai.com/v1", "/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            join_url("http://localhost:11434/", "/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn strips_fenced_output() {
        assert_eq!(strip_code_fence("```markdown\n# Hi\n```"), "# Hi");
        assert_eq!(strip_code_fence("```\n# Hi\n```"), "# Hi");
        assert_eq!(strip_code_fence("# Hi"), "# Hi");
        assert_eq!(strip_code_fence("  # Hi  "), "# Hi");
    }
}
