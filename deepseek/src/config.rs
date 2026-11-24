//! Environment-driven configuration.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::AppError;

/// Default refresh interval, in seconds, when `REFRESH_INTERVAL_SECS` is unset.
pub const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 300;
/// Default HTTP port when `PORT` is unset.
pub const DEFAULT_PORT: u16 = 8080;

/// Validated runtime configuration.
#[derive(Clone)]
pub struct Config {
    pub github_username: String,
    pub github_token: Option<String>,
    pub refresh_interval: Duration,
    pub port: u16,
    pub admin_token: Option<String>,
    pub pinned_repos: Vec<String>,
    pub excluded_repos: Vec<String>,
    pub show_forks: bool,
    pub show_archived: bool,
    /// Path of the on-disk JSON snapshot. Empty/`none` disables persistence.
    pub cache_file: Option<PathBuf>,
    pub github_api_base: String,
    /// Maximum number of repositories to enrich with `/languages` per refresh.
    pub language_budget: usize,
    /// Optional `Access-Control-Allow-Origin` value for cross-origin API reads.
    pub cors_allow_origin: Option<String>,
}

impl Config {
    /// Reads configuration from the environment (after `dotenvy` has run).
    pub fn from_env() -> Result<Self, AppError> {
        let github_username = env_required("GITHUB_USERNAME")?;
        if !is_valid_username(&github_username) {
            return Err(AppError::Config(
                "GITHUB_USERNAME may only contain letters, digits and single hyphens".into(),
            ));
        }
        Self::from_map(|key| std::env::var(key).ok(), github_username)
    }

    /// Builds a configuration from an arbitrary lookup function. Used by tests.
    pub fn from_map<F>(lookup: F, github_username: String) -> Result<Self, AppError>
    where
        F: Fn(&str) -> Option<String>,
    {
        if !is_valid_username(&github_username) {
            return Err(AppError::Config("invalid GitHub username".into()));
        }
        let github_token = lookup("GITHUB_TOKEN").filter(|v| !v.trim().is_empty());
        let refresh_secs = parse_or(
            &lookup,
            "REFRESH_INTERVAL_SECS",
            DEFAULT_REFRESH_INTERVAL_SECS,
        )?;
        if refresh_secs == 0 || refresh_secs > 86400 * 30 {
            return Err(AppError::Config(
                "REFRESH_INTERVAL_SECS must be 1..=2592000".into(),
            ));
        }
        if refresh_secs < 15 {
            tracing::warn!(
                interval = refresh_secs,
                "REFRESH_INTERVAL_SECS is very low; GitHub rate limits may be exhausted"
            );
        }
        let port = parse_or(&lookup, "PORT", DEFAULT_PORT)?;
        let admin_token = lookup("ADMIN_TOKEN")
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        if admin_token.is_none() {
            tracing::warn!("ADMIN_TOKEN is not set: POST /api/refresh is disabled");
        }
        let cache_file = match lookup("CACHE_FILE") {
            None => Some(PathBuf::from("data/cache.json")),
            Some(value) if value.trim().is_empty() || value.trim() == "none" => None,
            Some(value) => Some(PathBuf::from(value.trim())),
        };
        let language_budget = parse_or(&lookup, "LANGUAGE_BUDGET", 40usize)?;
        let github_api_base = lookup("GITHUB_API_BASE")
            .map(|v| v.trim().trim_end_matches('/').to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://api.github.com".to_owned());
        let api_url = url::Url::parse(&github_api_base)
            .map_err(|_| AppError::Config("invalid GITHUB_API_BASE URL".into()))?;
        if !matches!(api_url.scheme(), "http" | "https")
            || api_url.host_str().is_none()
            || !api_url.username().is_empty()
            || api_url.password().is_some()
            || api_url.query().is_some()
        {
            return Err(AppError::Config(
                "GITHUB_API_BASE must be an HTTP(S) URL without credentials or query".into(),
            ));
        }
        if language_budget == 0 {
            return Err(AppError::Config("LANGUAGE_BUDGET must be positive".into()));
        }
        let cors_allow_origin = lookup("CORS_ALLOW_ORIGIN")
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty() && v != "*");

        Ok(Self {
            github_username,
            github_token,
            refresh_interval: Duration::from_secs(refresh_secs),
            port,
            admin_token,
            pinned_repos: parse_list(lookup("PINNED_REPOS").as_deref()),
            excluded_repos: parse_list(lookup("EXCLUDED_REPOS").as_deref()),
            show_forks: parse_bool(lookup("SHOW_FORKS").as_deref(), false),
            show_archived: parse_bool(lookup("SHOW_ARCHIVED").as_deref(), false),
            cache_file,
            github_api_base,
            language_budget,
            cors_allow_origin,
        })
    }
}

