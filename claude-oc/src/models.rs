use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Profile {
    pub login: String,
    pub name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: String,
    pub html_url: String,
    pub public_repos: u64,
    pub followers: u64,
    pub following: u64,
    pub location: Option<String>,
    pub blog: Option<String>,
    pub twitter_username: Option<String>,
    pub company: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub stargazers_count: u64,
    #[serde(default)]
    pub forks_count: u64,
    #[serde(default)]
    pub language: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub pinned: bool,
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct DiskCache {
    pub profile: Profile,
    pub repos: Vec<Repo>,
    pub languages: std::collections::HashMap<String, u64>,
    pub fetched_at: Option<DateTime<Utc>>,
}
