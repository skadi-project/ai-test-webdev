pub mod cache;
pub mod config;
pub mod error;
pub mod github;
pub mod models;
pub mod refresh;
pub mod repository;
pub mod routes;
pub mod state;

use std::sync::Arc;

use cache::PortfolioCache;
use config::Config;
use github::GitHubClient;
use state::AppState;

pub fn build_state(config: Config) -> Result<AppState, crate::error::AppError> {
    let github = GitHubClient::new(&config)?;
    let cache = PortfolioCache::new(config.cache_path.clone());
    Ok(AppState::new(Arc::new(config), github, cache))
}
