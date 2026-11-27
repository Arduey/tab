//! 个人标签页：单二进制服务，SQLite 存储，无前端构建链。

mod api;
mod assets;
mod auth;
mod config;
mod db;
mod favicon;
mod icons;
mod pages;
mod templates;

pub use auth::now;

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // -V / --version、--health 用于部署探针，不需要启动服务
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("tab {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--health") {
        return health_check().await;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("tab=info,tower_http=warn")),
        )
        .init();

    let cfg = config::Config::from_env();
    cfg.ensure_dirs()?;
    let db = db::open(&cfg.db_path())?;

    let state = Arc::new(api::AppState {
        db,
        cfg: cfg.clone(),
        http: favicon::client(),
    });

    let app = api::router(state);

    let addr = format!("{}:{}", cfg.bind, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{addr} (data dir: {})", cfg.data_dir.display());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    tracing::info!("shutting down");
}
/// 容器健康检查：连一下本机端口，能连上就算健康
async fn health_check() -> anyhow::Result<()> {
    let cfg = config::Config::from_env();
    let host = if cfg.bind == "0.0.0.0" || cfg.bind == "::" {
        "127.0.0.1".to_string()
    } else {
        cfg.bind.clone()
    };
    let addr = format!("{host}:{}", cfg.port);
    match tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_)) => {
            println!("ok");
            Ok(())
        }
        _ => {
            eprintln!("unhealthy: cannot connect to {addr}");
            std::process::exit(1);
        }
    }
}
