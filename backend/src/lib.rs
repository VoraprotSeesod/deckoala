pub mod auth;
pub mod decks;

use std::path::{Path, PathBuf};

use axum::extract::DefaultBodyLimit;
use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions::{cookie::SameSite, ExpiredDeletion, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub allow_signup: bool,
    /// Authority of DECKOALA_PUBLIC_URL, accepted as an additional origin
    /// (covers reverse proxies that rewrite the Host header).
    pub allowed_origin: Option<String>,
    pub secure_cookie: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
    pub allow_signup: bool,
    pub allowed_origin: Option<String>,
    pub secure_cookie: bool,
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | ""
            )
        })
        .unwrap_or(default)
}

/// Authority (host[:port]) of an http(s) URL, e.g. "https://a.example/x" → "a.example".
pub fn url_authority(url: &str) -> Option<String> {
    url.strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .map(|rest| {
            rest.split(['/', '?', '#'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
        .filter(|authority| !authority.is_empty())
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind: std::env::var("DECKOALA_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            data_dir: std::env::var("DECKOALA_DATA_DIR")
                .unwrap_or_else(|_| "/data".into())
                .into(),
            static_dir: std::env::var("DECKOALA_STATIC_DIR")
                .unwrap_or_else(|_| "./static".into())
                .into(),
            allow_signup: env_flag("DECKOALA_ALLOW_SIGNUP", true),
            allowed_origin: std::env::var("DECKOALA_PUBLIC_URL")
                .ok()
                .and_then(|url| url_authority(&url)),
            secure_cookie: env_flag("DECKOALA_SECURE_COOKIE", false),
        }
    }
}

/// Current UTC time as an RFC3339 string (project-wide timestamp format).
/// Fixed-width (always 6 subsecond digits) so TEXT columns holding these
/// values sort lexicographically in true chronological order — `time`'s
/// well-known RFC3339 trims trailing zeros and would misorder rows.
pub fn now_rfc3339() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.microsecond()
    )
}

/// Create the data directory if needed, open (or create) the SQLite database
/// in WAL mode and run all pending migrations.
pub async fn init_db(data_dir: &Path) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let options = SqliteConnectOptions::new()
        .filename(data_dir.join("deckoala.db"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Standard JSON error body used across the API.
pub fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
    db: &'static str,
    chromium: bool,
}

/// True iff CHROME_BIN is set and points at an existing regular file.
fn chromium_available() -> bool {
    std::env::var("CHROME_BIN")
        .map(|p| Path::new(&p).is_file())
        .unwrap_or(false)
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let db_ok =
        sqlx::query_scalar::<_, String>("SELECT value FROM meta WHERE key = 'schema_seeded'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(|v| v == "1")
            .unwrap_or(false);
    Json(Health {
        status: if db_ok { "ok" } else { "degraded" },
        version: env!("CARGO_PKG_VERSION"),
        db: if db_ok { "ok" } else { "error" },
        chromium: chromium_available(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Instance {
    allow_signup: bool,
    has_users: bool,
}

async fn instance(State(state): State<AppState>) -> Response {
    let user_count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
    {
        Ok(n) => n,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    };
    Json(Instance {
        allow_signup: state.allow_signup,
        has_users: user_count > 0,
    })
    .into_response()
}

async fn api_not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not found")
}

/// CSRF defense: mutating requests carrying a foreign `Origin` are rejected.
/// Requests without an Origin header (curl, native clients) pass; SameSite=Lax
/// cookies cover the remaining browser cases. See BRIEF-0001 / ARCHITECTURE §4.
async fn same_origin_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method();
    if !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        if let Some(origin) = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
        {
            let host = req
                .headers()
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !origin_allowed(origin, host, state.allowed_origin.as_deref()) {
                return json_error(StatusCode::FORBIDDEN, "cross-origin request rejected");
            }
        }
    }
    next.run(req).await
}

/// The Origin's authority (no port normalization — browsers omit default
/// ports, so `https://example.com` matches bare Host `example.com`) must
/// equal the raw Host header or the configured public-URL authority.
/// `Origin: null` and unparsable values are foreign.
fn origin_allowed(origin: &str, host: &str, allowed: Option<&str>) -> bool {
    match origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    {
        Some(authority) => {
            let matches_host = !host.is_empty() && authority.eq_ignore_ascii_case(host);
            let matches_allowed =
                allowed.is_some_and(|expected| authority.eq_ignore_ascii_case(expected));
            matches_host || matches_allowed
        }
        None => false,
    }
}

/// Build the full application router: `/api/*` JSON routes (session-enabled,
/// origin-guarded) plus the static SPA with an `index.html` fallback. The
/// fallback never shadows the reserved backend prefixes (`/api/`; `/assets/`
/// and `/fonts/` arrive in later briefs).
pub async fn app(state: AppState, static_dir: &Path) -> Result<Router, Box<dyn std::error::Error>> {
    let session_store = SqliteStore::new(state.db.clone());
    session_store.migrate().await?;
    // Without periodic cleanup the tower_sessions table grows forever on a
    // long-lived instance (rows only vanish on explicit logout). Dropping the
    // handle detaches the task; it runs for the process lifetime.
    let _cleanup_task = tokio::spawn(
        session_store
            .clone()
            .continuously_delete_expired(std::time::Duration::from_secs(3600)),
    );
    let session_layer = SessionManagerLayer::new(session_store)
        // Secure off by default: TLS usually terminates at the operator's
        // reverse proxy. Opt in with DECKOALA_SECURE_COOKIE=true.
        .with_secure(state.secure_cookie)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(30)));

    let api = Router::new()
        .route("/health", get(health))
        .route("/instance", get(instance))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/decks", get(decks::list).post(decks::create))
        .route(
            "/decks/{id}",
            get(decks::get_one)
                .patch(decks::update)
                .delete(decks::remove),
        )
        .route("/decks/{id}/duplicate", post(decks::duplicate))
        .route("/decks/{id}/export", get(decks::export))
        .fallback(api_not_found)
        // JSON escaping can double a 1 MB markdown payload; the app-level
        // 1 MB cap stays authoritative (BRIEF-0002).
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            same_origin_guard,
        ))
        .layer(session_layer)
        .with_state(state);

    let spa = ServeDir::new(static_dir).fallback(ServeFile::new(static_dir.join("index.html")));
    Ok(Router::new().nest("/api", api).fallback_service(spa))
}

