use std::time::Duration;

use reqwest::{
    header::{HeaderValue, ACCEPT, ETAG, IF_NONE_MATCH, USER_AGENT},
    Client, StatusCode, Url,
};
use serde::{de::DeserializeOwned, Deserialize};

use crate::{
    config::Config,
    error::AppError,
    models::{LanguageMap, Profile, Repository},
};

pub const REPOSITORIES_PER_PAGE: usize = 100;

#[derive(Clone)]
pub struct GitHubClient {
    client: Client,
    username: String,
    token: Option<String>,
}

#[derive(Debug)]
pub enum FetchResult<T> {
    NotModified,
    Modified { data: T, etag: Option<String> },
}

impl GitHubClient {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(25))
            .user_agent("github-portfolio/1.0")
            .build()?;
        Ok(Self {
            client,
            username: config.github_username.clone(),
            token: config.github_token.clone(),
        })
    }

    pub async fn fetch_profile(
        &self,
        etag: Option<&str>,
    ) -> Result<FetchResult<Profile>, AppError> {
        let url = self.endpoint(&["users", &self.username])?;
        self.get_json(url, etag).await
    }

    pub async fn fetch_repositories_page(
        &self,
        page: u32,
        etag: Option<&str>,
    ) -> Result<FetchResult<Vec<Repository>>, AppError> {
        let mut url = self.endpoint(&["users", &self.username, "repos"])?;
        url.query_pairs_mut()
            .append_pair("per_page", "100")
            .append_pair("page", &page.to_string())
            .append_pair("sort", "updated")
            .append_pair("direction", "desc")
            .append_pair("type", "owner");
        self.get_json(url, etag).await
    }

    pub async fn fetch_languages(
        &self,
        repository: &str,
        etag: Option<&str>,
    ) -> Result<FetchResult<LanguageMap>, AppError> {
        let url = self.endpoint(&["repos", &self.username, repository, "languages"])?;
        self.get_json(url, etag).await
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url, AppError> {
        let mut url = Url::parse("https://api.github.com")?;
        url.path_segments_mut()
            .map_err(|_| AppError::Config("the GitHub API base URL cannot be a base URL".into()))?
            .clear()
            .extend(segments);
        Ok(url)
    }

    async fn get_json<T>(&self, url: Url, etag: Option<&str>) -> Result<FetchResult<T>, AppError>
    where
        T: DeserializeOwned,
    {
        let mut request = self
            .client
            .get(url)
            .header(USER_AGENT, HeaderValue::from_static("github-portfolio/1.0"))
            .header(
                ACCEPT,
                HeaderValue::from_static("application/vnd.github+json"),
            )
            .header("X-GitHub-Api-Version", "2022-11-28");

        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request.send().await?;
        let status = response.status();
        if status == StatusCode::NOT_MODIFIED {
            return Ok(FetchResult::NotModified);
        }

        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        if matches!(
            status,
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
        ) {
            let remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok());
            if remaining == Some("0") || status == StatusCode::TOO_MANY_REQUESTS {
                let reset = response
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                return Err(AppError::RateLimited { reset });
            }
        }

        if !status.is_success() {
            let message = match response.text().await {
                Ok(body) => {
                    if let Ok(payload) = serde_json::from_str::<GitHubError>(&body) {
                        payload.message
                    } else {
                        body.chars().take(300).collect()
                    }
                }
                Err(_) => "unable to read the error response".into(),
            };
            return Err(AppError::GitHub { status, message });
        }

        let data = response.json::<T>().await?;
        Ok(FetchResult::Modified { data, etag })
    }
}

#[derive(Deserialize)]
struct GitHubError {
    message: String,
}
