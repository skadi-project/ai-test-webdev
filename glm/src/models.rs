use serde::{Deserialize, Serialize};

/// Public GitHub profile, field names match the GitHub REST API v3 payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    pub avatar_url: String,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub blog: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub twitter_username: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub following: u64,
    #[serde(default)]
    pub public_repos: u64,
    #[serde(default)]
    pub created_at: String,
}

/// Repository summary, field names match the GitHub REST API v3 payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub stargazers_count: u64,
    #[serde(default)]
    pub forks_count: u64,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Not part of the GitHub payload: set by the server for `PINNED_REPOS`.
    #[serde(default)]
    pub pinned: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE_JSON: &str = include_str!("../tests/fixtures/profile.json");
    const REPOS_JSON: &str = include_str!("../tests/fixtures/repos.json");

    #[test]
    fn parses_profile() {
        let p: Profile = serde_json::from_str(PROFILE_JSON).expect("valid profile json");
        assert_eq!(p.login, "octocat");
        assert_eq!(p.name.as_deref(), Some("The Octocat"));
        assert_eq!(p.followers, 9600);
        assert_eq!(p.public_repos, 8);
        assert_eq!(p.twitter_username.as_deref(), None);
        assert!(!p.avatar_url.is_empty());
    }

    #[test]
    fn parses_repo_list() {
        let repos: Vec<Repo> = serde_json::from_str(REPOS_JSON).expect("valid repos json");
        assert_eq!(repos.len(), 3);

        let hello = repos
            .iter()
            .find(|r| r.name == "hello-world")
            .expect("repo");
        assert_eq!(hello.language.as_deref(), Some("Rust"));
        assert_eq!(hello.stargazers_count, 128);
        assert_eq!(hello.topics, vec!["rust", "axum"]);
        assert_eq!(hello.forks_count, 12);
        assert!(!hello.fork);

        let forked = repos
            .iter()
            .find(|r| r.name == "spoon-knife")
            .expect("repo");
        assert!(forked.fork);
        assert!(forked.archived);
        assert_eq!(forked.topics, vec!["tools"]);

        let legacy = repos.iter().find(|r| r.name == "legacy").expect("repo");
        assert_eq!(legacy.language, None);
        assert!(legacy.archived);
    }
}