fn env_required(key: &str) -> Result<String, AppError> {
    match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_owned()),
        _ => Err(AppError::Config(format!(
            "{key} is required but missing or empty"
        ))),
    }
}

fn parse_or<T, F>(lookup: &F, key: &str, default: T) -> Result<T, AppError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
    F: Fn(&str) -> Option<String>,
{
    match lookup(key)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
    {
        Some(raw) => raw
            .parse::<T>()
            .map_err(|err| AppError::Config(format!("invalid {key}={raw:?}: {err}"))),
        None => Ok(default),
    }
}

/// Parses `a, b ,c` into `["a", "b", "c"]`, dropping blanks and duplicates.
pub fn parse_list(raw: Option<&str>) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for part in raw.unwrap_or_default().split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() || items.iter().any(|i| i == trimmed) {
            continue;
        }
        items.push(trimmed.to_owned());
    }
    items
}

/// Parses common truthy spellings; anything else is false.
pub fn parse_bool(raw: Option<&str>, default: bool) -> bool {
    match raw.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        _ => default,
    }
}

fn is_valid_username(username: &str) -> bool {
    let len = username.len();
    if !(1..=39).contains(&len) {
        return false;
    }
    !username.starts_with('-')
        && !username.ends_with('-')
        && !username.contains("--")
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key: &str| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn parse_list_trims_and_dedupes() {
        assert_eq!(
            parse_list(Some(" a, b ,,a , c ")),
            vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
        );
        assert!(parse_list(None).is_empty());
        assert!(parse_list(Some("  , ")).is_empty());
    }

    #[test]
    fn parse_bool_defaults() {
        assert!(parse_bool(Some("TRUE"), false));
        assert!(parse_bool(Some("yes"), false));
        assert!(!parse_bool(Some("off"), true));
        assert!(parse_bool(None, true));
        assert!(!parse_bool(Some("maybe"), false));
    }

    #[test]
    fn config_uses_defaults() {
        let cfg = Config::from_map(lookup(&[]), "octocat".to_owned()).expect("config");
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(
            cfg.refresh_interval,
            Duration::from_secs(DEFAULT_REFRESH_INTERVAL_SECS)
        );
        assert!(!cfg.show_forks);
        assert!(!cfg.show_archived);
        assert_eq!(
            cfg.cache_file.as_deref(),
            Some(std::path::Path::new("data/cache.json"))
        );
        assert_eq!(cfg.github_api_base, "https://api.github.com");
    }

    #[test]
    fn config_reads_overrides() {
        let cfg = Config::from_map(
            lookup(&[
                ("PORT", "9090"),
                ("REFRESH_INTERVAL_SECS", "60"),
                ("ADMIN_TOKEN", " secret "),
                ("PINNED_REPOS", "alpha,beta"),
                ("EXCLUDED_REPOS", "gamma"),
                ("SHOW_FORKS", "true"),
                ("SHOW_ARCHIVED", "1"),
                ("CACHE_FILE", "none"),
                ("LANGUAGE_BUDGET", "7"),
            ]),
            "octocat".to_owned(),
        )
        .expect("config");
        assert_eq!(cfg.port, 9090);
        assert_eq!(cfg.refresh_interval, Duration::from_secs(60));
        assert_eq!(cfg.admin_token.as_deref(), Some("secret"));
        assert_eq!(
            cfg.pinned_repos,
            vec!["alpha".to_owned(), "beta".to_owned()]
        );
        assert_eq!(cfg.excluded_repos, vec!["gamma".to_owned()]);
        assert!(cfg.show_forks && cfg.show_archived);
        assert!(cfg.cache_file.is_none());
        assert_eq!(cfg.language_budget, 7);
    }

    #[test]
    fn invalid_port_is_rejected() {
        let result = Config::from_map(lookup(&[("PORT", "eighty")]), "octocat".to_owned());
        assert!(matches!(result, Err(AppError::Config(_))));
    }

    #[test]
    fn username_validation() {
        assert!(is_valid_username("octocat"));
        assert!(is_valid_username("a-b"));
        assert!(!is_valid_username("-a"));
        assert!(!is_valid_username("a--b"));
        assert!(!is_valid_username("a b"));
        assert!(!is_valid_username(""));
    }

    #[test]
    fn zero_interval_is_rejected() {
        assert!(
            Config::from_map(lookup(&[("REFRESH_INTERVAL_SECS", "0")]), "octocat".into()).is_err()
        );
    }
}
