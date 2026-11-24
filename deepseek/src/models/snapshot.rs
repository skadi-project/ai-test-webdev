//! Normalized, cache-friendly domain models shared by the refresher and the API.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::colors;
use crate::models::github::{GitHubRepo, GitHubUser, LanguageBytes};

/// Public profile information rendered by the site.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
    pub html_url: String,
    pub bio: Option<String>,
    pub location: Option<String>,
    pub blog: Option<String>,
    pub twitter_username: Option<String>,
    pub company: Option<String>,
    pub followers: u64,
    pub following: u64,
    pub public_repos: u64,
}

impl Profile {
    pub fn from_github(user: GitHubUser) -> Self {
        Self {
            login: user.login,
            name: user.name.filter(|v| !v.trim().is_empty()),
            avatar_url: user.avatar_url,
            html_url: user.html_url,
            bio: user.bio.filter(|v| !v.trim().is_empty()),
            location: user.location.filter(|v| !v.trim().is_empty()),
            blog: user.blog.filter(|v| !v.trim().is_empty()),
            twitter_username: user.twitter_username.filter(|v| !v.trim().is_empty()),
            company: user.company.filter(|v| !v.trim().is_empty()),
            followers: user.followers,
            following: user.following,
            public_repos: user.public_repos,
        }
    }

    /// Display name fallback chain: name → login.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.login)
    }
}

/// One repository, normalized for the API and the cache.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub homepage: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u64,
    pub forks_count: u64,
    pub topics: Vec<String>,
    pub updated_at: Option<String>,
    pub fork: bool,
    pub archived: bool,
    pub pinned: bool,
}

impl Repo {
    pub fn from_github(repo: GitHubRepo, pinned: bool) -> Self {
        let full_name = if repo.full_name.is_empty() {
            match repo.owner {
                Some(owner) => format!("{}/{}", owner.login, repo.name),
                None => repo.name.clone(),
            }
        } else {
            repo.full_name
        };
        let mut topics: Vec<String> = Vec::new();
        for topic in repo.topics {
            let topic = topic.trim().to_ascii_lowercase();
            if !topic.is_empty() && !topics.contains(&topic) {
                topics.push(topic);
            }
        }
        Self {
            id: repo.id,
            name: repo.name,
            full_name,
            description: repo
                .description
                .map(|d| d.trim().to_owned())
                .filter(|d| !d.is_empty()),
            html_url: repo.html_url,
            homepage: repo
                .homepage
                .map(|h| h.trim().to_owned())
                .filter(|h| !h.is_empty()),
            language: repo.language.filter(|l| !l.trim().is_empty()),
            stargazers_count: repo.stargazers_count,
            forks_count: repo.forks_count,
            topics,
            updated_at: repo.updated_at,
            fork: repo.fork,
            archived: repo.archived,
            pinned,
        }
    }

    /// One-line description used by the SSR fallback and `<meta>` tags.
    pub fn description_text(&self) -> &str {
        self.description
            .as_deref()
            .unwrap_or("Описание пока не добавлено.")
    }

    pub fn language_or_unknown(&self) -> &str {
        self.language.as_deref().unwrap_or("Other")
    }

    /// Lowercased name + description, for case-insensitive search.
    pub fn search_blob(&self) -> String {
        let mut blob = self.name.to_lowercase();
        if let Some(description) = &self.description {
            blob.push(' ');
            blob.push_str(&description.to_lowercase());
        }
        blob
    }
}

/// Cached language breakdown for a single repository.
///
/// An empty map is a valid result (for example, an empty repository), so freshness
/// is tracked by comparing the repository's `updated_at` with the one seen when
/// the breakdown was fetched.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoLanguages {
    pub bytes: u64,
    pub languages: LanguageBytes,
    /// ETag of the last successful `/languages` response.
    pub etag: Option<String>,
    /// Value of the repository's `updated_at` when languages were fetched.
    /// Used to decide whether the breakdown may have changed.
    #[serde(default)]
    pub repo_updated_at: Option<String>,
}

impl RepoLanguages {
    pub fn from_bytes(
        languages: LanguageBytes,
        etag: Option<String>,
        repo_updated_at: Option<String>,
    ) -> Self {
        let bytes = languages.values().sum();
        Self {
            bytes,
            languages,
            etag,
            repo_updated_at,
        }
    }

    /// True when `repo.updated_at` differs from the value seen at fetch time.
    pub fn is_stale_for(&self, repo_updated_at: Option<&str>) -> bool {
        self.repo_updated_at.as_deref() != repo_updated_at
    }
}

