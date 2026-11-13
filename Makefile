.DEFAULT_GOAL := help

.PHONY: help setup build test lint fmt check contract/ts contract/check docs/check \
	rust/build rust/test rust/lint rust/fmt ui/install ui/build ui/test ui/lint ui/e2e ui/fmt dev lemonade/fetch

UI_PRESENT := $(wildcard ui/package.json)

##@ Bootstrap

setup: ## Install git hooks, the scratch area and UI dependencies
	.agents/bootstrap.sh
	$(if $(UI_PRESENT),cd ui && npm ci)

##@ Development

dev: ## Run the desktop app in development
	cd src-tauri && cargo tauri dev

##@ Build & Quality

build: rust/build $(if $(UI_PRESENT),ui/build) ## Build everything

test: rust/test $(if $(UI_PRESENT),ui/test) ## Run unit and integration tests

lint: rust/lint $(if $(UI_PRESENT),ui/lint) contract/check docs/check ## Run every linter and drift check
	pre-commit run --all-files

fmt: rust/fmt $(if $(UI_PRESENT),ui/fmt) ## Format all code

check: lint test ## Run every gate

rust/build: ## Build the Rust workspace
	cargo build --workspace

rust/test: ## Test the Rust workspace
	cargo test --workspace

rust/lint: ## Clippy and rustfmt check
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings

rust/fmt: ## Format Rust code
	cargo fmt --all

ui/install: ## Install UI dependencies
	cd ui && npm ci

ui/build: ## Build the interface
	cd ui && npm run build

ui/test: ## Unit-test the interface
	cd ui && npm test

ui/e2e: ## Browser tests against the mock engine
	cd ui && npm run e2e

ui/lint: ## Lint and type-check the interface
	cd ui && npm run lint

ui/fmt: ## Format the interface
	cd ui && npm run fmt

##@ Lemonade

lemonade/fetch: ## Download the pinned embedded Lemonade for this machine
	scripts/fetch-lemonade.sh

##@ Contract

contract/ts: ## Regenerate TypeScript bindings from crates/contract
	scripts/bindings.sh

# Bindings are committed; this fails when they are stale.
contract/check: contract/ts
	git diff --exit-code -- ui/src/engine/bindings

##@ Docs

docs/check: ## Validate the OKF docs bundle
	okf validate docs > /dev/null

##@ Utilities

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} \
	  /^[a-zA-Z0-9_/-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } \
	  /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)
