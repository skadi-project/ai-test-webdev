use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Profile {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
    pub html_url: String,
    pub bio: Option<String>,
    pub location: Option<String>,
    pub blog: Option<String>,
    pub company: Option<String>,
    pub twitter_username: Option<String>,
    pub public_repos: u64,
    pub followers: u64,
    pub following: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u64,
    pub forks_count: u64,
    pub topics: Vec<String>,
    pub fork: bool,
    pub archived: bool,
    pub homepage: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    pub pushed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub languages: std::collections::HashMap<String, u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LanguageStat {
    pub name: String,
    pub bytes: u64,
    pub percentage: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Snapshot {
    pub profile: Option<Profile>,
    pub repos: Vec<Repo>,
    pub languages: Vec<LanguageStat>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub etag_profile: Option<String>,
    pub etag_repos: Option<String>,
}
#[derive(Deserialize)]
pub struct RepoQuery {
    pub language: Option<String>,
    pub topic: Option<String>,
    pub search: Option<String>,
    pub sort: Option<String>,
}
