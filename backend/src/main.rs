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
    let mut print_secret = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut print_secret);
    let state = AppState {
        db,
        allow_signup: config.allow_signup,
        allowed_origin: config.allowed_origin.clone(),
        secure_cookie: config.secure_cookie,
        revision_min_secs: 300,
        data_dir: config.data_dir.clone(),
        print_secret,
        local_addr: deckoala_server::loopback_addr(&config.bind),
        export_sem: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
    };
    let router = app(state, &config.static_dir)
        .await
        .expect("failed to build application");

    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.bind));
    tracing::info!("Deckoala listening on {}", config.bind);
    axum::serve(listener, router).await.expect("server error");
}
