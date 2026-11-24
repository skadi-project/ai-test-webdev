FROM rust:1.88-slim-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY static ./static
RUN cargo build --release
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* && useradd -r -u 10001 app
WORKDIR /app
RUN mkdir data && chown -R app:app /app
COPY --from=build --chown=app:app /app/target/release/github-portfolio /usr/local/bin/github-portfolio
COPY --from=build --chown=app:app /app/static /app/static
USER app
ENV PORT=8080 CACHE_FILE=/app/data/cache.json
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD curl --fail --silent http://127.0.0.1:8080/health || exit 1
CMD ["/usr/local/bin/github-portfolio"]
