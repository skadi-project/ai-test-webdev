//! Refreshes are transactional for profile/repositories, incremental for languages.
use crate::{
    cache::Cache,
    config::Config,
    github::{GitHubClient, GitHubError},
    models::{GitHubRepo, Profile, Repo, RepoLanguages, Snapshot},
    state::RefreshOutcome,
};
use chrono::Utc;
use std::collections::BTreeMap;
use tokio::sync::Mutex;

pub async fn refresh(
    config: &Config,
    github: &GitHubClient,
    cache: &Cache,
    gate: &Mutex<()>,
) -> RefreshOutcome {
    // Coalesce overlapping timer/manual refreshes instead of queueing requests.
    let Ok(_guard) = gate.try_lock() else {
        return RefreshOutcome::Busy;
    };
    match refresh_locked(config, github, cache).await {
        Ok(outcome) => {
            cache.set_failed(false);
            outcome
        }
        Err(error) => {
            cache.set_failed(true);
            tracing::error!(%error, "refresh failed; retaining last successful snapshot");
            RefreshOutcome::Failed
        }
    }
}
async fn refresh_locked(
    config: &Config,
    github: &GitHubClient,
    cache: &Cache,
) -> Result<RefreshOutcome, GitHubError> {
    let previous = cache.snapshot();
    let result = github
        .user(previous.as_ref().and_then(|s| s.etag_profile.as_deref()))
        .await?;
    let profile = match result.value {
        Some(user) => Profile::from_github(user),
        None => previous
            .as_ref()
            .map(|s| s.profile.clone())
            .ok_or_else(|| GitHubError::Decode("304 without profile".into()))?,
    };
    let empty = BTreeMap::new();
    let (source_repos, repo_pages) = github
        .repos(previous.as_ref().map(|s| &s.repo_pages).unwrap_or(&empty))
        .await?;
    let repos = normalize_repos(source_repos.clone(), config);
    let mut repo_languages = previous
        .as_ref()
        .map(|s| s.repo_languages.clone())
        .unwrap_or_default();
    repo_languages.retain(|name, _| repos.iter().any(|r| &r.full_name == name));
    // Missing data first, then changed repos. Empty language maps are valid data.
    let mut pending: Vec<_> = repos
        .iter()
        .filter(|r| {
            repo_languages
                .get(&r.full_name)
                .is_none_or(|l| l.is_stale_for(r.updated_at.as_deref()))
        })
        .collect();
    pending.sort_by_key(|r| repo_languages.contains_key(&r.full_name));
    for repo in pending.into_iter().take(config.language_budget) {
        let old = repo_languages.get(&repo.full_name);
        match github
            .languages(&repo.full_name, old.and_then(|l| l.etag.as_deref()))
            .await
        {
            Ok(response) => {
                if let Some(bytes) = response.value {
                    repo_languages.insert(
                        repo.full_name.clone(),
                        RepoLanguages::from_bytes(bytes, response.etag, repo.updated_at.clone()),
                    );
                } else if let Some(stats) = repo_languages.get_mut(&repo.full_name) {
                    stats.etag = response.etag;
                    stats.repo_updated_at = repo.updated_at.clone();
                }
            }
            Err(error) => {
                tracing::warn!(repo = %repo.full_name, %error, "language fetch failed; retaining cached breakdown");
                // Stop this batch on any failure to avoid hammering upstream.
                break;
            }
        }
    }
    let unchanged = previous.as_ref().is_some_and(|s| {
        s.profile == profile && s.repos == repos && s.repo_languages == repo_languages
    });
    cache
        .store(Snapshot {
            profile,
            repos,
            source_repos,
            repo_pages,
            repo_languages,
            etag_profile: result.etag,
            etag_repos: None,
            updated_at: Utc::now(),
        })
        .await;
    Ok(if unchanged {
        RefreshOutcome::NotModified
    } else {
        RefreshOutcome::Updated
    })
}
pub fn normalize_repos(raw: Vec<GitHubRepo>, config: &Config) -> Vec<Repo> {
    raw.into_iter()
        .filter(|r| !matches_repo(&config.excluded_repos, &r.name, &r.full_name))
        .map(|r| {
            let pinned = matches_repo(&config.pinned_repos, &r.name, &r.full_name);
            Repo::from_github(r, pinned)
        })
        .collect()
}
fn matches_repo(list: &[String], name: &str, full_name: &str) -> bool {
    list.iter()
        .any(|item| item.eq_ignore_ascii_case(name) || item.eq_ignore_ascii_case(full_name))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_pins_and_exclusions() {
        let raw =
            serde_json::from_str(include_str!("../tests/fixtures/repos.json")).expect("fixture");
        let cfg = Config::from_map(
            |key| match key {
                "PINNED_REPOS" => Some("octocat/espresso".into()),
                "EXCLUDED_REPOS" => Some("Hello-World".into()),
                _ => None,
            },
            "octocat".into(),
        )
        .expect("config");
        let repos = normalize_repos(raw, &cfg);
        assert_eq!(repos.len(), 4);
        assert!(repos.iter().any(|r| r.name == "espresso" && r.pinned));
        assert!(repos.iter().all(|r| r.name != "Hello-World"));
    }
}
