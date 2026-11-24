# GitHub Portfolio

Rust/Axum SSR portfolio with GitHub integration, disk JSON cache and accessible progressive-enhancement frontend.

## Run
```bash
cp .env.example .env
cargo run
```

Endpoints: `GET /`, `/health`, `/api/profile`, `/api/repos`, `/api/languages`, and `POST /api/refresh` with `ADMIN_TOKEN`. JSON responses provide ETag/304 support. GitHub endpoint ETags are retained in memory and sent as If-None-Match. Cache is persisted to `CACHE_FILE`; refresh defaults to 300 seconds. Required env: `GITHUB_USERNAME`, `GITHUB_TOKEN`. Optional filtering env: `SHOW_FORKS`, `ARCHIVED`, `PINNED`, `EXCLUDED`. Repositories support q/language/topic/sort filters; the frontend displays language statistics.

## Docker
```bash
docker compose up --build
```
The image runs as an unprivileged user and listens on port 8080.
