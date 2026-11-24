use std::{env, net::SocketAddr, path::PathBuf};

use crate::error::AppError;

#[derive(Clone, Debug)]
pub struct Config {
    pub github_username: String,
    pub github_token: Option<String>,
    pub admin_token: Option<String>,
    pub github_api_base: String,
    pub refresh_interval_secs: u64,
    pub port: u16,
    pub cache_dir: PathBuf,
    pub pinned_repos: Vec<String>,
    pub excluded_repos: Vec<String>,
    pub show_forks: bool,
    pub show_archived: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let username = env::var("GITHUB_USERNAME")
            .map_err(|_| AppError::Config("GITHUB_USERNAME is required".into()))?;
        if username.trim().is_empty() {
            return Err(AppError::Config("GITHUB_USERNAME cannot be empty".into()));
        }
        Ok(Self {
            github_username: username,
            github_token: optional("GITHUB_TOKEN"),
            admin_token: optional("ADMIN_TOKEN"),
            github_api_base: env::var("GITHUB_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.github.com".into()),
            refresh_interval_secs: parse_u64("REFRESH_INTERVAL_SECS", 300),
            port: parse_u16("PORT", 8080),
            cache_dir: PathBuf::from(
                env::var("CACHE_DIR").unwrap_or_else(|_| "./data/cache".into()),
            ),
            pinned_repos: csv("PINNED_REPOS"),
            excluded_repos: csv("EXCLUDED_REPOS"),
            show_forks: parse_bool("SHOW_FORKS", false),
            show_archived: parse_bool("SHOW_ARCHIVED", false),
        })
    }
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.port))
    }
}
fn optional(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.trim().is_empty())
}
fn csv(key: &str) -> Vec<String> {
    env::var(key)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_lowercase)
        .collect()
}
fn parse_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
fn parse_u16(key: &str, default: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
fn parse_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
