use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
mod config;
mod models;
use chrono::Utc;
use config::Config;
use models::{DiskCache, Profile, Repo};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{env, path::PathBuf, sync::Arc, time::Duration};
use tokio::{fs, sync::RwLock};
use tower_http::trace::TraceLayer;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    username: String,
    github_etags: Arc<RwLock<std::collections::HashMap<String, String>>>,
    admin_token: String,
    cache: Arc<RwLock<DiskCache>>,
    path: PathBuf,
    client: reqwest::Client,
    interval: Duration,
    config: Config,
}
#[derive(Deserialize, Default)]
struct Filters {
    #[serde(rename = "_")]
    _cache_bust: Option<String>,
    q: Option<String>,
    language: Option<String>,
    topic: Option<String>,
    include_forks: Option<bool>,
    include_archived: Option<bool>,
    sort: Option<String>,
}

async fn github<T: serde::de::DeserializeOwned>(s: &AppState, path: &str) -> Result<T, String> {
    let mut request = s
        .client
        .get(format!("https://api.github.com{path}"))
        .header("User-Agent", "github-portfolio/0.2")
        .header("Accept", "application/vnd.github+json");
    if let Some(tag) = s.github_etags.read().await.get(path).cloned() {
        request = request.header(header::IF_NONE_MATCH, tag);
    }
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            request = request.bearer_auth(token);
        }
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if response.status() == StatusCode::NOT_MODIFIED {
        return Err("GitHub endpoint unchanged".into());
    }
    if let Some(tag) = response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
    {
        s.github_etags
            .write()
            .await
            .insert(path.to_owned(), tag.to_owned());
    }
    let status = response.status();
    let body = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "GitHub API {status}: {}",
            body.chars().take(300).collect::<String>()
        ));
    }
    serde_json::from_str(&body).map_err(|e| {
        format!(
            "GitHub JSON decode: {e}; body: {}",
            body.chars().take(300).collect::<String>()
        )
    })
}
async fn refresh_cache(s: &AppState) -> Result<(), String> {
    let profile: Profile =
        github(s, &format!("/users/{}", urlencoding::encode(&s.username))).await?;
    let mut repos: Vec<Repo> = github(
        s,
        &format!(
            "/users/{}/repos?per_page=100&sort=updated",
            urlencoding::encode(&s.username)
        ),
    )
    .await?;
    let pinned: Vec<Repo> = github(
        s,
        &format!(
            "/users/{}/repos?per_page=6&sort=stars",
            urlencoding::encode(&s.username)
        ),
    )
    .await
    .unwrap_or_default();
    let names: std::collections::HashSet<_> = pinned.into_iter().map(|r| r.name).collect();
    for r in &mut repos {
        r.pinned = names.contains(&r.name) || s.config.pinned.iter().any(|n| n == &r.name);
    }
    repos.retain(|r| {
        (s.config.show_forks || !r.fork)
            && (s.config.archived || !r.archived)
            && !s.config.excluded.iter().any(|n| n == &r.name)
    });
    let mut languages = std::collections::HashMap::new();
    for repo in &repos {
        if let Some(language) = &repo.language {
            *languages.entry(language.clone()).or_insert(0) += 1;
        }
    }
    let data = DiskCache {
        profile,
        repos,
        languages,
        fetched_at: Some(Utc::now()),
    };
    fs::create_dir_all(s.path.parent().unwrap_or(std::path::Path::new(".")))
        .await
        .map_err(|e| e.to_string())?;
    fs::write(
        &s.path,
        serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;
    *s.cache.write().await = data;
    Ok(())
}
async fn load(s: &AppState) {
    if let Ok(bytes) = fs::read(&s.path).await {
        if let Ok(data) = serde_json::from_slice(&bytes) {
            *s.cache.write().await = data;
        }
    }
}
fn etag(data: &str) -> String {
    format!("\"{:x}\"", Sha256::digest(data.as_bytes()))
}
async fn api_languages(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let body = serde_json::to_string(&s.cache.read().await.languages).unwrap_or_default();
    conditional(body, headers)
}
async fn health(State(s): State<AppState>) -> impl IntoResponse {
    let c = s.cache.read().await;
    Json(serde_json::json!({"status":"ok","cached_at":c.fetched_at}))
}
async fn api_profile(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let body = serde_json::to_string(&s.cache.read().await.profile).unwrap_or_default();
    conditional(body, headers)
}
async fn api_repos(
    State(s): State<AppState>,
    Query(f): Query<Filters>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let c = s.cache.read().await;
    let q = f.q.unwrap_or_default().to_lowercase();
    let mut repos: Vec<_> = c
        .repos
        .iter()
        .filter(|r| {
            (f.include_forks.unwrap_or(false) || !r.fork)
                && (f.include_archived.unwrap_or(false) || !r.archived)
                && (q.is_empty()
                    || r.name.to_lowercase().contains(&q)
                    || r.description
                        .clone()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&q))
                && f.language
                    .as_deref()
                    .is_none_or(|x| x.is_empty() || r.language.as_deref() == Some(x))
                && f.topic
                    .as_deref()
                    .is_none_or(|x| x.is_empty() || r.topics.iter().any(|t| t == x))
        })
        .cloned()
        .collect();
    repos.sort_by(|a, b| {
        let ordering = match f.sort.as_deref() {
            Some("name") => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            Some("updated") => b.updated_at.cmp(&a.updated_at),
            _ => b.stargazers_count.cmp(&a.stargazers_count),
        };
        b.pinned.cmp(&a.pinned).then(ordering)
    });
    conditional(serde_json::to_string(&repos).unwrap_or_default(), headers)
}
fn conditional(body: String, headers: HeaderMap) -> axum::response::Response {
    let tag = etag(&body);
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|x| x.to_str().ok())
        == Some(&tag)
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, tag)]).into_response();
    }
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, Box::leak(tag.into_boxed_str())),
        ],
        body,
    )
        .into_response()
}
async fn refresh(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if headers.get("ADMIN_TOKEN").and_then(|x| x.to_str().ok()) != Some(&s.admin_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match refresh_cache(&s).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            error!(%e,"refresh failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}
async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}
async fn app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../static/app.js"),
    )
}
async fn style_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../static/style.css"),
    )
}
async fn favicon() -> StatusCode {
    StatusCode::NO_CONTENT
}
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config = match Config::from_env() {
        Ok(v) => v,
        Err(e) => {
            error!(%e, "invalid configuration");
            return;
        }
    };
    let s = AppState {
        username: config.username.clone(),
        admin_token: config.admin_token.clone(),
        path: config.cache_file.clone(),
        client: reqwest::Client::new(),
        cache: Arc::new(RwLock::new(DiskCache::default())),
        github_etags: Arc::new(RwLock::new(std::collections::HashMap::new())),
        interval: config.interval,
        config,
    };
    load(&s).await;
    if let Err(e) = refresh_cache(&s).await {
        error!(%e, username = %s.username, "initial GitHub refresh failed; serving disk cache");
    }
    let bg = s.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(bg.interval).await;
            if let Err(e) = refresh_cache(&bg).await {
                error!(%e,"background refresh failed");
            }
        }
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/style.css", get(style_css))
        .route("/favicon.ico", get(favicon))
        .route("/health", get(health))
        .route("/api/profile", get(api_profile))
        .route("/api/repos", get(api_repos))
        .route("/api/languages", get(api_languages))
        .route("/api/refresh", post(refresh))
        .with_state(s.clone())
        .layer(TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::compression::CompressionLayer::new());
    let port = s.config.port.clone();
    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
        Ok(v) => v,
        Err(e) => {
            error!(%e,"bind failed");
            return;
        }
    };
    info!(%port,"listening");
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
    {
        error!(%e,"server failed");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_etag() {
        assert_eq!(etag("x"), etag("x"));
    }
}
