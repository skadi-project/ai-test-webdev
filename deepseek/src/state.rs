//! Shared application state.
//!
//! `AppState` is cheap to clone (everything inside is `Arc`-backed) and holds the
//! GitHub client, the cache and the refresh coordination primitive.

use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::cache::Cache;
use crate::config::Config;
use crate::github::{GitHubClient, GitHubError};
use crate::models::Snapshot;
use crate::refresh;

/// Outcome of a refresh attempt, shared by the background loop and `POST /api/refresh`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// New data was fetched and stored.
    Updated,
    /// GitHub answered `304 Not Modified` (or nothing actually changed).
    NotModified,
    /// The attempt failed; the previous snapshot is still served.
    Failed,
    /// Another refresh was already running; this one was skipped.
    Busy,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub github: Arc<GitHubClient>,
    pub cache: Cache,
    /// Serializes refreshes so a manual trigger and the timer never overlap.
    pub refresh_gate: Arc<Mutex<()>>,
    pub refresh_requested: Arc<tokio::sync::Notify>,
    pub started_at: Instant,
}

impl AppState {
    pub fn new(config: Config) -> Result<Self, GitHubError> {
        let github = GitHubClient::new(&config)?;
        let cache = Cache::memory_only();
        Ok(Self {
            config: Arc::new(config),
            github: Arc::new(github),
            cache,
            refresh_gate: Arc::new(Mutex::new(())),
            refresh_requested: Arc::new(tokio::sync::Notify::new()),
            started_at: Instant::now(),
        })
    }

    pub fn with_cache(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
    }

    /// Forces a refresh, waiting for any in-flight refresh to finish first.
    pub async fn refresh_now(&self) -> RefreshOutcome {
        refresh::refresh(&self.config, &self.github, &self.cache, &self.refresh_gate).await
    }

    /// Snapshot for read endpoints.
    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.cache.snapshot()
    }
}
