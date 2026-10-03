.DEFAULT_GOAL := help

CARGO_HOME ?= $(HOME)/.cargo
TYPOS_VERSION := 1.42.3
CARGO_DENY_VERSION := 0.20.2
CARGO_LLVM_COV_VERSION := 0.9.1
TYPOS_BIN := $(CARGO_HOME)/bin/typos

.PHONY: help fmt fmt-check lint test deny typos check cov doc tools clean

help: ## List the available targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

fmt: ## Format the code
	cargo fmt --all

fmt-check: ## Check formatting
	cargo fmt --all --check

lint: ## Run clippy with warnings denied
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test: ## Run all tests
	cargo test --workspace --all-features --locked

deny: ## Check dependencies: advisories, licences, bans, sources
	@command -v cargo-deny >/dev/null || { echo "cargo-deny not found · run make tools"; exit 1; }
	cargo deny check

typos: ## Check spelling with the pinned typos version
	@bin="$(TYPOS_BIN)"; [ -x "$$bin" ] || bin="$$(command -v typos || true)"; \
	[ -n "$$bin" ] || { echo "typos not found · run make tools"; exit 1; }; \
	found="$$("$$bin" --version | awk '{print $$2}')"; \
	[ "$$found" = "$(TYPOS_VERSION)" ] || { echo "typos $(TYPOS_VERSION) required (found $$found) · run make tools"; exit 1; }; \
	"$$bin"

check: fmt-check lint test deny typos ## Run the local gate, same as CI

cov: ## Build an HTML coverage report
	cargo llvm-cov --workspace --all-features --html

doc: ## Build the API docs
	cargo doc --workspace --no-deps --all-features

# typos-cli is pinned (TYPOS_VERSION) because newer releases need rustc 1.95 (the repo pins 1.90); CI's crate-ci/typos uses the same version (1.42.3).
tools: ## Install cargo-deny, typos and cargo-llvm-cov
	cargo install --locked cargo-deny@$(CARGO_DENY_VERSION) typos-cli@$(TYPOS_VERSION) cargo-llvm-cov@$(CARGO_LLVM_COV_VERSION)
	rustup component add llvm-tools-preview

clean: ## Remove build artifacts
	cargo clean
