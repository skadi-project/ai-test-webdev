use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Instant,
};

use chrono::{DateTime, Utc};
use futures::{stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{
    cache::{CacheState, CachedLanguages, CachedPage},
    error::AppError,
    github::{FetchResult, REPOSITORIES_PER_PAGE},
    models::{LanguageMap, Repository},
    repository::{aggregate_languages, apply_preferences, is_excluded},
    state::AppState,
};

const MAX_REPOSITORY_PAGES: u32 = 100;
const LANGUAGE_CONCURRENCY: usize = 6;

pub async fn refresh(state: &AppState) -> Result<DateTime<Utc>, AppError> {
    let _guard = state.refresh_lock.lock().await;
    let started = Instant::now();
    let mut working = state.cache.working_copy().await;

    match state
        .github
        .fetch_profile(working.profile_etag.as_deref())
        .await?
    {
        FetchResult::Modified { data, etag } => {
            working.snapshot.profile = Some(data);
            working.profile_etag = etag;
        }
        FetchResult::NotModified if working.snapshot.profile.is_some() => {}
        FetchResult::NotModified => {
            return Err(AppError::Cache(
                "GitHub returned 304 for a profile missing from the cache".into(),
            ));
        }
    }

    let repositories = fetch_all_repositories(state, &mut working).await?;
    working.snapshot.repositories = apply_preferences(
        repositories.clone(),
        &state.config.pinned_repos,
        &state.config.excluded_repos,
        state.config.show_forks,
        state.config.show_archived,
    );

    refresh_languages(state, &repositories, &mut working).await;
    working.snapshot.updated_at = Some(Utc::now());
    let updated_at = working.snapshot.updated_at.unwrap_or_else(Utc::now);
    let repository_count = working.snapshot.repositories.len();
    state.cache.commit(working).await;

    info!(
        repository_count,
        elapsed_ms = started.elapsed().as_millis(),
        "portfolio cache refreshed"
    );
    Ok(updated_at)
}

async fn fetch_all_repositories(
    state: &AppState,
    working: &mut CacheState,
) -> Result<Vec<Repository>, AppError> {
    let mut pages = BTreeMap::new();
    let mut repositories = Vec::new();

    for page_number in 1..=MAX_REPOSITORY_PAGES {
        let previous = working.repository_pages.get(&page_number).cloned();
        let result = state
            .github
            .fetch_repositories_page(
                page_number,
                previous.as_ref().and_then(|page| page.etag.as_deref()),
            )
            .await?;

        let page = match result {
            FetchResult::Modified { data, etag } => CachedPage {
                etag,
                repositories: data,
            },
            FetchResult::NotModified => previous.ok_or_else(|| {
                AppError::Cache(format!(
                    "GitHub returned 304 for repository page {page_number} missing from the cache"
                ))
            })?,
        };
        let is_last_page = page.repositories.len() < REPOSITORIES_PER_PAGE;
        repositories.extend(page.repositories.iter().cloned());
        pages.insert(page_number, page);

        if is_last_page {
            working.repository_pages = pages;
            return Ok(repositories);
        }
    }

    Err(AppError::Cache(format!(
        "repository pagination exceeded {MAX_REPOSITORY_PAGES} pages"
    )))
}

async fn refresh_languages(
    state: &AppState,
    repositories: &[Repository],
    working: &mut CacheState,
) {
    let included: HashSet<String> = repositories
        .iter()
        .filter(|repository| !is_excluded(repository, &state.config.excluded_repos))
        .map(|repository| repository.name.clone())
        .collect();
    working
        .languages
        .retain(|repository, _| included.contains(repository));

    let requests: Vec<_> = included
        .iter()
        .map(|repository| {
            let etag = working
                .languages
                .get(repository)
                .and_then(|cached| cached.etag.clone());
            (repository.clone(), etag)
        })
        .collect();

    let results = stream::iter(requests.into_iter().map(|(repository, etag)| {
        let github = &state.github;
        async move {
            let result = github.fetch_languages(&repository, etag.as_deref()).await;
            (repository, result)
        }
    }))
    .buffer_unordered(LANGUAGE_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    for (repository, result) in results {
        match result {
            Ok(FetchResult::Modified { data, etag }) => {
                working
                    .languages
                    .insert(repository, CachedLanguages { etag, bytes: data });
            }
            Ok(FetchResult::NotModified) => {}
            Err(error) => {
                warn!(
                    repository,
                    error = %error,
                    "failed to refresh language statistics; keeping the previous value"
                );
            }
        }
    }

    let maps: Vec<LanguageMap> = included
        .iter()
        .filter_map(|repository| working.languages.get(repository))
        .map(|cached| cached.bytes.clone())
        .collect();
    working.snapshot.languages = aggregate_languages(maps);
}

pub async fn refresh_loop(state: Arc<AppState>, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            result = refresh(&state) => {
                if let Err(error) = result {
                    error!(error = %error, "portfolio refresh failed; serving the last successful cache");
                }
            }
        }

        tokio::select! {
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(state.config.refresh_interval) => {}
        }
    }
    info!("background refresh task stopped");
}
