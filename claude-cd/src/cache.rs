use crate::{
    error::AppError,
    models::{LanguageStat, Repo, Snapshot},
};
use chrono::Utc;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Cache {
    inner: Arc<RwLock<Snapshot>>,
    path: PathBuf,
}
impl Cache {
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Cache(e.to_string()))?;
        }
        let snapshot = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Snapshot::default(),
        };
        Ok(Self {
            inner: Arc::new(RwLock::new(snapshot)),
            path,
        })
    }
    pub async fn get(&self) -> Snapshot {
        self.inner.read().await.clone()
    }
    pub async fn replace(&self, mut value: Snapshot) -> Result<(), AppError> {
        value.fetched_at = Some(Utc::now());
        let bytes =
            serde_json::to_vec_pretty(&value).map_err(|e| AppError::Cache(e.to_string()))?;
        let temp = self.path.with_extension("json.tmp");
        tokio::fs::write(&temp, bytes)
            .await
            .map_err(|e| AppError::Cache(e.to_string()))?;
        tokio::fs::rename(&temp, &self.path)
            .await
            .map_err(|e| AppError::Cache(e.to_string()))?;
        *self.inner.write().await = value;
        Ok(())
    }
}
pub fn language_stats(repos: &[Repo]) -> Vec<LanguageStat> {
    let mut totals: HashMap<String, u64> = HashMap::new();
    for repo in repos {
        for (name, bytes) in &repo.languages {
            *totals.entry(name.clone()).or_default() += bytes;
        }
    }
    let total: u64 = totals.values().sum();
    let mut out: Vec<_> = totals
        .into_iter()
        .map(|(name, bytes)| LanguageStat {
            name,
            bytes,
            percentage: if total == 0 {
                0.0
            } else {
                bytes as f64 * 100.0 / total as f64
            },
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.bytes));
    out
}
