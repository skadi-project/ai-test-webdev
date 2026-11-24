use std::{env, path::PathBuf, time::Duration};
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Config {
    pub username: String,
    pub token: String,
    pub admin_token: String,
    pub cache_file: PathBuf,
    pub interval: Duration,
    pub show_forks: bool,
    pub archived: bool,
    pub pinned: Vec<String>,
    pub excluded: Vec<String>,
    pub port: String,
}
fn flag(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
fn list(name: &str, alias: &str) -> Vec<String> {
    env::var(name)
        .or_else(|_| env::var(alias))
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
        .collect()
}
impl Config {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            username: env::var("GITHUB_USERNAME")
                .map_err(|_| "GITHUB_USERNAME is required".to_string())?,
            token: env::var("GITHUB_TOKEN").unwrap_or_default(),
            admin_token: env::var("ADMIN_TOKEN").unwrap_or_default(),
            cache_file: env::var("CACHE_FILE")
                .unwrap_or_else(|_| "data/cache.json".into())
                .into(),
            interval: Duration::from_secs(
                env::var("REFRESH_INTERVAL_SECS")
                    .unwrap_or_else(|_| "300".into())
                    .parse()
                    .map_err(|_| "invalid REFRESH_INTERVAL_SECS")?,
            ),
            show_forks: flag("SHOW_FORKS", false),
            archived: env::var("SHOW_ARCHIVED")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| flag("ARCHIVED", false)),
            pinned: list("PINNED_REPOS", "PINNED"),
            excluded: list("EXCLUDED_REPOS", "EXCLUDED"),
            port: env::var("PORT").unwrap_or_else(|_| "8080".into()),
        })
    }
}
