.DEFAULT_GOAL := help

.PHONY: help setup build test lint fmt check check-changed contract/ts contract/check docs/check \
	rust/build rust/test rust/lint rust/fmt ui/install ui/build ui/test ui/lint ui/e2e ui/fmt dev lemonade/fetch scripts/test package smoke netcheck netcheck-remote netcheck-online e2e-real e2e-online e2e-models e2e-help e2e-decisions preview-real helpers-pin phone tailnet/test \
	server/build server/image phone/env phone/android phone/android-release phone/lint phone/test \
	site site/serve site/clean

UI_PRESENT := $(wildcard ui/package.json)
GO ?= go

##@ Bootstrap

setup: ## Install git hooks, the scratch area and UI dependencies
	.agents/bootstrap.sh
	$(if $(UI_PRESENT),cd ui && npm ci)

##@ Development

DEV_DATA_DIR ?= $(CURDIR)/target/dev-data

dev: ui/node_modules ## Run the desktop app with a scripted mind and a scratch data dir
	cd src-tauri && NOCTURNE_FAKE_MIND=1 NOCTURNE_DATA_DIR=$(DEV_DATA_DIR) \
		../ui/node_modules/.bin/tauri dev --features fake-mind

# The real preview is for development only and is never packaged. See docs/contribution/building.md.
preview-real: ui/node_modules ## Use the real engine and models from a browser (development only)
	cd ui && npx vite build --mode preview-real
	cargo build -p nocturne-app --features preview --bin nocturne-preview
	scripts/preview-real.sh

##@ Build & Quality

build: rust/build $(if $(UI_PRESENT),ui/build) ## Build everything

test: rust/test scripts/test tailnet/test $(if $(UI_PRESENT),ui/test) ## Run unit and integration tests

lint: rust/lint $(if $(UI_PRESENT),ui/lint) contract/check docs/check site/strings ## Run every linter and drift check
	pre-commit run --all-files

fmt: rust/fmt $(if $(UI_PRESENT),ui/fmt) ## Format all code

check: lint test ## Run every gate

# CI runs the full suite. TESTS is the change's own tests, run last, e.g. TESTS="cargo test -p x y::".
check-changed: ## Run the basics for what changed since BASE (default merge base with main); CI runs the full suite
	BASE="$(BASE)" TESTS="$(TESTS)" scripts/check-changed.sh

rust/build: ## Build the Rust workspace
	cargo build --workspace

rust/test: ## Test the Rust workspace
	cargo test --workspace
	cargo test -p nocturne-app --features server --lib --test preview

rust/lint: ## Clippy and rustfmt check
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy -p nocturne-app --features server --all-targets -- -D warnings

rust/fmt: ## Format Rust code
	cargo fmt --all

ui/install: ## Install UI dependencies
	cd ui && npm ci

ui/build: ui/node_modules ## Build the interface
	cd ui && npm run build

phone: ui/node_modules ## Build the phone app alone (Away from home)
	npm --prefix ui run build:phone

ui/test: ui/node_modules ## Unit-test the interface
	cd ui && npm test

ui/e2e: ui/node_modules ## Browser tests against the mock engine
	cd ui && npm run e2e

ui/lint: ui/node_modules ## Lint and type-check the interface
	cd ui && npm run lint

ui/fmt: ui/node_modules ## Format the interface
	cd ui && npm run fmt

scripts/test: ui/node_modules ## Unit-test the packaging scripts
	python3 -I -m unittest discover -s scripts -p 'test_*.py'

##@ Packaging

package: ui/node_modules ## Build the Linux packages (AppImage, deb, rpm) with the private engine
	scripts/package.sh

smoke: ## Launch the built binary and packages under a virtual display and judge them
	scripts/smoke-launch.sh all

netcheck: ## Run the built binary in a loopback-only namespace and fail on any outbound traffic
	scripts/net-check.sh launch binary

netcheck-remote: ## Run the built binary with Away from home on through a fake helper and judge its traffic
	scripts/net-check.sh remote binary

netcheck-online: ## Run the built binary with Online answers on against a stand-in provider and judge its traffic
	scripts/net-check.sh online binary

# The experimental server and its container image. See docs/adr/0012-server-mode.md.
IMAGE ?= localhost/nocturne:dev

server/build: ui/node_modules ## Build the experimental server and its interface (ui/dist-server)
	cd ui && npx vite build --mode server
	cargo build --release -p nocturne-app --features server --bin nocturne-server

