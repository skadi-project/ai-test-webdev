//! GitHub REST API v3 client with conditional requests.

mod client;
pub mod error;

pub use client::{Conditional, GitHubClient, LinkHeader, RepoPage};
pub use error::GitHubError;
