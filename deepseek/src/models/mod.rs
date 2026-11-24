//! Domain models.

pub mod github;
pub mod snapshot;

pub use github::{GitHubOwner, GitHubRepo, GitHubUser, LanguageBytes};
pub use snapshot::{LanguageStat, Profile, Repo, RepoLanguages, Snapshot};
