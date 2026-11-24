use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
    pub html_url: String,
    pub bio: Option<String>,
    pub location: Option<String>,
    pub company: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub blog: Option<String>,
    pub twitter_username: Option<String>,
    pub followers: u64,
    pub public_repos: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Repository {
    pub name: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub description: Option<String>,
    pub html_url: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub homepage: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u64,
    pub forks_count: u64,
    #[serde(default)]
    pub topics: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub fork: bool,
    pub archived: bool,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageStat {
    pub name: String,
    pub bytes: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Snapshot {
    pub profile: Option<Profile>,
    pub repositories: Vec<Repository>,
    pub languages: Vec<LanguageStat>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
    pub updated_at: DateTime<Utc>,
}

impl<T> ApiResponse<T> {
    pub fn new(data: T, updated_at: DateTime<Utc>) -> Self {
        Self { data, updated_at }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryQuery {
    pub q: Option<String>,
    pub language: Option<String>,
    pub topic: Option<String>,
    #[serde(default)]
    pub sort: RepositorySort,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RepositorySort {
    #[default]
    Stars,
    Updated,
    Name,
}

pub type LanguageMap = BTreeMap<String, u64>;

fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(
        Option::<String>::deserialize(deserializer)?.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        }),
    )
}
