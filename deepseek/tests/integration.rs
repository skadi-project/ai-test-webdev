use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use github_portfolio::{
    config::Config,
    routes,
    state::{AppState, RefreshOutcome},
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Mock {
    fail: Arc<AtomicBool>,
    version: Arc<AtomicUsize>,
    conditional: Arc<AtomicUsize>,
}
async fn profile(State(s): State<Mock>, headers: HeaderMap) -> axum::response::Response {
    if s.fail.load(Ordering::SeqCst) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if headers.get("if-none-match").is_some() {
        s.conditional.fetch_add(1, Ordering::SeqCst);
        return StatusCode::NOT_MODIFIED.into_response();
    }
    (
        [("etag", "\"profile\"")],
        Json(
            serde_json::from_str::<serde_json::Value>(include_str!("fixtures/user.json"))
                .expect("fixture"),
        ),
    )
        .into_response()
}
async fn repos(State(s): State<Mock>, headers: HeaderMap) -> axum::response::Response {
    if s.fail.load(Ordering::SeqCst) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let version = s.version.load(Ordering::SeqCst);
    let etag = format!("\"repos-{version}\"");
    if headers.get("if-none-match").and_then(|h| h.to_str().ok()) == Some(&etag) {
        s.conditional.fetch_add(1, Ordering::SeqCst);
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let mut list: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/repos.json")).expect("fixture");
    if version > 0 {
        list.push(
            serde_json::json!({"id":999,"name":"new-project","full_name":"octocat/new-project"}),
        );
    }
    ([("etag", etag)], Json(list)).into_response()
}
async fn languages(headers: HeaderMap) -> axum::response::Response {
    if headers.get("if-none-match").is_some() {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    (
        [("etag", "\"languages\"")],
        Json(serde_json::json!({"Rust":10000,"HTML":500})),
    )
        .into_response()
}

#[tokio::test]
async fn cold_refresh_conditional_new_repo_and_failure_fallback() {
    let mock = Mock::default();
    let server = Router::new()
        .route("/users/octocat", get(profile))
        .route("/users/octocat/repos", get(repos))
        .route("/repos/octocat/{repo}/languages", get(languages))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let task = tokio::spawn(async move {
        axum::serve(listener, server).await.expect("server");
    });
    let cfg = Config::from_map(
        |k| match k {
            "GITHUB_API_BASE" => Some(base.clone()),
            "ADMIN_TOKEN" => Some("test-secret".into()),
            _ => None,
        },
        "octocat".into(),
    )
    .expect("config");
    let state = AppState::new(cfg).expect("state");
    let app = routes::router(state.clone());
    let request = |uri: &str| {
        Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request")
    };
    assert_eq!(
        app.clone()
            .oneshot(request("/api/repos"))
            .await
            .expect("response")
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        app.clone()
            .oneshot(request("/health"))
            .await
            .expect("response")
            .status(),
        StatusCode::OK
    );
    assert_eq!(state.refresh_now().await, RefreshOutcome::Updated);
    assert_eq!(state.snapshot().expect("cache").repos.len(), 5);
    assert_eq!(state.refresh_now().await, RefreshOutcome::NotModified);
    assert_eq!(mock.conditional.load(Ordering::SeqCst), 2);
    mock.version.store(1, Ordering::SeqCst);
    assert_eq!(state.refresh_now().await, RefreshOutcome::Updated);
    assert_eq!(state.snapshot().expect("cache").repos.len(), 6);
    mock.fail.store(true, Ordering::SeqCst);
    assert_eq!(state.refresh_now().await, RefreshOutcome::Failed);
    assert!(state.cache.failed());
    assert_eq!(state.snapshot().expect("retained").repos.len(), 6);
    let response = app
        .clone()
        .oneshot(request("/api/repos"))
        .await
        .expect("response");
    let etag = response.headers()["etag"].clone();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("body");
    let body: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(body["stale"], true);
    assert_eq!(body["total"], 4);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/repos")
                .header("if-none-match", etag)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    let request = Request::builder()
        .method("POST")
        .uri("/api/refresh")
        .body(Body::empty())
        .expect("request");
    assert_eq!(
        app.clone()
            .oneshot(request)
            .await
            .expect("response")
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/refresh")
        .header("x-admin-token", "test-secret")
        .body(Body::empty())
        .expect("request");
    assert_eq!(
        app.oneshot(request).await.expect("response").status(),
        StatusCode::ACCEPTED
    );
    task.abort();
}

#[tokio::test]
async fn second_page_is_checked_when_first_page_is_304() {
    let version = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let next = format!("<{base}/page2>; rel=\"next\"");
    let page1 = move |headers: HeaderMap| {
        let next = next.clone();
        async move {
            if headers.contains_key("if-none-match") {
                return StatusCode::NOT_MODIFIED.into_response();
            }
            (
                [("etag", "\"page1\"".to_owned()), ("link", next)],
                Json(serde_json::json!([{"id":1,"name":"first"}])),
            )
                .into_response()
        }
    };
    let v = version.clone();
    let page2 = move || {
        let v = v.clone();
        async move {
            let name = if v.load(Ordering::SeqCst) == 0 {
                "before"
            } else {
                "after"
            };
            Json(serde_json::json!([{"id":2,"name":name}]))
        }
    };
    let app = Router::new()
        .route("/users/octocat/repos", get(page1))
        .route("/page2", get(page2));
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let cfg = Config::from_map(
        |k| (k == "GITHUB_API_BASE").then(|| base.clone()),
        "octocat".into(),
    )
    .expect("config");
    let client = github_portfolio::github::GitHubClient::new(&cfg).expect("client");
    let (repos, pages) = client.repos(&Default::default()).await.expect("first");
    assert_eq!(repos[1].name, "before");
    version.store(1, Ordering::SeqCst);
    let (repos, _) = client.repos(&pages).await.expect("second");
    assert_eq!(repos[1].name, "after");
    task.abort();
}
