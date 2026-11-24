use std::{
    collections::{BTreeMap, HashMap},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::{fs, sync::RwLock};
use tracing::{info, warn};

use crate::{
    error::AppError,
    models::{LanguageMap, Repository, Snapshot},
};

#[derive(Clone)]
pub struct PortfolioCache {
    state: Arc<RwLock<CacheState>>,
    path: Option<Arc<PathBuf>>,
}

#[derive(Clone, Default)]
pub(crate) struct CacheState {
    pub snapshot: Snapshot,
    pub profile_etag: Option<String>,
    pub repository_pages: BTreeMap<u32, CachedPage>,
    pub languages: HashMap<String, CachedLanguages>,
}

#[derive(Clone)]
pub(crate) struct CachedPage {
    pub etag: Option<String>,
    pub repositories: Vec<Repository>,
}

#[derive(Clone)]
pub(crate) struct CachedLanguages {
    pub etag: Option<String>,
    pub bytes: LanguageMap,
}

impl PortfolioCache {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            state: Arc::new(RwLock::new(CacheState::default())),
            path: path.map(Arc::new),
        }
    }

    pub async fn restore(&self) -> Result<(), AppError> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        let bytes = match fs::read(path.as_ref()).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let snapshot = serde_json::from_slice::<Snapshot>(&bytes)?;
        let repository_count = snapshot.repositories.len();
        self.state.write().await.snapshot = snapshot;
        info!(
            path = %path.display(),
            repository_count,
            "restored portfolio cache from disk"
        );
        Ok(())
    }

    pub async fn snapshot(&self) -> Snapshot {
        self.state.read().await.snapshot.clone()
    }

    pub(crate) async fn working_copy(&self) -> CacheState {
        self.state.read().await.clone()
    }

    pub(crate) async fn commit(&self, state: CacheState) {
        let snapshot = state.snapshot.clone();
        *self.state.write().await = state;
        if let Some(path) = &self.path {
            if let Err(error) = save_snapshot(path, &snapshot).await {
                warn!(error = %error, "failed to persist portfolio cache");
            }
        }
    }
}

async fn save_snapshot(path: &Path, snapshot: &Snapshot) -> Result<(), AppError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).await?;
    }

    let bytes = serde_json::to_vec_pretty(snapshot)?;
    let temporary_path = path.with_extension("tmp");
    fs::write(&temporary_path, bytes).await?;

    #[cfg(windows)]
    if fs::metadata(path).await.is_ok() {
        fs::remove_file(path).await?;
    }

    fs::rename(&temporary_path, path).await?;
    Ok(())
}
