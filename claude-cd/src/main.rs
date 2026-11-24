mod cache;
mod config;
mod error;
mod github;
mod models;
mod routes;
use crate::{
    cache::{Cache, language_stats},
    config::Config,
    github::GithubClient,
    models::Snapshot,
    routes::{AppState, health, index, languages, profile, refresh, repos},
};
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;
use tokio::time::{Duration, interval};
use tower_http::{compression::CompressionLayer, services::ServeDir, trace::TraceLayer};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let config = Arc::new(Config::from_env()?);
    let github = GithubClient::new(
        config.github_api_base.clone(),
        config.github_username.clone(),
        config.github_token.clone(),
    )?;
    let cache = Cache::load(config.cache_dir.join("snapshot.json")).await?;
    let state = Arc::new(AppState {
        config: config.clone(),
        github,
        cache,
    });
    let refresh_state = state.clone();
    tokio::spawn(async move {
        let mut timer = interval(Duration::from_secs(
            refresh_state.config.refresh_interval_secs,
        ));
        loop {
            timer.tick().await;
            if let Err(err) = refresh_data(&refresh_state).await {
                error!(%err, "background refresh failed");
            }
        }
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/api/profile", get(profile))
        .route("/api/repos", get(repos))
        .route("/api/languages", get(languages))
        .route("/api/refresh", post(refresh))
        .route("/health", get(health))
        .nest_service("/static", ServeDir::new("static"))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(config.addr()).await?;
    info!(address = %config.addr(), "portfolio listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}
async fn refresh_data(state: &AppState) -> Result<(), crate::error::AppError> {
    let mut snapshot: Snapshot = state.cache.get().await;
    snapshot.profile = Some(state.github.profile().await?);
    snapshot.repos = state.github.repos().await?;
    for repo in &mut snapshot.repos {
        if let Ok(languages) = state.github.languages(&repo.full_name).await {
            repo.languages = languages;
        }
    }
    snapshot.languages = language_stats(&snapshot.repos);
    state.cache.replace(snapshot).await
}
async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
