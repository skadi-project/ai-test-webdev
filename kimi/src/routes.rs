use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{
        header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
        HeaderMap, HeaderName, HeaderValue, Method, StatusCode,
    },
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    error::AppError,
    models::{ApiResponse, RepositoryQuery},
    refresh,
    repository::filter_and_sort,
    state::AppState,
};

pub fn router(state: Arc<AppState>) -> Result<Router, AppError> {
    let cors = cors_layer(&state.config.cors_allow_origins)?;
    Ok(Router::new()
        .route("/", get(index))
        .route("/assets/app.css", get(stylesheet))
        .route("/assets/app.js", get(javascript))
        .route("/assets/favicon.svg", get(favicon))
        .route("/api/profile", get(profile))
        .route("/api/repos", get(repositories))
        .route("/api/languages", get(languages))
        .route("/api/refresh", post(force_refresh))
        .route("/health", get(health))
        .fallback(not_found)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http()))
}

async fn index() -> impl IntoResponse {
    (
        [(CACHE_CONTROL, "no-cache")],
        Html(include_str!("../static/index.html")),
    )
}

async fn stylesheet() -> Response {
    static_response("text/css; charset=utf-8", include_str!("../static/app.css"))
}

async fn javascript() -> Response {
    static_response(
        "text/javascript; charset=utf-8",
        include_str!("../static/app.js"),
    )
}

async fn favicon() -> Response {
    static_response("image/svg+xml", include_str!("../static/favicon.svg"))
}

fn static_response(content_type: &'static str, body: &'static str) -> Response {
    (
        [
            (CONTENT_TYPE, content_type),
            (CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

async fn profile(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<crate::models::Profile>>, AppError> {
    let snapshot = state.cache.snapshot().await;
    let updated_at = snapshot.updated_at.ok_or(AppError::CacheEmpty)?;
    let profile = snapshot.profile.ok_or(AppError::CacheEmpty)?;
    Ok(Json(ApiResponse::new(profile, updated_at)))
}

async fn repositories(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RepositoryQuery>,
) -> Result<Json<ApiResponse<Vec<crate::models::Repository>>>, AppError> {
    let snapshot = state.cache.snapshot().await;
    let updated_at = snapshot.updated_at.ok_or(AppError::CacheEmpty)?;
    let repositories = filter_and_sort(&snapshot.repositories, &query);
    Ok(Json(ApiResponse::new(repositories, updated_at)))
}

async fn languages(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<crate::models::LanguageStat>>>, AppError> {
    let snapshot = state.cache.snapshot().await;
    let updated_at = snapshot.updated_at.ok_or(AppError::CacheEmpty)?;
    Ok(Json(ApiResponse::new(snapshot.languages, updated_at)))
}

#[derive(Serialize)]
struct RefreshResponse {
    status: &'static str,
    updated_at: DateTime<Utc>,
}

async fn force_refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<RefreshResponse>, AppError> {
    let Some(expected) = &state.config.admin_token else {
        return Err(AppError::RefreshDisabled);
    };
    let provided = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok())
        .or_else(|| {
            headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
        });
    if !provided.is_some_and(|token| constant_time_eq(token, expected)) {
        return Err(AppError::Unauthorized);
    }

    let updated_at = refresh::refresh(&state).await?;
    Ok(Json(RefreshResponse {
        status: "ok",
        updated_at,
    }))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    cache_ready: bool,
    updated_at: Option<DateTime<Utc>>,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let snapshot = state.cache.snapshot().await;
    Json(HealthResponse {
        status: "ok",
        cache_ready: snapshot.updated_at.is_some(),
        updated_at: snapshot.updated_at,
    })
}

async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "Not Found",
            "message": "the requested resource does not exist"
        })),
    )
}

fn cors_layer(origins: &[String]) -> Result<CorsLayer, AppError> {
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            CONTENT_TYPE,
            AUTHORIZATION,
            HeaderName::from_static("x-admin-token"),
        ]);

    if origins.iter().any(|origin| origin == "*") {
        return Ok(layer.allow_origin(Any));
    }

    let values = origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .map_err(|error| AppError::Config(format!("invalid CORS origin {origin}: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(layer.allow_origin(values))
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let mut difference = left.len() ^ right.len();
    for (left_byte, right_byte) in left.bytes().zip(right.bytes()) {
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn token_comparison_checks_content_and_length() {
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secret", "secreu"));
        assert!(!constant_time_eq("secret", "secret-longer"));
    }
}
