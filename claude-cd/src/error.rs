use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("github error: {0}")]
    Github(String),
    #[error("cache error: {0}")]
    Cache(String),
    #[error("template error: {0}")]
    Template(String),
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Github(_) | Self::Cache(_) | Self::Template(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (
            status,
            axum::Json(serde_json::json!({"error": self.to_string()})),
        )
            .into_response()
    }
}
