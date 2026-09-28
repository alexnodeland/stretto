# stretto's everyday commands. `make` or `make help` lists them.
#
# The Rust targets run the commands CI runs (.github/workflows/ci.yml and
# coverage.yml), so a green `make ci` before a push means a green check job.
# Cargo honours CARGO_TARGET_DIR, and so do the targets that run the binaries.

.DEFAULT_GOAL := help

# Line coverage under which `make coverage`, and the Coverage workflow that
# runs it, fails: four points below the 82.05% measured on 2026-09-28
# (cargo-llvm-cov 0.9.1, Rust 1.98.1). Raise it as coverage grows.
COVERAGE_MIN := 78

# The rust-version Cargo.toml claims, as a rustup toolchain (1.88 -> 1.88.0).
MSRV := $(shell sed -n 's/^rust-version = "\([0-9]*\.[0-9]*\)"$$/\1.0/p' Cargo.toml)

# Where cargo writes the debug binaries the quickstart and walkthrough run.
BIN := $(or $(CARGO_TARGET_DIR),target)/debug

# The tag `make docker` gives the image.
IMAGE ?= stretto:dev

.PHONY: help fmt check lint test types doc ci msrv coverage bless quickstart walkthrough \
	site docker install-dev-tools all

help: ## Show this help
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Targets:'
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z_-]+:.*## / { printf "  %-18s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

fmt: ## Format the Rust code
	cargo fmt --all

check: ## Check the formatting, as CI does
	cargo fmt --all -- --check

lint: ## Clippy on every target, with warnings as errors, and on the console's ts feature
	cargo clippy --all-targets -- -D warnings
	cargo clippy -p stretto-console --all-targets --features ts -- -D warnings

test: ## Unit, integration and doc tests (the docs' drift tests among them)
	cargo test --all-targets
	cargo test --doc

types: ## The API's TypeScript for the UI (console/src/api/generated/): rewrites it, and fails if it was out of date
	cargo test -p stretto-console --features ts --lib api::typescript

doc: ## API docs, with rustdoc warnings as errors
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

ci: check lint test types doc ## What CI's check job runs: check, lint, test, types and doc

msrv: ## Check the workspace on Cargo.toml's rust-version, with the lockfile
	rustup toolchain install $(MSRV) --profile minimal
	cargo +$(MSRV) check --workspace --all-targets --locked

coverage: ## Line coverage with cargo-llvm-cov: writes lcov.info, fails under COVERAGE_MIN
	cargo llvm-cov --workspace --all-targets --no-report
	cargo llvm-cov report --lcov --output-path lcov.info
	cargo llvm-cov report --fail-under-lines $(COVERAGE_MIN)

bless: ## Regenerate docs/cli.md and the review examples from the code
	STRETTO_BLESS=1 cargo test

quickstart: ## The no-key quickstart (examples/quickstart/run.sh) on a debug build
	cargo build -p stretto-report -p stretto-proxy
	sh examples/quickstart/run.sh --bin $(BIN)

walkthrough: ## docs/walkthrough.md end to end on a debug build (needs Node for npx)
	cargo build -p stretto-report -p stretto-proxy
	python3 scripts/walkthrough.py --bin $(BIN)

site: website/node_modules/.package-lock.json ## Build the documentation site (website/), failing on a dead link
	cd website && npm run build

# npm writes node_modules/.package-lock.json last, so an install that stopped
# halfway runs again.
website/node_modules/.package-lock.json: website/package-lock.json
	cd website && npm ci --no-audit --no-fund

docker: ## Build the container image from the Dockerfile (IMAGE=stretto:dev)
	docker build -t $(IMAGE) .

install-dev-tools: ## Install rustfmt, clippy, llvm-tools and cargo-llvm-cov
	rustup component add rustfmt clippy llvm-tools-preview
	command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov --locked
	@command -v shellcheck >/dev/null 2>&1 || echo 'shellcheck, which CI runs on the shell scripts, comes from your package manager'

all: fmt lint test types coverage ## Format, then lint, test, types and coverage
