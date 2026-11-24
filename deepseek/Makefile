.PHONY: dev build test fmt clippy docker

dev:
	cargo run

build:
	cargo build --release

test:
	cargo test

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

docker:
	docker compose up -d --build