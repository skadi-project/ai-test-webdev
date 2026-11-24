use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{cache::PortfolioCache, config::Config, github::GitHubClient};

pub struct AppState {
    pub config: Arc<Config>,
    pub github: GitHubClient,
    pub cache: PortfolioCache,
    pub refresh_lock: Mutex<()>,
}

impl AppState {
    pub fn new(config: Arc<Config>, github: GitHubClient, cache: PortfolioCache) -> Self {
        Self {
            config,
            github,
            cache,
            refresh_lock: Mutex::new(()),
        }
    }
}
