//! Error type for GitHub REST API access.

use thiserror::Error;

/// Everything that can go wrong while talking to GitHub.
#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("resource not found on GitHub")]
    NotFound,

    #[error("GitHub rate limit exceeded (remaining={remaining}, reset_at={reset_at:?})")]
    RateLimited {
        reset_at: Option<String>,
        remaining: u32,
    },

    #[error("GitHub rejected the credentials (status {status})")]
    Unauthorized { status: u16 },

    #[error("unexpected GitHub status {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },

    #[error("failed to decode GitHub payload: {0}")]
    Decode(String),

    #[error("network error while calling GitHub: {0}")]
    Network(String),
}

impl GitHubError {
    /// Stable code for API responses and logs.
    pub fn code(&self) -> &'static str {
        match self {
            GitHubError::NotFound => "github_not_found",
            GitHubError::RateLimited { .. } => "github_rate_limited",
            GitHubError::Unauthorized { .. } => "github_unauthorized",
            GitHubError::UnexpectedStatus { .. } => "github_unexpected_status",
            GitHubError::Decode(_) => "github_decode_error",
            GitHubError::Network(_) => "github_network_error",
        }
    }

    /// True when retrying later may succeed without changing the request.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            GitHubError::Network(_) | GitHubError::RateLimited { .. }
        )
    }
}
