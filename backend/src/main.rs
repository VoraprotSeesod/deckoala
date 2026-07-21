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
    // Bootstrap admin (BRIEF-0010). Seeded here — never inside init_db — so the
    // integration suite's "first signup is the admin" behaviour is untouched.
    const DEFAULT_ROOT_PASSWORD: &str = "Admin123456@";
    // compose.yml always sets the var (possibly empty) — treat blank as unset.
    let root_password = std::env::var("DECKOALA_ROOT_PASSWORD")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ROOT_PASSWORD.into());
    let is_builtin_default = root_password == DEFAULT_ROOT_PASSWORD;
    match deckoala_server::auth::seed_root(&db, &root_password, is_builtin_default).await {
        Ok(true) if is_builtin_default => tracing::warn!(
            "seeded bootstrap admin 'root' with the BUILT-IN DEFAULT password — \
             change it from Admin settings, or set DECKOALA_ROOT_PASSWORD before first start"
        ),
        Ok(true) => tracing::info!("seeded bootstrap admin 'root' from DECKOALA_ROOT_PASSWORD"),
        Ok(false) => {}
        Err(e) => tracing::error!("could not seed the bootstrap admin: {e}"),
    }

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
        share_export_sem: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
        ai_sem: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        ai_last_call: std::sync::Arc::new(tokio::sync::Mutex::new(Default::default())),
        mcp_writes: std::sync::Arc::new(tokio::sync::Mutex::new(Default::default())),
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
