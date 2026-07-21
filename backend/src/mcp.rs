//! MCP server over HTTP (BRIEF-0011).
//!
//! JSON-RPC 2.0 at `POST /mcp`, authenticated by a per-user API token
//! (`Authorization: Bearer dko_…`). Every tool is scoped to the TOKEN'S user —
//! unlike `shares.rs`, which resolves a deck's real owner, MCP must never do
//! that or it would break owner scoping.
//!
//! This is INBOUND only: it adds no outbound call, so CLAUDE.md §2 is unchanged.

use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::decks;
use crate::tokens::McpUser;
use crate::AppState;

/// Default when the client asks for something we don't know.
const PROTOCOL_VERSION: &str = "2024-11-05";
const SUPPORTED_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];

/// Write budget per user, as a fixed window. Generous next to any real agent's
/// cadence, but it stops a runaway loop from filling the volume with decks.
const WRITE_WINDOW: Duration = Duration::from_secs(60);
const WRITES_PER_WINDOW: u32 = 60;
const RATE_LIMITED: &str = "too many writes — slow down and retry in a minute";

pub async fn handle(
    State(state): State<AppState>,
    McpUser(user_id): McpUser,
    body: String,
) -> Response {
    let Ok(message) = serde_json::from_str::<Value>(&body) else {
        return Json(error_object(Value::Null, -32700, "parse error")).into_response();
    };

    match message {
        // Batch: reply only for the non-notification members; an all-
        // notification batch gets no body at all.
        Value::Array(items) => {
            let mut replies = Vec::new();
            for item in items {
                if let Some(reply) = dispatch(&state, &user_id, item).await {
                    replies.push(reply);
                }
            }
            if replies.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                Json(Value::Array(replies)).into_response()
            }
        }
        single => match dispatch(&state, &user_id, single).await {
            Some(reply) => Json(reply).into_response(),
            // A notification (no `id`) must never get a JSON-RPC reply.
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

fn error_object(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Returns None for notifications (a message with no `id`).
async fn dispatch(state: &AppState, user_id: &str, message: Value) -> Option<Value> {
    // `id` may be a string, number or null; absent means notification.
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

    let outcome = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => tools_call(state, user_id, &params).await,
        _ => Err((-32601i64, format!("method not found: {method}"))),
    };

    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_object(id, code, &message),
    })
}

fn initialize(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = if SUPPORTED_VERSIONS.contains(&requested) {
        requested
    } else {
        PROTOCOL_VERSION
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "deckoala", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "list_decks",
                "description": "List your Deckoala decks (id, title, last updated).",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            },
            {
                "name": "get_deck",
                "description": "Get one deck's Marp Markdown by id.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "deckId": { "type": "string" } },
                    "required": ["deckId"],
                    "additionalProperties": false,
                },
            },
            {
                "name": "create_deck",
                "description": "Create a deck from Marp Markdown. Begin with a YAML frontmatter block delimited by --- containing `marp: true`, `theme: deckoala` and `paginate: true`; after it, separate slides with a line containing only ---. If the frontmatter is missing it is added for you.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "markdown": { "type": "string" },
                    },
                    "additionalProperties": false,
                },
            },
            {
                "name": "update_deck",
                "description": "Replace a deck's title and/or Markdown. Call get_deck first and PRESERVE its YAML frontmatter — it carries the theme and any font overrides the owner set. The previous content is snapshotted as a revision first.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "deckId": { "type": "string" },
                        "title": { "type": "string" },
                        "markdown": { "type": "string" },
                        "baseUpdatedAt": {
                            "type": "string",
                            "description": "The updatedAt you based this edit on; forces a revision snapshot when stale.",
                        },
                    },
                    "required": ["deckId"],
                    "additionalProperties": false,
                },
            },
        ]
    })
}

async fn tools_call(
    state: &AppState,
    user_id: &str,
    params: &Value,
) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602i64, "missing tool name".to_owned()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let outcome = match name {
        "list_decks" => list_decks(state, user_id).await,
        "get_deck" => get_deck(state, user_id, &args).await,
        "create_deck" => create_deck(state, user_id, &args).await,
        "update_deck" => update_deck(state, user_id, &args).await,
        other => return Err((-32602, format!("unknown tool: {other}"))),
    };

    // Tool-level failures travel as MCP `isError` content, not JSON-RPC errors.
    Ok(match outcome {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(message) => {
            json!({ "content": [{ "type": "text", "text": message }], "isError": true })
        }
    })
}

/// Fixed-window write budget. Returns false once the window is spent; the
/// caller turns that into a tool-level error rather than a protocol error.
async fn allow_write(state: &AppState, user_id: &str) -> bool {
    let mut seen = state.mcp_writes.lock().await;
    let now = Instant::now();
    // Bound the map so a large user base can't grow it without limit.
    if seen.len() > 10_000 {
        seen.retain(|_, (started, _)| now.duration_since(*started) < WRITE_WINDOW);
    }
    let entry = seen.entry(user_id.to_owned()).or_insert((now, 0));
    if now.duration_since(entry.0) >= WRITE_WINDOW {
        *entry = (now, 0);
    }
    if entry.1 >= WRITES_PER_WINDOW {
        return false;
    }
    entry.1 += 1;
    true
}

