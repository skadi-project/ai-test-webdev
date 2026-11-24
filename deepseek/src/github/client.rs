//! Conditional GitHub REST requests. Each repository page has its own validator.
use super::GitHubError;
use crate::{
    config::Config,
    models::{GitHubRepo, GitHubUser, LanguageBytes},
};
use reqwest::{
    header::{HeaderMap, ETAG, IF_NONE_MATCH, LINK},
    Client, StatusCode, Url,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Conditional<T> {
    pub value: Option<T>,
    pub etag: Option<String>,
    pub not_modified: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoPage {
    pub repos: Vec<GitHubRepo>,
    pub etag: Option<String>,
    pub next: Option<String>,
}
#[derive(Debug, Clone, Default)]
pub struct LinkHeader {
    pub next: Option<String>,
}
impl LinkHeader {
    pub fn parse(raw: &str) -> Self {
        let next = raw.split(',').find_map(|part| {
            let (url, params) = part.split_once(';')?;
            params
                .split(';')
                .any(|p| p.trim() == "rel=\"next\"")
                .then(|| {
                    url.trim()
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        .to_owned()
                })
        });
        Self { next }
    }
}
#[derive(Clone)]
pub struct GitHubClient {
    http: Client,
    base: String,
    username: String,
    token: Option<String>,
    blocked_until: Arc<Mutex<Option<std::time::Instant>>>,
}
impl GitHubClient {
    pub fn new(config: &Config) -> Result<Self, GitHubError> {
        let http = Client::builder()
            .user_agent(concat!("github-portfolio/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| GitHubError::Network(e.to_string()))?;
        Ok(Self {
            http,
            base: config.github_api_base.clone(),
            username: config.github_username.clone(),
            token: config.github_token.clone(),
            blocked_until: Arc::new(Mutex::new(None)),
        })
    }
    pub async fn user(&self, etag: Option<&str>) -> Result<Conditional<GitHubUser>, GitHubError> {
        let (result, _) = self
            .get(&format!("{}/users/{}", self.base, self.username), etag)
            .await?;
        Ok(result)
    }
    /// All pages are revalidated, even if page 1 returns 304. Cache includes Link.
    pub async fn repos(
        &self,
        previous: &BTreeMap<String, RepoPage>,
    ) -> Result<(Vec<GitHubRepo>, BTreeMap<String, RepoPage>), GitHubError> {
        let mut next = Some(format!(
            "{}/users/{}/repos?per_page=100&sort=full_name&type=owner",
            self.base, self.username
        ));
        let mut pages = BTreeMap::new();
        let mut repos = Vec::new();
        let mut ids = HashSet::new();
        while let Some(url) = next {
            if pages.contains_key(&url) || pages.len() >= 1000 {
                return Err(GitHubError::Decode(
                    "pagination loop or >100000 repositories".into(),
                ));
            }
            let old = previous.get(&url);
            let (result, headers) = self
                .get::<Vec<GitHubRepo>>(&url, old.and_then(|p| p.etag.as_deref()))
                .await?;
            let page = if result.not_modified {
                old.cloned()
                    .ok_or_else(|| GitHubError::Decode("304 without cached page".into()))?
            } else {
                RepoPage {
                    repos: result
                        .value
                        .ok_or_else(|| GitHubError::Decode("missing page body".into()))?,
                    etag: result.etag,
                    next: headers
                        .get(LINK)
                        .and_then(|h| h.to_str().ok())
                        .and_then(|h| LinkHeader::parse(h).next),
                }
            };
            next = page.next.clone();
            repos.extend(page.repos.iter().filter(|r| ids.insert(r.id)).cloned());
            pages.insert(url, page);
        }
        Ok((repos, pages))
    }
    pub async fn languages(
        &self,
        full_name: &str,
        etag: Option<&str>,
    ) -> Result<Conditional<LanguageBytes>, GitHubError> {
        let parts: Vec<_> = full_name.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|p| {
                p.is_empty()
                    || *p == "."
                    || *p == ".."
                    || !p
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            })
        {
            return Err(GitHubError::Decode("invalid repository identifier".into()));
        }
        let (result, _) = self
            .get(&format!("{}/repos/{full_name}/languages", self.base), etag)
            .await?;
        Ok(result)
    }
    async fn get<T: DeserializeOwned>(
        &self,
        raw: &str,
        etag: Option<&str>,
    ) -> Result<(Conditional<T>, HeaderMap), GitHubError> {
        if self
            .blocked_until
            .lock()
            .await
            .is_some_and(|t| t > std::time::Instant::now())
        {
            return Err(GitHubError::RateLimited {
                remaining: 0,
                reset_at: None,
            });
        }
        let url = validate_url(raw, &self.base)?;
        let mut request = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(value) = etag {
            request = request.header(IF_NONE_MATCH, value);
        }
        let response = request
            .send()
            .await
            .map_err(|e| GitHubError::Network(e.to_string()))?;
        let status = response.status();
        let headers = response.headers().clone();
        let response_etag = headers
            .get(ETAG)
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        if status == StatusCode::NOT_MODIFIED {
            return Ok((
                Conditional {
                    value: None,
                    etag: response_etag.or_else(|| etag.map(str::to_owned)),
                    not_modified: true,
                },
                headers,
            ));
        }
        if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::FORBIDDEN {
            let reset = headers
                .get("x-ratelimit-reset")
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned);
            let retry = headers
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let now = chrono::Utc::now().timestamp().max(0) as u64;
            let seconds = retry
                .or_else(|| {
                    reset
                        .as_deref()
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|t| t.saturating_sub(now) + 1)
                })
                .unwrap_or(60)
                .clamp(1, 86400);
            *self.blocked_until.lock().await =
                Some(std::time::Instant::now() + Duration::from_secs(seconds));
            return Err(GitHubError::RateLimited {
                remaining: 0,
                reset_at: reset,
            });
        }
        match status {
            StatusCode::OK => {
                let value = response
                    .json()
                    .await
                    .map_err(|e| GitHubError::Decode(e.to_string()))?;
                Ok((
                    Conditional {
                        value: Some(value),
                        etag: response_etag,
                        not_modified: false,
                    },
                    headers,
                ))
            }
            StatusCode::NOT_FOUND => Err(GitHubError::NotFound),
            StatusCode::UNAUTHORIZED => Err(GitHubError::Unauthorized { status: 401 }),
            _ => Err(GitHubError::UnexpectedStatus {
                status: status.as_u16(),
                body: "upstream request rejected".into(),
            }),
        }
    }
}
fn validate_url(raw: &str, base: &str) -> Result<Url, GitHubError> {
    let url = Url::parse(raw).map_err(|e| GitHubError::Decode(e.to_string()))?;
    let base = Url::parse(base).map_err(|e| GitHubError::Decode(e.to_string()))?;
    if url.origin() != base.origin() || !url.username().is_empty() || url.password().is_some() {
        return Err(GitHubError::Decode("pagination URL origin mismatch".into()));
    }
    Ok(url)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links() {
        assert_eq!(LinkHeader::parse("<https://api.github.com/x?page=2>; rel=\"next\", <https://api.github.com/x?page=5>; rel=\"last\"").next.as_deref(), Some("https://api.github.com/x?page=2"));
        assert!(LinkHeader::parse("invalid").next.is_none());
    }
    #[test]
    fn rejects_other_origin() {
        assert!(validate_url("https://evil.example/x", "https://api.github.com").is_err());
        assert!(validate_url("https://api.github.com:444/x", "https://api.github.com").is_err());
        assert!(validate_url("https://api.github.com/x", "https://api.github.com").is_ok());
    }
}
