use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::{Notify, RwLock};

use crate::{
    config::Config,
    filter,
    github::{Fetch, GitHubClient},
    models::{Profile, Repo},
};

/// Per-repository language breakdown plus the ETag of its last fresh fetch.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RepoLangs {
    pub bytes: BTreeMap<String, u64>,
    pub etag: Option<String>,
}

/// Everything served by the site, persisted to disk between restarts.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CacheData {
    pub profile: Option<Profile>,
    pub repos: Vec<Repo>,
    pub repo_langs: HashMap<String, RepoLangs>,
    pub etags: Etags,
    pub updated_at: Option<String>,
}

/// ETags for the conditional requests of each API resource.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Etags {
    pub profile: Option<String>,
    pub repos: Option<String>,
}

/// Shared application state handed to every handler.
pub struct AppInner {
    pub config: Config,
    pub github: GitHubClient,
    pub cache: RwLock<CacheData>,
    pub refresh_notify: Notify,
}

pub type AppState = Arc<AppInner>;

impl AppInner {
    /// Create the state from configuration and load the on-disk snapshot, if any.
    ///
    /// # Errors
    /// Returns an error if the GitHub client cannot be built.
    pub fn new(config: Config) -> Result<Self> {
        let github = GitHubClient::new(config.github_token.as_deref())?;
        let cache = RwLock::new(load_snapshot(&config).unwrap_or_default());
        Ok(Self {
            config,
            github,
            cache,
            refresh_notify: Notify::new(),
        })
    }

    /// Write the current snapshot to disk so a restart serves data instantly.
    ///
    /// # Errors
    /// Returns an error if the cache directory cannot be created or the
    /// snapshot cannot be serialized or written.
    pub async fn persist(&self, data: &CacheData) -> Result<()> {
        let path = self.config.cache_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(data).context("serialize cache snapshot")?;
        std::fs::write(&tmp, bytes)
            .with_context(|| format!("write {}", tmp.display()))
            .and_then(|()| {
                std::fs::rename(&tmp, &path)
                    .with_context(|| format!("rename to {}", path.display()))
            })
    }
}

fn load_snapshot(config: &Config) -> Option<CacheData> {
    let raw = std::fs::read(config.cache_file()).ok()?;
    match serde_json::from_slice::<CacheData>(&raw) {
        Ok(data) => {
            tracing::info!(
                path = %config.cache_file().display(),
                repos = data.repos.len(),
                "loaded cached snapshot from disk"
            );
            Some(data)
        }
        Err(e) => {
            tracing::warn!(error = %e, "cache snapshot is corrupt, ignoring it");
            None
        }
    }
}

