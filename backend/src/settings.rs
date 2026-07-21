//! Instance settings (BRIEF-0010): key/value config, incl. the AI provider.
//!
//! The provider API key lives here and is **write-only** across the whole API
//! surface — no handler ever returns it, and it is never logged.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::decks::AdminUser;
use crate::{json_error, now_rfc3339, AppState};

/// Set at seed time ONLY when the built-in default password was used; cleared
/// when root changes their password.
pub(crate) const ROOT_PW_DEFAULT: &str = "root_password_is_default";
const AI_ENABLED: &str = "ai_enabled";
const AI_PROVIDER: &str = "ai_provider";
const AI_BASE_URL: &str = "ai_base_url";
const AI_MODEL: &str = "ai_model";
const AI_API_KEY: &str = "ai_api_key";

pub(crate) const PROVIDER_ANTHROPIC: &str = "anthropic";
pub(crate) const PROVIDER_OPENAI: &str = "openai";

pub(crate) async fn get(db: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

pub(crate) async fn set(db: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (key) DO UPDATE SET value = ?2, updated_at = ?3",
    )
    .bind(key)
    .bind(value)
    .bind(now_rfc3339())
    .execute(db)
    .await
    .map(|_| ())
}

async fn set_tx(
    conn: &mut sqlx::SqliteConnection,
    key: &str,
    value: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (key) DO UPDATE SET value = ?2, updated_at = ?3",
    )
    .bind(key)
    .bind(value)
    .bind(now_rfc3339())
    .execute(&mut *conn)
    .await
    .map(|_| ())
}

async fn remove_tx(conn: &mut sqlx::SqliteConnection, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM settings WHERE key = ?1")
        .bind(key)
        .execute(&mut *conn)
        .await
        .map(|_| ())
}

pub(crate) async fn remove(db: &SqlitePool, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM settings WHERE key = ?1")
        .bind(key)
        .execute(db)
        .await
        .map(|_| ())
}

pub(crate) async fn flag(db: &SqlitePool, key: &str) -> bool {
    get(db, key).await.as_deref() == Some("true")
}

/// Server-side AI config. `api_key` never leaves the backend.
pub(crate) struct AiConfig {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl AiConfig {
    /// Usable only when enabled AND fully configured.
    pub fn is_usable(&self) -> bool {
        self.enabled
            && !self.model.is_empty()
            && !self.base_url.is_empty()
            && self.api_key.as_deref().is_some_and(|k| !k.is_empty())
    }
}

pub(crate) async fn ai_config(db: &SqlitePool) -> AiConfig {
    AiConfig {
        enabled: flag(db, AI_ENABLED).await,
        provider: get(db, AI_PROVIDER)
            .await
            .unwrap_or_else(|| PROVIDER_ANTHROPIC.to_owned()),
        base_url: get(db, AI_BASE_URL).await.unwrap_or_default(),
        model: get(db, AI_MODEL).await.unwrap_or_default(),
        api_key: get(db, AI_API_KEY).await,
    }
}

/// Whether the AI feature is usable — the ONLY AI signal a non-admin may read
/// (provider/base URL/model can name internal hosts, so they stay admin-only).
pub(crate) async fn ai_is_usable(db: &SqlitePool) -> bool {
    ai_config(db).await.is_usable()
}

// --- admin API ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiView {
    enabled: bool,
    provider: String,
    base_url: String,
    model: String,
    /// The key itself is never sent — only whether one is stored, and its tail.
    api_key_set: bool,
    api_key_last4: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    ai: AiView,
    root_password_is_default: bool,
}

fn last4(key: &str) -> Option<String> {
    let chars: Vec<char> = key.chars().collect();
    (chars.len() >= 4).then(|| chars[chars.len() - 4..].iter().collect())
}

