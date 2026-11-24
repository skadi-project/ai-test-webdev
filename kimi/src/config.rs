use std::{collections::HashSet, env, path::PathBuf, time::Duration};

use crate::error::AppError;

const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 300;
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_CACHE_PATH: &str = "data/cache.json";

#[derive(Clone)]
pub struct Config {
    pub github_username: String,
    pub github_token: Option<String>,
    pub refresh_interval: Duration,
    pub port: u16,
    pub admin_token: Option<String>,
    pub pinned_repos: Vec<String>,
    pub excluded_repos: HashSet<String>,
    pub show_forks: bool,
    pub show_archived: bool,
    pub cache_path: Option<PathBuf>,
    pub cors_allow_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let github_username = required_string("GITHUB_USERNAME")?;
        if github_username.is_empty()
            || github_username.len() > 39
            || !github_username
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(AppError::Config(
                "GITHUB_USERNAME must be a valid GitHub username".into(),
            ));
        }

        let refresh_interval_secs = env::var("REFRESH_INTERVAL_SECS")
            .ok()
            .map(|value| parse_number::<u64>("REFRESH_INTERVAL_SECS", &value))
            .transpose()?
            .unwrap_or(DEFAULT_REFRESH_INTERVAL_SECS);
        if refresh_interval_secs == 0 {
            return Err(AppError::Config(
                "REFRESH_INTERVAL_SECS must be greater than zero".into(),
            ));
        }

        let port = env::var("PORT")
            .ok()
            .map(|value| parse_number::<u16>("PORT", &value))
            .transpose()?
            .unwrap_or(DEFAULT_PORT);

        let cache_path = optional_string("CACHE_PATH").map_or_else(
            || Some(PathBuf::from(DEFAULT_CACHE_PATH)),
            |value| {
                if value.eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(PathBuf::from(value))
                }
            },
        );

        Ok(Self {
            github_username,
            github_token: optional_string("GITHUB_TOKEN"),
            refresh_interval: Duration::from_secs(refresh_interval_secs),
            port,
            admin_token: optional_string("ADMIN_TOKEN"),
            pinned_repos: parse_list(&env::var("PINNED_REPOS").unwrap_or_default()),
            excluded_repos: parse_list(&env::var("EXCLUDED_REPOS").unwrap_or_default())
                .into_iter()
                .collect(),
            show_forks: parse_bool_env("SHOW_FORKS", false)?,
            show_archived: parse_bool_env("SHOW_ARCHIVED", false)?,
            cache_path,
            cors_allow_origins: parse_origins(&env::var("CORS_ALLOW_ORIGINS").unwrap_or_default()),
        })
    }
}

fn required_string(name: &str) -> Result<String, AppError> {
    optional_string(name).ok_or_else(|| AppError::Config(format!("{name} is required")))
}

fn optional_string(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_number<T>(name: &str, value: &str) -> Result<T, AppError>
where
    T: std::str::FromStr,
{
    value
        .trim()
        .parse::<T>()
        .map_err(|_| AppError::Config(format!("{name} must be a valid number")))
}

fn parse_bool_env(name: &str, default: bool) -> Result<bool, AppError> {
    match optional_string(name) {
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(AppError::Config(format!(
                "{name} must be one of: true/false, 1/0, yes/no, on/off"
            ))),
        },
        None => Ok(default),
    }
}

fn parse_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn parse_origins(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_list, parse_origins};

    #[test]
    fn repository_lists_are_normalized() {
        assert_eq!(
            parse_list(" Alpha, beta , ,ALPHA"),
            ["alpha", "beta", "alpha"]
        );
    }

    #[test]
    fn origins_preserve_case_and_scheme() {
        assert_eq!(
            parse_origins("https://Example.com, http://localhost:3000"),
            ["https://Example.com", "http://localhost:3000"]
        );
    }
}
