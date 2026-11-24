use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Single application error type: a wrapper around `anyhow::Error` that
/// converts into a JSON 500 response.
#[derive(Debug, Error)]
#[error("{0:#}")]
pub struct AppError(#[from] anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let message = self.0.to_string();
        tracing::error!(error = ?self.0, "request handler failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": message })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_chained_anyhow_errors() {
        let err: AppError = anyhow::anyhow!("root")
            .context("middle")
            .context("outer")
            .into();
        assert_eq!(err.to_string(), "outer: middle: root");
    }
}
