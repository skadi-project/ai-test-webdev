use crate::{
    cache::{Cache, language_stats},
    config::Config,
    error::AppError,
    github::GithubClient,
    models::{Repo, RepoQuery, Snapshot},
};
use askama::Template;
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub github: GithubClient,
    pub cache: Cache,
}
#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    snapshot: Snapshot,
}
pub async fn index(State(state): State<Arc<AppState>>) -> Result<Html<String>, AppError> {
    let snapshot = state.cache.get().await;
    let body = IndexTemplate { snapshot }
        .render()
        .map_err(|e| AppError::Template(e.to_string()))?;
    Ok(Html(body))
}
pub async fn profile(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(state.cache.get().await.profile).unwrap_or_default())
}
pub async fn languages(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<crate::models::LanguageStat>> {
    Json(state.cache.get().await.languages)
}
pub async fn repos(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RepoQuery>,
) -> Json<Vec<Repo>> {
    let repos = filtered(state.cache.get().await.repos, &state.config, query);
    Json(repos)
}
fn filtered(mut repos: Vec<Repo>, config: &Config, query: RepoQuery) -> Vec<Repo> {
    repos.retain(|r| {
        (config.show_forks || !r.fork)
            && (config.show_archived || !r.archived)
            && !config.excluded_repos.contains(&r.name.to_lowercase())
    });
    if let Some(lang) = query.language {
        repos.retain(|r| {
            r.language
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(&lang))
        });
    }
    if let Some(topic) = query.topic {
        repos.retain(|r| r.topics.iter().any(|v| v.eq_ignore_ascii_case(&topic)));
    }
    if let Some(search) = query.search {
        let needle = search.to_lowercase();
        repos.retain(|r| {
            r.name.to_lowercase().contains(&needle)
                || r.description
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&needle)
        });
    }
    repos.sort_by_key(|r| {
        if config.pinned_repos.contains(&r.name.to_lowercase()) {
            0
        } else {
            1
        }
    });
    match query.sort.as_deref() {
        Some("stars") => repos.sort_by_key(|a| std::cmp::Reverse(a.stargazers_count)),
        Some("name") => repos.sort_by_key(|r| r.name.to_lowercase()),
        _ => repos.sort_by_key(|a| std::cmp::Reverse(a.updated_at)),
    }
    repos
}
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status":"ok"})))
}
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let allowed = state.config.admin_token.as_deref().is_some_and(|token| {
        headers.get("x-admin-token").and_then(|v| v.to_str().ok()) == Some(token)
    });
    if !allowed {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        ));
    }
    let mut snapshot = state.cache.get().await;
    snapshot.profile = Some(state.github.profile().await?);
    snapshot.repos = state.github.repos().await?;
    for repo in &mut snapshot.repos {
        if let Ok(languages) = state.github.languages(&repo.full_name).await {
            repo.languages = languages;
        }
    }
    snapshot.languages = language_stats(&snapshot.repos);
    state.cache.replace(snapshot).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({"status":"refreshed"})),
    ))
}
