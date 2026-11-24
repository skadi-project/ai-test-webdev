use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("GitHub request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("GitHub API returned {status}: {message}")]
    GitHub { status: StatusCode, message: String },
    #[error("GitHub API rate limit exceeded (reset: {reset:?})")]
    RateLimited { reset: Option<String> },
    #[error("invalid GitHub API URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cache is not ready yet")]
    CacheEmpty,
    #[error("cache error: {0}")]
    Cache(String),
    #[error("POST /api/refresh is disabled because ADMIN_TOKEN is not configured")]
    RefreshDisabled,
    #[error("a valid X-Admin-Token or Authorization: Bearer token is required")]
    Unauthorized,
    #[error("invalid server address: {0}")]
    Address(#[from] std::net::AddrParseError),
    #[error("background task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::CacheEmpty | Self::RefreshDisabled | Self::RateLimited { .. } => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Request(_) | Self::GitHub { .. } | Self::Url(_) => StatusCode::BAD_GATEWAY,
            Self::Config(_)
            | Self::Cache(_)
            | Self::Io(_)
            | Self::Json(_)
            | Self::Address(_)
            | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = ErrorBody {
            error: status.canonical_reason().unwrap_or("Error"),
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}
