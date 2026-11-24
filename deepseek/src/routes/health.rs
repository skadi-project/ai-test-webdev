//! Liveness and readiness reporting.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

/// `GET /health` and `GET /api/health`.
///
/// Always `200` while the process can answer: the container health check must
/// not restart a perfectly healthy site just because GitHub is briefly down.
/// The payload tells monitoring *why* the data may be missing.
pub async fn health(State(state): State<AppState>) -> Response {
    let updated_at = state.cache.updated_at();
    let has_snapshot = updated_at.is_some();
    let uptime = state.started_at.elapsed().as_secs();

    let body = HealthResponse {
        status: "ok",
        ready: has_snapshot,
        cached: has_snapshot,
        updated_at,
        uptime_secs: uptime,
        refresh_interval_secs: state.config.refresh_interval.as_secs(),
        user: state.config.github_username.clone(),
        version: env!("CARGO_PKG_VERSION"),
    };

    let mut response = (StatusCode::OK, Json(body)).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub ready: bool,
    pub cached: bool,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub uptime_secs: u64,
    pub refresh_interval_secs: u64,
    pub user: String,
    pub version: &'static str,
}
