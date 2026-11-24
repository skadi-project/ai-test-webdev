//! Unified error type for the application and its HTTP representation.

use std::fmt;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

use crate::github::GitHubError;

/// Errors surfaced by the HTTP layer.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("GitHub API error: {0}")]
    GitHub(#[from] GitHubError),

    #[error("cache is not populated yet: {0}")]
    CacheCold(&'static str),

    #[error("unauthorized")]
    Unauthorized,

    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Machine-readable error code used by the JSON body.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Config(_) => "config_error",
            AppError::GitHub(err) => err.code(),
            AppError::CacheCold(_) => "cache_cold",
            AppError::Unauthorized => "unauthorized",
            AppError::NotFound => "not_found",
            AppError::BadRequest(_) => "bad_request",
            AppError::Io(_) | AppError::Serde(_) | AppError::Internal(_) => "internal_error",
        }
    }

    /// HTTP status code matching the error.
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::Config(_) | AppError::Internal(_) | AppError::Io(_) | AppError::Serde(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            AppError::GitHub(GitHubError::RateLimited { .. }) => StatusCode::TOO_MANY_REQUESTS,
            AppError::GitHub(_) => StatusCode::BAD_GATEWAY,
            AppError::CacheCold(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        // Client mistakes are expected noise; server-side ones deserve a full trace.
        if status.is_server_error() {
            tracing::error!(error = %self, code = self.code(), "request failed");
        } else {
            tracing::warn!(error = %self, code = self.code(), "request rejected");
        }
        let body = ApiErrorBody {
            error: self.to_string(),
            code: self.code(),
        };
        (status, Json(body)).into_response()
    }
}

/// Uniform JSON error payload.
#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: String,
    pub code: &'static str,
}

impl fmt::Display for ApiErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.error, self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_are_mapped() {
        assert_eq!(AppError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            AppError::CacheCold("nope").status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            AppError::GitHub(GitHubError::RateLimited {
                reset_at: None,
                remaining: 0
            })
            .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            AppError::GitHub(GitHubError::NotFound).status(),
            StatusCode::BAD_GATEWAY
        );
    }
}
