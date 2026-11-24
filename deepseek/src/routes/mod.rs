//! HTTP layer: router, guard layers and handlers.

pub mod api;
pub mod assets;
pub mod health;

use axum::http::{header, HeaderValue, Method};
use axum::routing::{get, post};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Builds the full application router.
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/profile", get(api::profile))
        .route("/repos", get(api::repos))
        .route("/refresh", post(api::refresh))
        .route("/health", get(health::health));

    let app = Router::new()
        .route("/", get(assets::index))
        .route("/index.html", get(assets::index))
        .route("/app.css", get(assets::css))
        .route("/app.js", get(assets::js))
        .route("/favicon.svg", get(assets::favicon))
        .route("/robots.txt", get(assets::robots))
        .nest("/api", api.clone())
        .route("/health", get(health::health))
        .fallback(api::not_found)
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        .layer(CompressionLayer::new())
        .layer(trace_layer())
        .layer(cors_layer(state.config.cors_allow_origin.as_deref()))
        .layer(security_headers())
        .with_state(state);

    app
}

/// Same-origin by default; an explicit `CORS_ALLOW_ORIGIN` opts into cross-origin reads.
fn cors_layer(allow_origin: Option<&str>) -> CorsLayer {
    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::IF_NONE_MATCH,
            header::AUTHORIZATION,
            header::HeaderName::from_static("x-admin-token"),
        ])
        .max_age(std::time::Duration::from_secs(600));

    match allow_origin.and_then(|origin| HeaderValue::from_str(origin).ok()) {
        Some(origin) => base.allow_origin(AllowOrigin::exact(origin)),
        None => base.allow_origin(AllowOrigin::list(Vec::new())),
    }
}

/// Baseline hardening headers. The page ships no inline JS, so `script-src 'self'`
/// is enough; inline *styles* stay allowed because the language chart and the
/// accent bar set colors dynamically.
fn security_headers() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' https: data:; \
             style-src 'self' 'unsafe-inline'; script-src 'self'; \
             connect-src 'self'; base-uri 'none'; form-action 'none'; \
             frame-ancestors 'none'; object-src 'none'",
        ),
    )
}

fn trace_layer(
) -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
        .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
        .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO))
}
