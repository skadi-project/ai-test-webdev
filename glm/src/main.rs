mod cache;
mod config;
mod error;
mod filter;
mod github;
mod models;
mod routes;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

use crate::cache::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Load `.env` when present so `cargo run` works like `docker compose`.
    // Real environment variables always win over the file.
    if let Err(e) = dotenvy::dotenv() {
        tracing::debug!(error = %e, "no .env file loaded, using the process environment");
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gh_portfolio=info,tower_http=info".into()),
        )
        .init();

    let config = config::Config::from_env()?;
    let addr = format!("0.0.0.0:{}", config.port);
    let state: AppState = std::sync::Arc::new(cache::AppInner::new(config)?);

    tracing::info!(
        username = %state.config.github_username,
        %addr,
        refresh_interval_secs = state.config.refresh_interval.as_secs(),
        "starting gh-portfolio"
    );

    tokio::spawn(cache::refresh_loop(state.clone()));

    let app = routes::router(state);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server run")?;
    Ok(())
}

/// Resolve on Ctrl-C (all platforms) and SIGTERM (unix), for containers.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = ctrl_c => tracing::info!("ctrl-c received, shutting down"),
                    _ = terminate.recv() => tracing::info!("SIGTERM received, shutting down"),
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "cannot install SIGTERM handler, waiting for ctrl-c");
                if ctrl_c.await.is_ok() {
                    tracing::info!("ctrl-c received, shutting down");
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        if ctrl_c.await.is_ok() {
            tracing::info!("ctrl-c received, shutting down");
        }
    }
}
