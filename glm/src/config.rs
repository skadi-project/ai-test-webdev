use std::{path::PathBuf, time::Duration};

/// Runtime configuration, fully driven by environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    pub github_username: String,
    pub github_token: Option<String>,
    pub refresh_interval: Duration,
    pub port: u16,
    pub admin_token: Option<String>,
    pub pinned: Vec<String>,
    pub excluded: Vec<String>,
    pub show_forks: bool,
    pub show_archived: bool,
    pub cache_dir: PathBuf,
}

impl Config {
    /// Load configuration from the process environment.
    ///
    /// # Errors
    /// Returns an error if a required variable (`GITHUB_USERNAME`) is missing
    /// or an optional numeric variable cannot be parsed.
    pub fn from_env() -> anyhow::Result<Self> {
        let github_username = std::env::var("GITHUB_USERNAME")
            .map_err(|_| anyhow::anyhow!("GITHUB_USERNAME is required"))?;
        if github_username.trim().is_empty() {
            anyhow::bail!("GITHUB_USERNAME must not be empty");
        }

        let refresh_interval =
            Duration::from_secs(env_parse("REFRESH_INTERVAL_SECS", 300, |v: u64| v.max(15))?);
        let port = env_parse("PORT", 8080, |v: u16| v)?;
        let cache_dir = std::env::var("CACHE_DIR").unwrap_or_else(|_| "./data".into());

        Ok(Self {
            github_username,
            github_token: std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()),
            refresh_interval,
            port,
            admin_token: std::env::var("ADMIN_TOKEN").ok().filter(|t| !t.is_empty()),
            pinned: env_list("PINNED_REPOS"),
            excluded: env_list("EXCLUDED_REPOS"),
            show_forks: env_bool("SHOW_FORKS", false),
            show_archived: env_bool("SHOW_ARCHIVED", false),
            cache_dir: PathBuf::from(cache_dir),
        })
    }

    /// Path of the on-disk cache snapshot.
    pub fn cache_file(&self) -> PathBuf {
        self.cache_dir.join("cache.json")
    }
}

fn env_parse<T>(key: &str, default: T, normalize: impl FnOnce(T) -> T) -> anyhow::Result<T>
where
    T: std::str::FromStr + Copy,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(raw) if !raw.is_empty() => raw
            .parse::<T>()
            .map(normalize)
            .map_err(|e| anyhow::anyhow!("invalid {key}: {e}")),
        _ => Ok(default),
    }
}

fn env_list(key: &str) -> Vec<String> {
    std::env::var(key)
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

#[cfg(test)]
impl Config {
    /// Test configuration with no env dependency.
    pub fn test(pinned: &[&str], excluded: &[&str], show_forks: bool, show_archived: bool) -> Self {
        Self {
            github_username: "octocat".into(),
            github_token: None,
            refresh_interval: Duration::from_secs(300),
            port: 8080,
            admin_token: None,
            pinned: pinned.iter().map(|s| (*s).to_owned()).collect(),
            excluded: excluded.iter().map(|s| (*s).to_owned()).collect(),
            show_forks,
            show_archived,
            cache_dir: std::env::temp_dir(),
        }
    }
}
