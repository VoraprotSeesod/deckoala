use deckoala_server::{app, healthcheck, init_db, AppState, Config};

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        std::process::exit(healthcheck());
    }

    tracing_subscriber::fmt().init();

    let config = Config::from_env();
    let db = init_db(&config.data_dir)
        .await
        .expect("failed to initialize database");
    let router = app(AppState { db }, &config.static_dir);

    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.bind));
    tracing::info!("Deckoala listening on {}", config.bind);
    axum::serve(listener, router).await.expect("server error");
}
