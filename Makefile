.DEFAULT_GOAL := help

.PHONY: help setup build test lint fmt check docs/check

##@ Bootstrap

setup: ## Install git hooks and the local scratch area
	.agents/bootstrap.sh

##@ Build & Quality

# The application lands with the first implementation wave; until then these say so.
build: ## Build the app
	@printf 'No application code yet.\n'

test: ## Run the test suite
	@printf 'No application code yet.\n'

lint: ## Run every hook against the whole tree
	pre-commit run --all-files

fmt: ## Apply the hook fixers (whitespace, end of file)
	-pre-commit run --all-files trailing-whitespace
	-pre-commit run --all-files end-of-file-fixer

check: lint docs/check test ## Run every gate

##@ Docs

docs/check: ## Validate the OKF docs bundle
	okf validate docs > /dev/null

##@ Utilities

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} \
	  /^[a-zA-Z0-9_/-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } \
	  /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)
