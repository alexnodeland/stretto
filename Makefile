# stretto's everyday commands. `make` or `make help` lists them.
#
# The Rust targets run the commands CI runs (.github/workflows/ci.yml and
# coverage.yml), so a green `make ci` before a push means a green check job.
# Cargo honours CARGO_TARGET_DIR, and so do the targets that run the binaries.

.DEFAULT_GOAL := help

# Line coverage under which `make coverage`, and the Coverage workflow that
# runs it, fails (cargo-llvm-cov 0.9.1, Rust 1.98.1). It rises with each
# change that covers more, up to 100: the floor, not a target.
COVERAGE_MIN := 98

# `make coverage` also fails when a line added since the merge base with this
# ref is not run by any test (scripts/patch_coverage.py).
COVERAGE_BASE ?= origin/main

# The rust-version Cargo.toml claims, as a rustup toolchain (1.88 -> 1.88.0).
MSRV := $(shell sed -n 's/^rust-version = "\([0-9]*\.[0-9]*\)"$$/\1.0/p' Cargo.toml)

# Where cargo writes the debug binaries the quickstart and walkthrough run.
BIN := $(or $(CARGO_TARGET_DIR),target)/debug

# The tag `make docker` gives the image.
IMAGE ?= stretto:dev
CONSOLE_IMAGE ?= stretto-console:dev

# The UI's npm commands, in console/.
NPM := npm --prefix console

.PHONY: help fmt check lint test types doc ci msrv coverage bless quickstart walkthrough \
	site ui ui-check e2e console docker docker-console install-dev-tools all

help: ## Show this help
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Targets:'
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  %-18s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

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

# `clean --workspace` first: --no-report keeps the profiles and test binaries
# of earlier runs (CI's cache restores them), and the report would count them.
coverage: ## Line coverage with cargo-llvm-cov: writes lcov.info; fails under COVERAGE_MIN, or on an added line no test runs
	cargo llvm-cov clean --workspace
	cargo llvm-cov --workspace --all-targets --no-report
	cargo llvm-cov report --lcov --output-path lcov.info
	cargo llvm-cov report --fail-under-lines $(COVERAGE_MIN)
	python3 scripts/patch_coverage.py lcov.info $(COVERAGE_BASE)

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

ui: console/node_modules/.package-lock.json ## Build the console's UI into console/dist, which stretto-console embeds
	$(NPM) run build

ui-check: console/node_modules/.package-lock.json ## The UI's checks, as CI's ui job runs them: formatting, lint, types, unit tests, build
	$(NPM) run format:check
	$(NPM) run lint
	$(NPM) run typecheck
	$(NPM) test
	$(NPM) run build

e2e: console/node_modules/.package-lock.json ## The UI's end-to-end tests in Chromium, on the mock API and on the console over the test fixtures
	cargo build -p stretto-console -p stretto-report -p stretto-proxy
	$(NPM) run e2e
	$(NPM) run e2e:real

console: ui ## Build the UI, then run the console on ~/.stretto (ARGS="--read-only --open")
	cargo run -p stretto-console -- $(ARGS)

console/node_modules/.package-lock.json: console/package-lock.json
	$(NPM) ci --no-audit --no-fund

docker: ## Build the CLI's container image from the Dockerfile (IMAGE=stretto:dev)
	docker build -t $(IMAGE) .

docker-console: ## Build the console's container image, the Dockerfile's console target (CONSOLE_IMAGE=stretto-console:dev)
	docker build --target console -t $(CONSOLE_IMAGE) .

install-dev-tools: ## Install rustfmt, clippy, llvm-tools and cargo-llvm-cov
	rustup component add rustfmt clippy llvm-tools-preview
	command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov --locked
	@command -v shellcheck >/dev/null 2>&1 || echo 'shellcheck, which CI runs on the shell scripts, comes from your package manager'

all: fmt lint test types coverage ## Format, then lint, test, types and coverage
