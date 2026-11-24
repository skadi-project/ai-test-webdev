//! Binary entry point: logging, configuration, cache warm-up, background
//! refresher and a graceful shutdown on `SIGINT`/`SIGTERM`.

use std::process::ExitCode;

use anyhow::{Context, Result};
use github_portfolio::cache::Cache;
use github_portfolio::config::Config;
use github_portfolio::routes;
use github_portfolio::state::AppState;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> ExitCode {
    // `.env` is convenient locally; real deployments inject the variables.
    let _ = dotenvy::dotenv();
    init_tracing();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = format!("{err:#}"), "fatal error");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let config = Config::from_env().context("invalid configuration")?;
    tracing::info!(
        user = %config.github_username,
        port = config.port,
        refresh_interval_secs = config.refresh_interval.as_secs(),
        authenticated = config.github_token.is_some(),
        cache_file = ?config.cache_file,
        "starting github-portfolio"
    );

    let cache = Cache::new(config.cache_file.clone()).await;
    let cache = match cache.snapshot() {
        Some(snapshot)
            if !snapshot
                .profile
                .login
                .eq_ignore_ascii_case(&config.github_username) =>
        {
            tracing::warn!("disk cache belongs to another profile; ignoring");
            Cache::empty(config.cache_file.clone())
        }
        Some(snapshot) => {
            let mut restored = (*snapshot).clone();
            restored.repos =
                github_portfolio::refresh::normalize_repos(restored.source_repos.clone(), &config);
            cache.store_memory(restored);
            cache
        }
        None => cache,
    };
    let state = AppState::new(config)
        .context("failed to build the GitHub client")?
        .with_cache(cache);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let refresher = tokio::spawn(refresher_loop(state.clone(), shutdown_rx.clone()));

    let app = routes::router(state.clone());
    let addr = format!("0.0.0.0:{}", state.config.port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("cannot bind {addr}"))?;
    tracing::info!(%addr, "http server listening");

    let server =
        axum::serve(listener, app).with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()));

    if let Err(err) = server.await {
        tracing::error!(error = %err, "http server stopped with an error");
    }

    // Tell the refresher to stop, then wait for it so the final snapshot is on disk.
    let _ = shutdown_tx.send(true);
    if let Err(err) = refresher.await {
        tracing::warn!(error = %err, "refresher task join error");
    }
    tracing::info!("shutdown complete");
    Ok(())
}

/// Periodic refresh loop. The first refresh happens immediately so the site is
/// useful right after a cold start.
async fn refresher_loop(state: AppState, mut shutdown: watch::Receiver<bool>) {
    let interval = state.config.refresh_interval;
    loop {
        let outcome = tokio::select! {
            outcome = state.refresh_now() => outcome,
            _ = shutdown.changed() => break,
        };
        tracing::info!(?outcome, "background refresh finished");
        if *shutdown.borrow() {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = state.refresh_requested.notified() => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

/// Resolves when the process receives `Ctrl+C` or `SIGTERM`.
async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "failed to listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl+C, draining connections"),
        _ = terminate => tracing::info!("received SIGTERM, draining connections"),
    }
    let _ = shutdown_tx.send(true);
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("github_portfolio=info,tower_http=info,warn"));

    let json_logs = std::env::var("LOG_FORMAT")
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    let registry = tracing_subscriber::registry().with(filter);
    if json_logs {
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(false),
            )
            .init();
    } else {
        registry.with(tracing_subscriber::fmt::layer()).init();
    }
}
