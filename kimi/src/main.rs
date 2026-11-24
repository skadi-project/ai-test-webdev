use std::{net::SocketAddr, sync::Arc};

use github_portfolio::{
    build_state, config::Config, error::AppError, refresh::refresh_loop, routes,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .with_target(false)
        .compact()
        .init();

    let config = Config::from_env()?;
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let state = Arc::new(build_state(config)?);
    if let Err(error) = state.cache.restore().await {
        warn!(error = %error, "could not restore disk cache; starting with an empty cache");
    }

    let shutdown = CancellationToken::new();
    let background = tokio::spawn(refresh_loop(Arc::clone(&state), shutdown.clone()));
    let listener = TcpListener::bind(address).await?;
    info!(%address, username = %state.config.github_username, "portfolio server started");

    let server_result = axum::serve(listener, routes::router(state)?)
        .with_graceful_shutdown(shutdown_signal(shutdown.clone()))
        .await;
    shutdown.cancel();
    if let Err(error) = background.await {
        warn!(error = %error, "background refresh task did not finish cleanly");
    }
    server_result?;
    info!("portfolio server stopped");
    Ok(())
}

async fn shutdown_signal(shutdown: CancellationToken) {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            warn!(error = %error, "failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => warn!(error = %error, "failed to install SIGTERM handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown signal received");
    shutdown.cancel();
}
