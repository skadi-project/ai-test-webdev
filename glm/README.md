# gh-portfolio

A self-updating portfolio site that renders a GitHub profile and its
repositories. New repositories appear on the site automatically — no manual
edits, no rebuilds, no redeploy.

- **Backend**: Rust (axum + tokio + reqwest/rustls), embedded static frontend
- **Data**: GitHub REST API v3 with conditional requests (ETag / `If-None-Match`)
- **Cache**: in-memory `Arc<RwLock<...>>` + JSON snapshot on disk
- **Deploy**: multi-stage Docker image, non-root user, healthcheck

## Quick start

### Local (cargo)

`.env` is loaded automatically at startup (via `dotenvy`), so plain
`cargo run` behaves exactly like the Docker deployment. Real environment
variables always override values from the file.

```bash
cp .env.example .env        # set GITHUB_USERNAME at minimum
cargo run --release
# open http://localhost:8080
```

To point `tracing` at a different log level:

```bash
RUST_LOG=gh_portfolio=debug cargo run
```

### Docker

```bash
cp .env.example .env        # set GITHUB_USERNAME at minimum
docker compose up -d --build
# open http://localhost:8080
```

The compose file mounts `./data` as the cache volume, so a restart serves the
last snapshot immediately while the first refresh runs. Inside the container
the app always listens on 8080; `PORT` from `.env` only selects the *host*
port of the mapping (`${PORT}:8080`).

> **Linux note:** the container runs as the non-root user `app`. If the
> bind-mounted `./data` directory is owned by root, grant write access once:
> `mkdir -p data && sudo chown 1000:1000 data`.

## How the automatic updates work

1. On startup the last on-disk snapshot (`data/cache.json`) is loaded, then a
   background task immediately refreshes from the GitHub API.
2. The task repeats every `REFRESH_INTERVAL_SECS` (default 300). Every request
   carries the stored `ETag` as `If-None-Match`; GitHub answers `304 Not
   Modified` (free with respect to the rate limit) while nothing changed.
3. Profile, repository list and per-repo `/languages` are stored in a
   `tokio::sync::RwLock`; the whole snapshot is persisted to disk after each
   successful refresh.
4. On an API error or rate-limit exhaustion, the last good cache keeps being
   served and the error is logged — the site never goes blank.
5. The frontend polls `/api/repos`, `/api/profile` and
   `/api/stats/languages` every 30 s and swaps the DOM in place, so new
   repositories show up without a page reload.
6. `POST /api/refresh` (with the admin token) asks the background task to
   refresh right away.

## Configuration

| Variable               | Default     | Description                                              |
|------------------------|-------------|----------------------------------------------------------|
| `GITHUB_USERNAME`      | *required*  | GitHub user the site renders                              |
| `GITHUB_TOKEN`         | *(none)*    | Optional PAT; raises the rate limit to 5000 req/h         |
| `REFRESH_INTERVAL_SECS`| `300`       | Polling interval (minimum 15 s)                           |
| `PORT`                 | `8080`      | HTTP listen port                                          |
| `ADMIN_TOKEN`          | *(none)*    | Enables `POST /api/refresh` when set                      |
| `PINNED_REPOS`         | *(none)*    | Comma-separated repos shown first, in the given order      |
| `EXCLUDED_REPOS`       | *(none)*    | Comma-separated repos that are never shown                 |
| `SHOW_FORKS`           | `0`         | Show forked repositories                                   |
| `SHOW_ARCHIVED`        | `0`         | Show archived repositories                                 |
| `CACHE_DIR`            | `./data`    | Directory for the JSON snapshot (`/data` in Docker)        |
| `RUST_LOG`             | `info`      | `tracing` filter                                          |

## API

| Endpoint                    | Description                                                        |
|-----------------------------|--------------------------------------------------------------------|
| `GET /`                     | The portfolio page (HTML + CSS/JS embedded in the binary)           |
| `GET /api/profile`          | GitHub profile as JSON; `503` until the first successful fetch      |
| `GET /api/repos`            | Repositories after view rules, with query params:<br>`q` — search by name/description/topics<br>`language` — exact language filter<br>`topic` — exact topic filter<br>`sort` — `stars` / `updated` / `name`<br>`order` — `asc` / `desc` |
| `GET /api/stats/languages`  | Aggregated language bytes across visible repositories               |
| `GET /health`               | Liveness + last refresh time (used by the Docker `HEALTHCHECK`)     |
| `POST /api/refresh`         | Header `Authorization: Bearer <ADMIN_TOKEN>`; forces a refresh      |

Forks and archived repositories are hidden by default (unless
`SHOW_FORKS` / `SHOW_ARCHIVED`); pinned repositories keep their configured
order at the top of the list.

## Development

```bash
cargo test            # unit tests: GitHub payload parsing + filter/sort logic
cargo fmt --check
cargo clippy -- -D warnings
```

Project layout:

```
src/
  main.rs     entrypoint: config, state, background loop, graceful shutdown
  config.rs   environment-variable configuration
  github.rs   GitHub REST client with ETag conditional requests
  cache.rs    in-memory cache, disk snapshot, refresh cycle
  filter.rs   view rules (pin/exclude/forks/archived), search, sorting
  routes.rs   axum router and handlers, embedded static assets
  models.rs   Profile / Repo models (match GitHub field names)
  error.rs    AppError -> JSON 500 responses
static/       frontend (no build step, embedded into the binary)
tests/fixtures/  sample GitHub API payloads used by unit tests
```

## Design concept — “Terminal Aurora”

Dark-first deep-slate surface (`#0b1220`) with an indigo→cyan
(`#6366f1 → #22d3ee`) accent used as one consistent idea: gradient text for
numbers and headings, gradient spine on hovered cards, gradient avatar ring.
Monospace (system mono stack) for repository names, language labels and all
numerals echoes the GitHub-native feel; a humanist sans stack carries the
prose. Light theme is a warm paper variant of the same palette, driven by
`prefers-color-scheme` with a persisted manual toggle. Motion is restrained:
one entrance rise for cards, shimmer skeletons, a growing bar chart — all
disabled under `prefers-reduced-motion`. No external fonts or CDNs: the page
is fully self-contained and works offline behind a proxy.