/// Exit code for the Docker HEALTHCHECK: GET /api/health on the local port,
/// 0 on HTTP 200, 1 otherwise. Std-only so it adds nothing to the binary.
pub fn healthcheck() -> i32 {
    use std::io::{Read, Write};
    let bind = std::env::var("DECKOALA_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let addr = format!("127.0.0.1:{port}");
    match std::net::TcpStream::connect(&addr) {
        Ok(mut stream) => {
            let request =
                format!("GET /api/health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
            if stream.write_all(request.as_bytes()).is_err() {
                return 1;
            }
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            if response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200") {
                0
            } else {
                1
            }
        }
        Err(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{now_rfc3339, origin_allowed, url_authority};

    #[test]
    fn timestamps_are_fixed_width_and_sortable() {
        let ts = now_rfc3339();
        assert_eq!(ts.len(), 27, "YYYY-MM-DDTHH:MM:SS.ffffffZ = 27 chars: {ts}");
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.chars().filter(|c| *c == '.').count(), 1);
    }

    #[test]
    fn origin_matches_raw_host() {
        assert!(origin_allowed(
            "http://localhost:8321",
            "localhost:8321",
            None
        ));
        assert!(origin_allowed(
            "http://LOCALHOST:8321",
            "localhost:8321",
            None
        ));
    }

    #[test]
    fn https_default_port_matches_bare_host() {
        assert!(origin_allowed(
            "https://deckoala.dimenshade.com",
            "deckoala.dimenshade.com",
            None
        ));
    }

    #[test]
    fn null_and_foreign_origins_rejected() {
        assert!(!origin_allowed("null", "localhost:8321", None));
        assert!(!origin_allowed(
            "http://evil.example",
            "localhost:8321",
            None
        ));
        assert!(!origin_allowed("garbage", "localhost:8321", None));
        assert!(!origin_allowed("http://x", "", None));
    }

    #[test]
    fn public_url_authority_accepted_when_host_rewritten() {
        // Reverse proxy rewrote Host to the upstream address.
        assert!(origin_allowed(
            "https://deckoala.dimenshade.com",
            "127.0.0.1:8080",
            Some("deckoala.dimenshade.com")
        ));
        assert!(!origin_allowed(
            "https://evil.example",
            "127.0.0.1:8080",
            Some("deckoala.dimenshade.com")
        ));
    }

    #[test]
    fn url_authority_extraction() {
        assert_eq!(
            url_authority("https://deckoala.dimenshade.com/some/path"),
            Some("deckoala.dimenshade.com".to_owned())
        );
        assert_eq!(
            url_authority("http://localhost:8321"),
            Some("localhost:8321".to_owned())
        );
        assert_eq!(url_authority("ftp://nope"), None);
        assert_eq!(url_authority("https://"), None);
    }
}
