use std::path::{Path, PathBuf};

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
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
        }
    }
}

/// Create the data directory if needed, open (or create) the SQLite database
/// in WAL mode and run all pending migrations.
pub async fn init_db(data_dir: &Path) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let options = SqliteConnectOptions::new()
        .filename(data_dir.join("deckoala.db"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
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

async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "not found" })),
    )
}

/// Build the full application router: `/api/*` JSON routes plus the static SPA
/// with an `index.html` fallback. The fallback never shadows the reserved
/// backend prefixes (`/api/`; `/assets/` and `/fonts/` arrive in later briefs).
pub fn app(state: AppState, static_dir: &Path) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .fallback(api_not_found)
        .with_state(state);
    let spa = ServeDir::new(static_dir).fallback(ServeFile::new(static_dir.join("index.html")));
    Router::new().nest("/api", api).fallback_service(spa)
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
