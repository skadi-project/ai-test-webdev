//! In-memory snapshot cache with optional JSON persistence.
//!
//! The cache is the single source of truth for request handlers. Disk persistence
//! is best-effort: failing to write a snapshot never fails a refresh, and a
//! corrupt file on startup is ignored with a warning so the site still boots.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::models::Snapshot;

/// Snapshot plus metadata about the last successful refresh.
#[derive(Debug, Clone)]
pub struct Cached {
    pub snapshot: Arc<Snapshot>,
    /// When the snapshot was produced (equals `snapshot.updated_at`).
    pub refreshed_at: DateTime<Utc>,
}

/// Thread-safe cache used by handlers and the refresher.
#[derive(Debug, Clone)]
pub struct Cache {
    inner: Arc<RwLock<Option<Cached>>>,
    failed: Arc<std::sync::atomic::AtomicBool>,
    path: Option<PathBuf>,
    /// Serializes disk writes so concurrent refreshes cannot interleave.
    write_lock: Arc<Mutex<()>>,
}

impl Cache {
    /// Creates a cache, loading the persisted snapshot if `path` exists.
    pub async fn new(path: Option<PathBuf>) -> Self {
        let cache = Self {
            inner: Arc::new(RwLock::new(None)),
            failed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            path,
            write_lock: Arc::new(Mutex::new(())),
        };
        cache.load_from_disk().await;
        cache
    }

    pub fn empty(path: Option<PathBuf>) -> Self {
        Self {
            path,
            ..Self::memory_only()
        }
    }

    /// In-memory-only cache (used by tests).
    pub fn memory_only() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            failed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            path: None,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn set_failed(&self, failed: bool) {
        self.failed
            .store(failed, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn failed(&self) -> bool {
        self.failed.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the current snapshot, if any.
    pub fn get(&self) -> Option<Cached> {
        read_lock(&self.inner).as_ref().cloned()
    }

    /// Current snapshot regardless of freshness.
    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.get().map(|cached| cached.snapshot)
    }

    pub fn updated_at(&self) -> Option<DateTime<Utc>> {
        self.get().map(|cached| cached.refreshed_at)
    }

    pub fn is_empty(&self) -> bool {
        self.get().is_none()
    }

    /// Replaces the snapshot and (optionally) persists it. Returns the stored value.
    pub async fn store(&self, snapshot: Snapshot) -> Arc<Snapshot> {
        let refreshed_at = snapshot.updated_at;
        let shared = Arc::new(snapshot);
        *write_lock(&self.inner) = Some(Cached {
            snapshot: Arc::clone(&shared),
            refreshed_at,
        });
        if let Some(path) = &self.path {
            if let Err(err) = self.persist(path, &shared).await {
                tracing::warn!(error = %err, path = %path.display(), "failed to persist cache snapshot");
            }
        }
        shared
    }

    /// Replaces the snapshot without touching the disk (used when only ETags change).
    pub fn store_memory(&self, snapshot: Snapshot) -> Arc<Snapshot> {
        let refreshed_at = snapshot.updated_at;
        let shared = Arc::new(snapshot);
        *write_lock(&self.inner) = Some(Cached {
            snapshot: shared.clone(),
            refreshed_at,
        });
        shared
    }

    async fn persist(&self, path: &Path, snapshot: &Snapshot) -> std::io::Result<()> {
        let _guard = self.write_lock.lock().await;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let serialized = serde_json::to_vec(snapshot)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        // Write-then-rename keeps readers from ever seeing a half-written file.
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &serialized).await?;
        match tokio::fs::rename(&tmp, path).await {
            Ok(()) => Ok(()),
            Err(err) => {
                let _ = tokio::fs::remove_file(&tmp).await;
                Err(err)
            }
        }
    }

    async fn load_from_disk(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let bytes = match tokio::fs::read(path).await {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %path.display(), "no persisted cache found yet");
                return;
            }
            Err(err) => {
                tracing::warn!(error = %err, path = %path.display(), "cannot read cache file");
                return;
            }
        };
        match serde_json::from_slice::<Snapshot>(&bytes) {
            Ok(snapshot) => {
                tracing::info!(
                    path = %path.display(),
                    repos = snapshot.repos.len(),
                    updated_at = %snapshot.updated_at,
                    "restored snapshot from disk"
                );
                self.store_memory(snapshot);
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    path = %path.display(),
                    "cache file is corrupt, ignoring it"
                );
            }
        }
    }
}

/// Poison-tolerant read guard: a panic elsewhere must not take the site down.
fn read_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!("cache read lock was poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

fn write_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!("cache write lock was poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

/// Persisted envelope, kept for forward compatibility of the file format.
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheFile {
    pub version: u32,
    pub snapshot: Snapshot,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Profile, Repo};

    fn snapshot() -> Snapshot {
        Snapshot {
            profile: Profile {
                login: "octocat".into(),
                ..Default::default()
            },
            repos: vec![Repo {
                id: 1,
                name: "demo".into(),
                full_name: "octocat/demo".into(),
                description: None,
                html_url: "https://github.com/octocat/demo".into(),
                homepage: None,
                language: Some("Rust".into()),
                stargazers_count: 3,
                forks_count: 1,
                topics: vec![],
                updated_at: None,
                fork: false,
                archived: false,
                pinned: false,
            }],
            repo_pages: Default::default(),
            source_repos: Vec::new(),
            repo_languages: Default::default(),
            etag_profile: Some("\"p\"".into()),
            etag_repos: Some("\"r\"".into()),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn memory_cache_roundtrip() {
        let cache = Cache::memory_only();
        assert!(cache.is_empty());
        let stored = cache.store(snapshot()).await;
        assert_eq!(stored.repos.len(), 1);
        let read_back = cache.snapshot().expect("snapshot");
        assert_eq!(read_back.profile.login, "octocat");
        assert!(cache.updated_at().is_some());
    }

    #[tokio::test]
    async fn persists_and_restores() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested/cache.json");

        let cache = Cache::new(Some(path.clone())).await;
        assert!(cache.is_empty());
        cache.store(snapshot()).await;
        assert!(path.exists());

        let restored = Cache::new(Some(path)).await;
        let snapshot = restored.snapshot().expect("restored snapshot");
        assert_eq!(snapshot.repos[0].name, "demo");
        assert_eq!(snapshot.etag_repos.as_deref(), Some("\"r\""));
    }

    #[tokio::test]
    async fn corrupt_file_is_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cache.json");
        tokio::fs::write(&path, b"{not json").await.expect("write");
        let cache = Cache::new(Some(path)).await;
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn store_memory_does_not_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cache.json");
        let cache = Cache::new(Some(path.clone())).await;
        cache.store_memory(snapshot());
        assert!(!path.exists());
        assert!(cache.snapshot().is_some());
    }
}
