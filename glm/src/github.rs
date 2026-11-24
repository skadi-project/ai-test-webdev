use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::{
    header::{HeaderMap, ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH, USER_AGENT},
    StatusCode, Url,
};
use serde::de::DeserializeOwned;

/// Result of a conditional HTTP request.
#[derive(Debug)]
pub enum Fetch<T> {
    /// 200 OK with the decoded body and an optional ETag to store.
    Fresh(T, Option<String>),
    /// 304 Not Modified: the cached copy is still valid.
    NotModified,
}

/// Thin client for the GitHub REST API v3 with conditional-request support.
#[derive(Clone)]
pub struct GitHubClient {
    http: reqwest::Client,
}

impl GitHubClient {
    /// Build a client with the common GitHub headers and a 20 s timeout.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be constructed (bad token
    /// header value or TLS backend failure).
    pub fn new(token: Option<&str>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            "gh-portfolio/0.1".parse().context("user agent")?,
        );
        headers.insert(
            ACCEPT,
            "application/vnd.github+json".parse().context("accept")?,
        );
        if let Some(token) = token {
            let value = format!("Bearer {token}")
                .parse()
                .context("github token header")?;
            headers.insert(AUTHORIZATION, value);
        }
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .context("build http client")?;
        Ok(Self { http })
    }

    fn url(&self, path: &str) -> Result<Url> {
        Url::parse("https://api.github.com")
            .context("parse base url")?
            .join(path)
            .with_context(|| format!("build url for {path}"))
    }

    /// GET `path`, sending `etag` as `If-None-Match` when present.
    ///
    /// # Errors
    /// Network failures, non-success statuses (other than 304) and malformed
    /// JSON bodies are returned as errors.
    pub async fn get_json<T>(&self, path: &str, etag: Option<&str>) -> Result<Fetch<T>>
    where
        T: DeserializeOwned,
    {
        let url = self.url(path)?;
        let mut request = self.http.get(url.clone());
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = response.status();
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        match status {
            StatusCode::OK => {
                let body = response
                    .text()
                    .await
                    .with_context(|| format!("read {path} body"))?;
                let value = serde_json::from_str(&body)
                    .with_context(|| format!("decode {path} response"))?;
                Ok(Fetch::Fresh(value, etag))
            }
            StatusCode::NOT_MODIFIED => Ok(Fetch::NotModified),
            other => {
                let body = response.text().await.unwrap_or_default();
                bail!("GET {path} failed: status {other}, body: {body}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_urls_safely() {
        let client = GitHubClient::new(None).expect("client");
        let url = client.url("/users/octocat").expect("url");
        assert_eq!(url.as_str(), "https://api.github.com/users/octocat");

        // Path traversal must not escape the API host.
        let url = client.url("/users/../admin").expect("url");
        assert!(url.as_str().starts_with("https://api.github.com/"));
    }
}