pub async fn get_settings(State(state): State<AppState>, _admin: AdminUser) -> Response {
    let ai = ai_config(&state.db).await;
    let key = ai.api_key.unwrap_or_default();
    Json(SettingsView {
        ai: AiView {
            enabled: ai.enabled,
            provider: ai.provider,
            base_url: ai.base_url,
            model: ai.model,
            api_key_set: !key.is_empty(),
            api_key_last4: last4(&key),
        },
        root_password_is_default: flag(&state.db, ROOT_PW_DEFAULT).await,
    })
    .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    enabled: bool,
    provider: String,
    base_url: String,
    model: String,
    /// Absent/empty keeps the stored key; a value replaces it.
    #[serde(default)]
    api_key: Option<String>,
    /// Explicit removal — "empty keeps existing" alone gives no way to clear.
    #[serde(default)]
    remove_api_key: bool,
}

/// http(s) URL with a host, e.g. `https://api.anthropic.com` or
/// `http://localhost:11434`. A LAN/loopback host is intentional (local models).
fn valid_base_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https") && u.host_str().is_some_and(|h| !h.is_empty())
    })
}

pub async fn put_settings(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<UpdateSettings>,
) -> Response {
    let provider = body.provider.trim().to_owned();
    if provider != PROVIDER_ANTHROPIC && provider != PROVIDER_OPENAI {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "provider must be 'anthropic' or 'openai'",
        );
    }
    let base_url = body.base_url.trim().trim_end_matches('/').to_owned();
    let model = body.model.trim().to_owned();
    if !base_url.is_empty() && !valid_base_url(&base_url) {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "base URL must be an http(s) URL",
        );
    }
    if base_url.len() > 500 || model.len() > 200 {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "value too long");
    }

    // Resolve the key that WILL be stored, so "enabled" can be validated
    // against the real end state.
    let new_key = body.api_key.map(|k| k.trim().to_owned());
    let stored_key = get(&state.db, AI_API_KEY).await;
    let effective_key = if body.remove_api_key {
        None
    } else {
        match new_key {
            Some(k) if !k.is_empty() => Some(k),
            _ => stored_key,
        }
    };
    if effective_key.as_deref().is_some_and(|k| k.len() > 500) {
        return json_error(StatusCode::UNPROCESSABLE_ENTITY, "API key too long");
    }
    if body.enabled
        && (model.is_empty()
            || base_url.is_empty()
            || effective_key.as_deref().is_none_or(str::is_empty))
    {
        return json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "base URL, model and an API key are required to enable AI",
        );
    }

    // One transaction: a mid-sequence failure must never leave AI *enabled*
    // pointing at a half-written provider/key.
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    let wrote = async {
        set_tx(&mut tx, AI_PROVIDER, &provider).await?;
        set_tx(&mut tx, AI_BASE_URL, &base_url).await?;
        set_tx(&mut tx, AI_MODEL, &model).await?;
        match &effective_key {
            Some(k) => set_tx(&mut tx, AI_API_KEY, k).await?,
            None => remove_tx(&mut tx, AI_API_KEY).await?,
        }
        // Flip the switch LAST, so a partial write can't leave it on.
        set_tx(
            &mut tx,
            AI_ENABLED,
            if body.enabled { "true" } else { "false" },
        )
        .await?;
        Ok::<(), sqlx::Error>(())
    }
    .await;
    if wrote.is_err() || tx.commit().await.is_err() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error");
    }
    get_settings(State(state), AdminUser(String::new())).await
}

#[cfg(test)]
mod tests {
    use super::{last4, valid_base_url};

    #[test]
    fn last4_masks_short_and_long_keys() {
        assert_eq!(last4("sk-abcdefgh").as_deref(), Some("efgh"));
        assert_eq!(last4("abcd").as_deref(), Some("abcd"));
        assert_eq!(last4("abc"), None); // too short to reveal anything
    }

    #[test]
    fn base_url_accepts_http_and_loopback() {
        assert!(valid_base_url("https://api.anthropic.com"));
        // Local models are a supported, intentional configuration.
        assert!(valid_base_url("http://localhost:11434"));
        assert!(!valid_base_url("ftp://example.com"));
        assert!(!valid_base_url("not a url"));
        assert!(!valid_base_url(""));
    }
}
