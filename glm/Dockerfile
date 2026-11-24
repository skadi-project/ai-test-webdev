# ---------------------------------------------------------------------------
# Build stage: compile with dependency caching via the dummy-main trick.
# ---------------------------------------------------------------------------
FROM rust:slim AS builder
WORKDIR /app

# 1) Build the dependency graph once; this layer is cached across rebuilds.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/incremental

# 2) Build the real binary (static assets are embedded with include_str!).
COPY src ./src
COPY static ./static
RUN touch src/main.rs && cargo build --release

# ---------------------------------------------------------------------------
# Runtime stage: minimal debian-slim, non-root user, curl for HEALTHCHECK.
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --system --create-home --shell /usr/sbin/nologin app && \
    mkdir -p /data && chown app:app /data

COPY --from=builder /app/target/release/gh-portfolio /usr/local/bin/gh-portfolio

USER app
WORKDIR /home/app
ENV CACHE_DIR=/data PORT=8080
VOLUME /data

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://localhost:${PORT}/health" || exit 1

EXPOSE 8080
ENTRYPOINT ["gh-portfolio"]
