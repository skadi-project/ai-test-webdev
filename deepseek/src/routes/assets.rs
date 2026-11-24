//! Static frontend assets embedded in the binary.
//!
//! Embedding (instead of serving a directory) means the Docker image needs
//! nothing but the executable, and there is no path-traversal surface.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

const INDEX_HTML: &str = include_str!("../../static/index.html");
const APP_CSS: &str = include_str!("../../static/app.css");
const APP_JS: &str = include_str!("../../static/app.js");
const FAVICON_SVG: &str = include_str!("../../static/favicon.svg");

const HTML_CACHE: &str = "no-cache";
const ASSET_CACHE: &str = "public, max-age=300";

/// `GET /`
pub async fn index() -> Response {
    html(INDEX_HTML)
}

/// `GET /app.css`
pub async fn css() -> Response {
    asset(APP_CSS, "text/css; charset=utf-8", ASSET_CACHE)
}

/// `GET /app.js`
pub async fn js() -> Response {
    asset(APP_JS, "text/javascript; charset=utf-8", ASSET_CACHE)
}

/// `GET /favicon.svg`
pub async fn favicon() -> Response {
    asset(FAVICON_SVG, "image/svg+xml", "public, max-age=86400")
}

/// `GET /robots.txt`
pub async fn robots() -> Response {
    asset(
        "User-agent: *\nAllow: /\nDisallow: /api/\n",
        "text/plain; charset=utf-8",
        "public, max-age=86400",
    )
}

fn html(body: &'static str) -> Response {
    asset(body, "text/html; charset=utf-8", HTML_CACHE)
}

fn asset(body: &'static str, content_type: &'static str, cache: &'static str) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(
        header::HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    response
}