/// Marp front matter every Deckoala deck needs. Missing it, marp-core falls
/// back to its stock theme — no brand styling, no pagination, and (worst) no
/// Noto Sans Thai fallback, so Thai decks export with substituted glyphs.
const FRONT_MATTER: &str = "---\nmarp: true\ntheme: deckoala\npaginate: true\n---\n\n";

/// An external model writes this Markdown, so we cannot assume it kept the
/// directives. `ai.rs` handles the same problem with a system prompt; here the
/// tool descriptions ask for front matter and this backfills when they ignore
/// it. Markdown that already opens with a `---` delimiter is left alone.
fn ensure_front_matter(markdown: Option<String>) -> Option<String> {
    markdown.map(|md| {
        if md.trim_start().starts_with("---") {
            md
        } else {
            format!("{FRONT_MATTER}{md}")
        }
    })
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|value| value.to_owned())
}

async fn list_decks(state: &AppState, user_id: &str) -> Result<String, String> {
    let rows = decks::list_decks_data(state, user_id)
        .await
        .map_err(|e| e.message().to_owned())?;
    let listed: Vec<Value> = rows
        .into_iter()
        .map(|row| json!({ "id": row.id, "title": row.title, "updatedAt": row.updated_at }))
        .collect();
    Ok(serde_json::to_string_pretty(&json!({ "decks": listed })).unwrap_or_default())
}

async fn get_deck(state: &AppState, user_id: &str, args: &Value) -> Result<String, String> {
    let deck_id = arg_str(args, "deckId").ok_or("deckId is required")?;
    let row = decks::fetch_deck_data(state, &deck_id, user_id)
        .await
        .map_err(|e| e.message().to_owned())?;
    Ok(serde_json::to_string_pretty(&json!({
        "id": row.id,
        "title": row.title,
        "updatedAt": row.updated_at,
        "markdown": row.markdown,
    }))
    .unwrap_or_default())
}

async fn create_deck(state: &AppState, user_id: &str, args: &Value) -> Result<String, String> {
    if !allow_write(state, user_id).await {
        return Err(RATE_LIMITED.to_owned());
    }
    let row = decks::create_deck_data(
        state,
        user_id,
        arg_str(args, "title"),
        ensure_front_matter(arg_str(args, "markdown")),
    )
    .await
    .map_err(|e| e.message().to_owned())?;
    Ok(serde_json::to_string_pretty(&json!({
        "id": row.id,
        "title": row.title,
        "updatedAt": row.updated_at,
    }))
    .unwrap_or_default())
}

async fn update_deck(state: &AppState, user_id: &str, args: &Value) -> Result<String, String> {
    let deck_id = arg_str(args, "deckId").ok_or("deckId is required")?;
    if !allow_write(state, user_id).await {
        return Err(RATE_LIMITED.to_owned());
    }
    let row = decks::update_deck_data(
        state,
        &deck_id,
        user_id,
        arg_str(args, "title"),
        ensure_front_matter(arg_str(args, "markdown")),
        arg_str(args, "baseUpdatedAt").as_deref(),
    )
    .await
    .map_err(|e| e.message().to_owned())?;
    Ok(serde_json::to_string_pretty(&json!({
        "id": row.id,
        "title": row.title,
        "updatedAt": row.updated_at,
    }))
    .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{ensure_front_matter, initialize, tools_list};
    use serde_json::json;

    #[test]
    fn front_matter_is_backfilled_but_never_duplicated() {
        let bare = ensure_front_matter(Some("# Slide one\n".to_owned())).unwrap();
        assert!(bare.starts_with("---\nmarp: true\ntheme: deckoala\n"));
        assert!(bare.ends_with("# Slide one\n"));
        // Already has frontmatter (or a leading delimiter): left untouched.
        let kept = "---\nmarp: true\ntheme: deckoala\nstyle: |\n  section { font-family: 'Sarabun'; }\n---\n\n# Thai\n";
        assert_eq!(
            ensure_front_matter(Some(kept.to_owned())).unwrap(),
            kept,
            "an owner's custom frontmatter must survive verbatim"
        );
        // Absent markdown stays absent so create_deck_data still templates it.
        assert!(ensure_front_matter(None).is_none());
    }

    #[test]
    fn initialize_echoes_a_known_version_and_falls_back_otherwise() {
        let echoed = initialize(&json!({ "protocolVersion": "2025-06-18" }));
        assert_eq!(echoed["protocolVersion"], "2025-06-18");
        let fallback = initialize(&json!({ "protocolVersion": "1999-01-01" }));
        assert_eq!(fallback["protocolVersion"], "2024-11-05");
        assert_eq!(fallback["serverInfo"]["name"], "deckoala");
        assert!(fallback["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_exposes_the_four_deck_tools_and_no_delete() {
        let listed = tools_list();
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["list_decks", "get_deck", "create_deck", "update_deck"]
        );
        // Deleting decks over a long-lived token is deliberately not offered.
        assert!(!names.iter().any(|name| name.contains("delete")));
    }
}
