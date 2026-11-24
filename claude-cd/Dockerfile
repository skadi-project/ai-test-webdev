FROM rust:slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && printf 'fn main() {}\n' > src/main.rs && cargo build --release && rm -rf src
COPY src ./src
COPY templates ./templates
RUN cargo build --release

FROM debian:bookworm-slim
RUN useradd --create-home --uid 10001 appuser
WORKDIR /app
COPY --from=builder /app/target/release/github-portfolio /app/github-portfolio
COPY static /app/static
COPY templates /app/templates
RUN mkdir -p /data/cache && chown -R appuser:appuser /app /data
USER appuser
ENV PORT=8080 CACHE_DIR=/data/cache RUST_LOG=info
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD /usr/bin/curl -fsS http://127.0.0.1:8080/health || exit 1
CMD ["/app/github-portfolio"]
