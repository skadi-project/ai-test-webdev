//! GitHub REST API payloads (only the fields the site needs).
//!
//! Parsing is deliberately lenient: unknown fields are ignored and optional
//! values are boxed in `Option` so upstream API additions never break us.

use serde::{Deserialize, Serialize};

/// `GET /users/{user}`
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GitHubUser {
    pub login: String,
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub blog: Option<String>,
    #[serde(default)]
    pub twitter_username: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub following: u64,
    #[serde(default)]
    pub public_repos: u64,
}

/// `GET /users/{user}/repos`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubRepo {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub stargazers_count: u64,
    #[serde(default)]
    pub forks_count: u64,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub languages_url: Option<String>,
    #[serde(default)]
    pub owner: Option<GitHubOwner>,
}

/// Minimal owner block nested inside a repository.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubOwner {
    pub login: String,
}

/// `GET /repos/{owner}/{repo}/languages` → `{ "Rust": 12345, "HTML": 900 }`
pub type LanguageBytes = std::collections::BTreeMap<String, u64>;

#[cfg(test)]
mod tests {
    use super::*;

    const USER_JSON: &str = include_str!("../../tests/fixtures/user.json");
    const REPOS_JSON: &str = include_str!("../../tests/fixtures/repos.json");
    const LANGUAGES_JSON: &str = include_str!("../../tests/fixtures/languages.json");

    #[test]
    fn parses_user_fixture() {
        let user: GitHubUser = serde_json::from_str(USER_JSON).expect("user json");
        assert_eq!(user.login, "octocat");
        assert_eq!(user.name.as_deref(), Some("The Octocat"));
        assert_eq!(user.followers, 12345);
        assert_eq!(user.public_repos, 8);
        assert_eq!(user.twitter_username.as_deref(), Some("octocat"));
        // Unknown keys in the payload must be ignored.
        assert_eq!(user.blog.as_deref(), Some("https://octocat.example"));
    }

    #[test]
    fn parses_repos_fixture() {
        let repos: Vec<GitHubRepo> = serde_json::from_str(REPOS_JSON).expect("repos json");
        assert_eq!(repos.len(), 5);
        let sprocket = repos
            .iter()
            .find(|r| r.name == "Spoon-Knife")
            .expect("repo");
        assert!(sprocket.fork);
        assert_eq!(sprocket.language.as_deref(), Some("HTML"));
        assert!(repos.iter().any(|r| r.archived));
        assert!(repos
            .iter()
            .any(|r| r.topics.iter().any(|t| t == "system-design")));
        assert!(repos.iter().all(|r| r.id > 0));
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let repo: GitHubRepo =
            serde_json::from_str(r#"{"id": 1, "name": "bare"}"#).expect("minimal repo");
        assert_eq!(repo.name, "bare");
        assert!(repo.description.is_none());
        assert!(repo.topics.is_empty());
        assert!(!repo.fork && !repo.archived);
        assert_eq!(repo.stargazers_count, 0);
    }

    #[test]
    fn parses_languages_fixture() {
        let langs: LanguageBytes = serde_json::from_str(LANGUAGES_JSON).expect("languages");
        assert_eq!(langs.get("Rust"), Some(&10_000));
        assert_eq!(langs.get("HTML"), Some(&500));
    }
}
