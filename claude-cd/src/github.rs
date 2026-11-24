use crate::{
    error::AppError,
    models::{Profile, Repo},
};
use reqwest::{Client, StatusCode, header};
use serde::de::DeserializeOwned;

#[derive(Clone)]
pub struct GithubClient {
    client: Client,
    base: String,
    username: String,
    token: Option<String>,
}
impl GithubClient {
    pub fn new(base: String, username: String, token: Option<String>) -> Result<Self, AppError> {
        let client = Client::builder()
            .user_agent("github-portfolio/0.1")
            .build()
            .map_err(|e| AppError::Github(e.to_string()))?;
        Ok(Self {
            client,
            base: base.trim_end_matches('/').into(),
            username,
            token,
        })
    }
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, AppError> {
        let mut req = self
            .client
            .get(format!("{}/{}", self.base, path))
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .await
            .map_err(|e| AppError::Github(e.to_string()))?;
        let status = response.status();
        if status != StatusCode::OK {
            return Err(AppError::Github(format!("GitHub returned {status}")));
        }
        response
            .json()
            .await
            .map_err(|e| AppError::Github(e.to_string()))
    }
    pub async fn profile(&self) -> Result<Profile, AppError> {
        self.get(&format!("users/{}", self.username)).await
    }
    pub async fn repos(&self) -> Result<Vec<Repo>, AppError> {
        let mut all = Vec::new();
        for page in 1..=5 {
            let mut repos: Vec<Repo> = self
                .get(&format!(
                    "users/{}/repos?per_page=100&page={page}&sort=updated",
                    self.username
                ))
                .await?;
            let empty = repos.is_empty();
            all.append(&mut repos);
            if empty {
                break;
            }
        }
        Ok(all)
    }
    pub async fn languages(
        &self,
        full_name: &str,
    ) -> Result<std::collections::HashMap<String, u64>, AppError> {
        self.get(&format!("repos/{full_name}/languages")).await
    }
}