server/image: ## Build the experimental server's container image with podman (IMAGE)
	podman build --format docker --jobs 1 -f Containerfile -t $(IMAGE) .

##@ Real models

e2e-real: ## Run the whole product against the real small mind (downloads about 4.8 GB once), then again with the network cut
	scripts/e2e-real.sh all

e2e-online: ## Run the real small mind with Online answers on against the stand-in provider
	scripts/e2e-real.sh online

e2e-models: ## Run the real models with a model chosen for each job
	scripts/e2e-real.sh models

e2e-help: ## Run the help check on the real models of one size (NOCTURNE_E2E_TIER)
	scripts/e2e-real.sh help

e2e-decisions: ## Run the decision check on the real models of one size (NOCTURNE_E2E_TIER)
	scripts/e2e-real.sh decisions

##@ Away from home

helpers-pin: ## Pin the connection helpers: versions, checksums and captured help (Away from home)
	scripts/helpers-pin.sh

tailnet/test: ## Vet and test the Most private helper, plain and with the release tags (GO=<go>)
	GO="$(GO)" scripts/test-tailnet.sh

##@ Lemonade

lemonade/fetch: ## Download the pinned embedded Lemonade for this machine
	scripts/fetch-lemonade.sh

# Installs UI dependencies when missing or older than the lockfile.
ui/node_modules: ui/package-lock.json
	cd ui && npm ci
	@touch ui/node_modules

##@ Contract

contract/ts: ## Regenerate TypeScript bindings from crates/contract
	scripts/bindings.sh

# Bindings are committed; this fails when they are stale.
contract/check: contract/ts
	git diff --exit-code -- ui/src/engine/bindings

##@ Docs

docs/check: ## Validate the OKF docs bundle
	okf validate docs > /dev/null

##@ Site

site: ## Build the landing page and the docs into site/dist (SITE_URL=<origin>)
	$(MAKE) -C site site

site/strings: ## Check the site's translations: every string in every language, none stale
	$(MAKE) -C site strings

site/serve: ## Serve site/dist on loopback with its headers
	$(MAKE) -C site serve

site/clean: ## Remove the built site
	$(MAKE) -C site clean

##@ Phone app (Android)

# The Android toolchain comes from the environment, never from paths here: JAVA_HOME (JDK 17+),
# ANDROID_HOME, NDK_HOME, and optionally GRADLE_USER_HOME and CARGO_TARGET_DIR. Gradle calls the
# Tauri CLI back through phone/package.json, so the CLI from ui/node_modules goes on PATH.
PHONE_TAURI = cd phone && PATH="$(CURDIR)/ui/node_modules/.bin:$$PATH" npm run --silent tauri --
PHONE_FEATURES ?=

phone/env:
	@for v in JAVA_HOME ANDROID_HOME NDK_HOME; do \
	  if [ -z "$$(printenv $$v)" ]; then echo "$$v is not set; see docs/contribution/building.md" >&2; exit 1; fi; \
	done

phone/android: ui/node_modules phone/env ## Build the phone app's debug APK for arm64 (PHONE_FEATURES=dev-loopback)
	cd ui && npm run build:phone-native
	$(PHONE_TAURI) android build --debug --target aarch64 --apk $(if $(PHONE_FEATURES),--features $(PHONE_FEATURES))

phone/android-release: ui/node_modules phone/env ## Build the release APK and bundle for every phone ABI
	phone/check-release.sh --features
	cd ui && npm run build:phone-native
	$(PHONE_TAURI) android build --target aarch64 --target armv7 --target x86_64 --apk --aab
	phone/check-release.sh $$(find phone/src-tauri/gen/android/app/build/outputs \
	  -name '*-release*.apk' -o -name '*-release*.aab')

phone/lint: ## Clippy for the phone app and its plugins, on this machine
	cargo clippy -p nocturne-phone -p tauri-plugin-nocturne-system -p tauri-plugin-nocturne-calendar \
	  -p tauri-plugin-nocturne-health --all-targets -- -D warnings

phone/test: ## Test the phone app's Rust side on this machine
	cargo test -p nocturne-phone -p tauri-plugin-nocturne-system -p tauri-plugin-nocturne-calendar \
	  -p tauri-plugin-nocturne-health

##@ Utilities

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} \
	  /^[a-zA-Z0-9_/-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } \
	  /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)
