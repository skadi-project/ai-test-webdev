# syntax=docker/dockerfile:1.7
FROM rust:slim-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
# BuildKit keeps downloaded crates and compiled dependencies between builds.
# Copy out of the cache mount so the runtime stage can access the executable.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --locked --release && cp target/release/github-portfolio /portfolio

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 portfolio \
    && useradd --uid 10001 --gid portfolio --no-create-home --shell /usr/sbin/nologin portfolio \
    && mkdir -p /data && chown portfolio:portfolio /data
COPY --from=build /portfolio /usr/local/bin/github-portfolio
WORKDIR /app
ENV PORT=8080 CACHE_FILE=/data/cache.json LOG_FORMAT=json
USER 10001:10001
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl --fail --silent "http://127.0.0.1:${PORT}/health" > /dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/github-portfolio"]