/// Run one full refresh cycle: profile, repositories, per-repo languages.
///
/// On any top-level error the previously cached data stays untouched.
///
/// # Errors
/// Propagates profile/repo fetch failures; single-repo language failures are
/// logged and skipped so one broken repository cannot void a whole refresh.
pub async fn refresh_once(inner: &AppInner) -> Result<()> {
    let username = &inner.config.github_username;

    let (profile_etag, repos_etag) = {
        let guard = inner.cache.read().await;
        (guard.etags.profile.clone(), guard.etags.repos.clone())
    };

    // --- Profile -----------------------------------------------------------
    let profile_path = format!("/users/{username}");
    match inner
        .github
        .get_json::<Profile>(&profile_path, profile_etag.as_deref())
        .await?
    {
        Fetch::Fresh(profile, etag) => {
            let mut guard = inner.cache.write().await;
            guard.profile = Some(profile);
            guard.etags.profile = etag;
        }
        Fetch::NotModified => tracing::debug!(username, "profile not modified"),
    }

    // --- Repositories (paginated, first page conditional) --------------------
    let first_page = format!("/users/{username}/repos?per_page=100&sort=updated&page=1");
    match inner
        .github
        .get_json::<Vec<Repo>>(&first_page, repos_etag.as_deref())
        .await?
    {
        Fetch::NotModified => tracing::debug!(username, "repo list not modified"),
        Fetch::Fresh(first, etag) => {
            let mut all = first;
            let mut page = 2u32;
            // GitHub sorts by `updated`, so a 304 on page 1 means the whole
            // list is unchanged; otherwise keep pulling full pages.
            while all.len() == (page as usize - 1) * 100 && page <= 10 {
                let path = format!("/users/{username}/repos?per_page=100&sort=updated&page={page}");
                match inner.github.get_json::<Vec<Repo>>(&path, None).await? {
                    Fetch::Fresh(mut more, _) => {
                        let empty = more.is_empty();
                        all.append(&mut more);
                        if empty {
                            break;
                        }
                    }
                    Fetch::NotModified => break,
                }
                page += 1;
            }
            let mut guard = inner.cache.write().await;
            guard.repos = all;
            guard.etags.repos = etag;
        }
    }

    // --- Per-repository languages (conditional, failures are non-fatal) -----
    let candidates: Vec<Repo> = {
        let guard = inner.cache.read().await;
        guard
            .repos
            .iter()
            .filter(|r| {
                !inner
                    .config
                    .excluded
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(&r.name))
            })
            .filter(|r| inner.config.show_forks || !r.fork)
            .filter(|r| inner.config.show_archived || !r.archived)
            .cloned()
            .collect()
    };

    for repo in &candidates {
        refresh_repo_langs(inner, repo).await;
    }

    // --- Finalize ------------------------------------------------------------
    let snapshot = {
        let mut guard = inner.cache.write().await;
        let live: std::collections::HashSet<String> =
            candidates.iter().map(|r| r.full_name.clone()).collect();
        guard.repo_langs.retain(|key, _| live.contains(key));
        guard.updated_at = Some(Utc::now().to_rfc3339());
        guard.clone()
    };

    if let Err(e) = inner.persist(&snapshot).await {
        tracing::warn!(error = format!("{e:#}"), "failed to persist cache snapshot");
    }
    tracing::info!(repos = snapshot.repos.len(), "refresh completed");
    Ok(())
}

async fn refresh_repo_langs(inner: &AppInner, repo: &Repo) {
    let etag = {
        let guard = inner.cache.read().await;
        guard
            .repo_langs
            .get(&repo.full_name)
            .and_then(|l| l.etag.clone())
    };
    let path = format!("/repos/{}/languages", repo.full_name);
    match inner
        .github
        .get_json::<BTreeMap<String, u64>>(&path, etag.as_deref())
        .await
    {
        Ok(Fetch::Fresh(bytes, etag)) => {
            let mut guard = inner.cache.write().await;
            guard
                .repo_langs
                .insert(repo.full_name.clone(), RepoLangs { bytes, etag });
        }
        Ok(Fetch::NotModified) => {}
        Err(e) => {
            tracing::warn!(repo = %repo.full_name, error = format!("{e:#}"), "language fetch failed")
        }
    }
}

/// Background task: refresh immediately, then every `REFRESH_INTERVAL_SECS`
/// or whenever `/api/refresh` signals the [`Notify`].
pub async fn refresh_loop(state: AppState) {
    let interval = state.config.refresh_interval;
    loop {
        if let Err(e) = refresh_once(&state).await {
            tracing::error!(
                error = format!("{e:#}"),
                "refresh failed, serving last known good cache"
            );
        }
        tokio::select! {
            () = state.refresh_notify.notified() => {
                tracing::info!("manual refresh requested");
            }
            () = tokio::time::sleep(interval) => {}
        }
    }
}

/// Aggregate language bytes across all cached, visible repositories.
pub async fn language_stats(state: &AppState) -> (BTreeMap<String, u64>, u64) {
    let guard = state.cache.read().await;
    let repos = filter::view_repos(&guard.repos, &state.config);
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut total_bytes = 0u64;
    for repo in repos {
        if let Some(langs) = guard.repo_langs.get(&repo.full_name) {
            for (lang, bytes) in &langs.bytes {
                *totals.entry(lang.clone()).or_insert(0) += *bytes;
                total_bytes += *bytes;
            }
        }
    }
    (totals, total_bytes)
}
