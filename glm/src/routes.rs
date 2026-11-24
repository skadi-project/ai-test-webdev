use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{
    cache::{self, AppState},
    error::AppError,
    filter::{self, SortKey},
};

const INDEX_HTML: &str = include_str!("../static/index.html");
const STYLE_CSS: &str = include_str!("../static/style.css");
const APP_JS: &str = include_str!("../static/app.js");

/// Build the full application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/api/profile", get(api_profile))
        .route("/api/repos", get(api_repos))
        .route("/api/stats/languages", get(api_stats_languages))
        .route("/api/refresh", post(api_refresh))
        .route("/assets/style.css", get(stylesheet))
        .route("/assets/app.js", get(script))
        .layer(CompressionLayer::new())
        .layer(
            CorsLayer::new()
                .allow_methods([Method::GET, Method::POST])
                .allow_origin(tower_http::cors::Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn index() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        INDEX_HTML,
    )
}

async fn stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLE_CSS,
    )
}

async fn script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.cache.read().await;
    Json(json!({
        "status": "ok",
        "username": state.config.github_username,
        "repos": guard.repos.len(),
        "updated_at": guard.updated_at,
    }))
}

async fn api_profile(State(state): State<AppState>) -> Response {
    let guard = state.cache.read().await;
    match &guard.profile {
        Some(profile) => Json(json!({
            "profile": profile,
            "updated_at": guard.updated_at,
        }))
        .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "profile is not loaded yet, retry shortly" })),
        )
            .into_response(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct RepoQuery {
    q: Option<String>,
    language: Option<String>,
    topic: Option<String>,
    sort: Option<String>,
    order: Option<String>,
}

async fn api_repos(
    State(state): State<AppState>,
    Query(query): Query<RepoQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sort = SortKey::parse(query.sort.as_deref());
    let ascending = query
        .order
        .as_deref()
        .is_some_and(|o| o.eq_ignore_ascii_case("asc"));

    let guard = state.cache.read().await;
    if guard.repos.is_empty() && guard.updated_at.is_none() {
        return Ok(Json(json!({
            "repos": [],
            "count": 0,
            "total": 0,
            "updated_at": null,
            "languages": [],
            "topics": [],
            "pending": true,
        })));
    }

    let visible = filter::view_repos(&guard.repos, &state.config);
    let mut languages: Vec<String> = visible.iter().filter_map(|r| r.language.clone()).collect();
    languages.sort();
    languages.dedup();

    let mut topics: Vec<String> = visible
        .iter()
        .flat_map(|r| r.topics.iter().cloned())
        .collect();
    topics.sort();
    topics.dedup();

    let mut selected = filter::search_filter(
        visible,
        query.q.as_deref(),
        query.language.as_deref(),
        query.topic.as_deref(),
    );
    filter::sort_repos(&mut selected, sort, ascending);

    Ok(Json(json!({
        "repos": selected,
        "count": selected.len(),
        "total": guard.repos.len(),
        "updated_at": guard.updated_at,
        "sort": sort.as_str(),
        "order": if ascending { "asc" } else { "desc" },
        "languages": languages,
        "topics": topics,
    })))
}

async fn api_stats_languages(State(state): State<AppState>) -> impl IntoResponse {
    let (languages, total_bytes) = cache::language_stats(&state).await;
    let guard = state.cache.read().await;
    Json(json!({
        "languages": languages,
        "total_bytes": total_bytes,
        "updated_at": guard.updated_at,
    }))
}

async fn api_refresh(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(expected) = state.config.admin_token.as_deref() else {
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({ "error": "refresh endpoint is disabled (no ADMIN_TOKEN configured)" })),
        )
            .into_response();
    };

    let authorized = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .strip_prefix("Bearer ")
                .is_some_and(|token| token == expected)
        });

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing or invalid admin token" })),
        )
            .into_response();
    }

    state.refresh_notify.notify_one();
    tracing::info!("refresh triggered via /api/refresh");
    Json(json!({ "status": "refresh scheduled" })).into_response()
}
