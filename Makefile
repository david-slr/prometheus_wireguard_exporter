.DEFAULT_GOAL := help

.PHONY: help build check test fmt fmt-check lint audit clean docker-build docker-lint docker-test

help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-18s\033[0m %s\n", $$1, $$2}'

build: ## Build binary in release mode
	cargo build --release

check: ## Fast compilation check
	cargo check --all-targets

test: ## Run unit and integration tests
	cargo test

fmt: ## Format source files
	cargo fmt --all

fmt-check: ## Check formatting without modifying files
	cargo fmt --all -- --check

lint: ## Run clippy linter with warnings treated as errors
	cargo clippy --all-targets --all-features -- -D warnings

audit: ## Check dependencies for vulnerabilities, bans, and license issues
	cargo deny check

clean: ## Clean cargo target artifacts
	cargo clean

docker-lint: ## Run clippy linting inside Docker build stage
	docker build --target lint .

docker-test: ## Build and run tests inside Docker container
	docker build --target test -t prometheus-wireguard-exporter-test .
	docker run --rm prometheus-wireguard-exporter-test

docker-build: ## Build final Docker image
	docker build -t prometheus-wireguard-exporter:local .
