//! JSON API handlers.

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;

use crate::error::AppError;
use crate::filters::{self, RepoQuery, Visibility};
use crate::models::{LanguageStat, Profile, Repo, Snapshot};
use crate::state::AppState;

/// `GET /api/profile`
pub async fn profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let snapshot = require_snapshot(&state)?;
    let body = ProfileResponse {
        profile: snapshot.profile.clone(),
        updated_at: Some(snapshot.updated_at),
        repo_count: snapshot.repos.len(),
        stale: is_stale(&state, snapshot.updated_at),
    };
    let etag = etag_for(
        "profile",
        &[
            &snapshot.updated_at.to_rfc3339(),
            &body.repo_count.to_string(),
            bool_flag(body.stale),
        ],
    );
    Ok(json(StatusCode::OK, &body, &etag, &headers))
}

/// `GET /api/repos`
pub async fn repos(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RepoQuery>,
) -> Result<Response, AppError> {
    let snapshot = require_snapshot(&state)?;
    let defaults = Visibility {
        show_forks: state.config.show_forks,
        show_archived: state.config.show_archived,
    };
    let filter = filters::Filter::from_query(&query, defaults);
    let selected: Vec<Repo> = filter.apply(&snapshot.repos, &state.config.pinned_repos);

    let repos_etag = etag_for(
        "repos",
        &[
            &snapshot.updated_at.to_rfc3339(),
            filter.search.as_deref().unwrap_or(""),
            filter.language.as_deref().unwrap_or(""),
            filter.topic.as_deref().unwrap_or(""),
            filter.sort.as_str(),
            bool_flag(is_stale(&state, snapshot.updated_at)),
            bool_flag(filter.visibility.show_forks),
            bool_flag(filter.visibility.show_archived),
        ],
    );

    let body = ReposResponse {
        languages: snapshot.aggregate_languages(),
        available_languages: filters::available_languages(&snapshot.repos, filter.visibility),
        available_topics: filters::available_topics(&snapshot.repos, filter.visibility),
        total: selected.len(),
        languages_complete: languages_complete(
            &snapshot,
            Visibility {
                show_forks: true,
                show_archived: true,
            },
        ),
        updated_at: Some(snapshot.updated_at),
        stale: is_stale(&state, snapshot.updated_at),
        defaults: DefaultsResponse {
            show_forks: defaults.show_forks,
            show_archived: defaults.show_archived,
        },
        repos: selected,
    };
    Ok(json(StatusCode::OK, &body, &repos_etag, &headers))
}

/// `POST /api/refresh` — protected by `ADMIN_TOKEN`.
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    authorize(&state, &headers)?;
    state.refresh_requested.notify_one();
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"status":"scheduled"})),
    )
        .into_response())
}

/// Fallback handler for unknown paths.
pub async fn not_found() -> Response {
    let body = serde_json::json!({
        "error": "not found",
        "code": "not_found",
        "hint": "available routes: /, /api/profile, /api/repos, /api/refresh, /api/health, /health"
    });
    (StatusCode::NOT_FOUND, Json(body)).into_response()
}

fn require_snapshot(state: &AppState) -> Result<std::sync::Arc<Snapshot>, AppError> {
    state.snapshot().ok_or(AppError::CacheCold(
        "waiting for the first successful GitHub fetch",
    ))
}

/// Compares the admin token in constant time.
fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let expected = state
        .config
        .admin_token
        .as_deref()
        .ok_or(AppError::Unauthorized)?;
    let provided = headers
        .get("x-admin-token")
        .or_else(|| headers.get(header::AUTHORIZATION))
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .unwrap_or(value)
                .trim()
        })
        .filter(|value| !value.is_empty())
        .ok_or(AppError::Unauthorized)?;
    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    bool::from(a.ct_eq(b))
}

/// Snapshot older than two refresh intervals is flagged as stale for the UI.
fn is_stale(state: &AppState, updated_at: DateTime<Utc>) -> bool {
    let interval = ChronoDuration::from_std(state.config.refresh_interval)
        .unwrap_or_else(|_| ChronoDuration::seconds(300));
    state.cache.failed() || Utc::now() - updated_at > interval * 2
}

/// True when every visible repository has cached language bytes.
fn languages_complete(snapshot: &Snapshot, visibility: Visibility) -> bool {
    snapshot
        .repos
        .iter()
        .filter(|repo| {
            (visibility.show_forks || !repo.fork) && (visibility.show_archived || !repo.archived)
        })
        .all(|repo| {
            snapshot
                .repo_languages
                .get(&repo.full_name)
                .map(|stats| !stats.is_stale_for(repo.updated_at.as_deref()))
                .unwrap_or(false)
        })
}

fn bool_flag(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

/// Stable validator derived from the snapshot timestamp and the query shape.
fn etag_for(namespace: &str, parts: &[&str]) -> String {
    format!("\"{namespace}-{:016x}\"", fnv1a_64(&parts.join("\u{1f}")))
}

fn fnv1a_64(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

/// Serializes `body`, sets the ETag, and answers `304` on a matching `If-None-Match`.
fn json<T: Serialize>(status: StatusCode, body: &T, etag: &str, headers: &HeaderMap) -> Response {
    let header_value = HeaderValue::from_str(etag).ok();
    if matches_etag(headers, etag) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        if let Some(value) = header_value.clone() {
            response.headers_mut().insert(header::ETAG, value);
        }
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return response;
    }
    let mut response = (status, Json(body)).into_response();
    if let Some(value) = header_value {
        response.headers_mut().insert(header::ETAG, value);
    }
    // The client polls, so let it revalidate cheaply instead of caching blindly.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// `If-None-Match` comparison supporting lists and `*`.
pub fn matches_etag(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == etag || candidate == "*")
        })
        .unwrap_or(false)
}

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub profile: Profile,
    pub updated_at: Option<DateTime<Utc>>,
    pub repo_count: usize,
    pub stale: bool,
}

#[derive(Debug, Serialize)]
pub struct ReposResponse {
    pub repos: Vec<Repo>,
    pub languages: Vec<LanguageStat>,
    pub available_languages: Vec<String>,
    pub available_topics: Vec<String>,
    pub total: usize,
    pub updated_at: Option<DateTime<Utc>>,
    pub stale: bool,
    pub languages_complete: bool,
    pub defaults: DefaultsResponse,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DefaultsResponse {
    pub show_forks: bool,
    pub show_archived: bool,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub status: &'static str,
    pub latency_ms: u64,
    pub repos: usize,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etag_is_stable_and_namespaced() {
        assert_eq!(
            etag_for("repos", &["a", "b"]),
            etag_for("repos", &["a", "b"])
        );
        assert_ne!(
            etag_for("repos", &["a", "b"]),
            etag_for("repos", &["a", "c"])
        );
        assert_ne!(etag_for("repos", &["a"]), etag_for("profile", &["a"]));
        assert!(etag_for("repos", &["a"]).starts_with('"'));
    }

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secre"));
        assert!(!constant_time_eq(b"", b"x"));
    }

    #[test]
    fn if_none_match_parsing() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("\"x\", \"y\""),
        );
        assert!(matches_etag(&headers, "\"y\""));
        assert!(!matches_etag(&headers, "\"z\""));
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        assert!(matches_etag(&headers, "\"anything\""));
        assert!(!matches_etag(&HeaderMap::new(), "\"anything\""));
    }
}