/// Aggregated language statistics for the chart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageStat {
    pub name: String,
    pub bytes: u64,
    pub color: String,
}

/// Immutable snapshot served to readers. Cheap to clone thanks to `Arc` in the cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub profile: Profile,
    /// Repositories after `EXCLUDED_REPOS`/`PINNED_REPOS` are applied.
    pub repos: Vec<Repo>,
    /// Raw GitHub payloads, kept so a config change (pins/exclusions) can be
    /// re-applied without re-downloading anything.
    #[serde(default)]
    pub source_repos: Vec<GitHubRepo>,
    #[serde(default)]
    pub repo_pages: BTreeMap<String, crate::github::RepoPage>,
    #[serde(default)]
    pub repo_languages: BTreeMap<String, RepoLanguages>,
    pub etag_profile: Option<String>,
    /// Retained for cache-format compatibility; per-page validators live in `repo_pages`.
    #[serde(default)]
    pub etag_repos: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl Snapshot {
    /// Aggregated language bytes across all repositories, descending by size.
    pub fn aggregate_languages(&self) -> Vec<LanguageStat> {
        let mut totals: BTreeMap<String, u64> = BTreeMap::new();
        for repo in &self.repos {
            let Some(stats) = self.repo_languages.get(&repo.full_name) else {
                continue;
            };
            for (language, bytes) in &stats.languages {
                *totals.entry(language.clone()).or_default() += *bytes;
            }
        }
        let mut stats: Vec<LanguageStat> = totals
            .into_iter()
            .map(|(name, bytes)| LanguageStat {
                color: colors::color_for(&name).to_owned(),
                name,
                bytes,
            })
            .collect();
        stats.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_trims_blank_fields() {
        let user: GitHubUser =
            serde_json::from_str(include_str!("../../tests/fixtures/user.json")).expect("user");
        let profile = Profile::from_github(user);
        assert_eq!(profile.display_name(), "The Octocat");
        assert_eq!(profile.bio.as_deref(), Some("Builder of things."));
        assert_eq!(profile.blog.as_deref(), Some("https://octocat.example"));
    }

    #[test]
    fn repo_normalizes_topics_and_full_name_fallback() {
        let raw: GitHubRepo = serde_json::from_str(
            r#"{"id":7,"name":"demo","description":"  hello  ","html_url":"https://github.com/x/demo",
                "homepage":"  ","topics":[" Rust ","rust",""],"owner":{"login":"octocat"}}"#,
        )
        .expect("repo");
        let repo = Repo::from_github(raw, true);
        assert_eq!(repo.full_name, "octocat/demo");
        assert_eq!(repo.description.as_deref(), Some("hello"));
        assert_eq!(repo.topics, vec!["rust".to_owned()]);
        assert!(repo.homepage.is_none());
        assert!(repo.pinned);
        assert_eq!(repo.language_or_unknown(), "Other");
    }

    #[test]
    fn language_aggregation_sums_repositories() {
        let mut snapshot = Snapshot {
            profile: Profile::default(),
            repos: ["one", "two"]
                .into_iter()
                .map(|name| {
                    let raw: GitHubRepo = serde_json::from_value(
                        serde_json::json!({"id":1,"name":name,"full_name":format!("a/{name}")}),
                    )
                    .expect("repo");
                    Repo::from_github(raw, false)
                })
                .collect(),
            repo_pages: BTreeMap::new(),
            source_repos: Vec::new(),
            repo_languages: BTreeMap::new(),
            etag_profile: None,
            etag_repos: None,
            updated_at: Utc::now(),
        };
        snapshot.repo_languages.insert(
            "a/one".into(),
            RepoLanguages::from_bytes(
                [("Rust".to_owned(), 100), ("HTML".to_owned(), 50)]
                    .into_iter()
                    .collect(),
                None,
                None,
            ),
        );
        snapshot.repo_languages.insert(
            "a/two".into(),
            RepoLanguages::from_bytes([("Rust".to_owned(), 300)].into_iter().collect(), None, None),
        );
        let stats = snapshot.aggregate_languages();
        assert_eq!(stats[0].name, "Rust");
        assert_eq!(stats[0].bytes, 400);
        assert_eq!(stats[1].name, "HTML");
        assert_eq!(stats[1].bytes, 50);
        assert_eq!(stats[0].color, colors::color_for("Rust"));
    }
}
