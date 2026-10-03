.DEFAULT_GOAL := help

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

typos: ## Check spelling
	@command -v typos >/dev/null || { echo "typos not found · run make tools"; exit 1; }
	typos

check: fmt-check lint test deny typos ## Run the local gate, same as CI

cov: ## Build an HTML coverage report
	cargo llvm-cov --workspace --all-features --html

doc: ## Build the API docs
	cargo doc --workspace --no-deps --all-features

tools: ## Install cargo-deny, typos and cargo-llvm-cov
	cargo install --locked cargo-deny typos-cli@1.42.3 cargo-llvm-cov
	rustup component add llvm-tools-preview

clean: ## Remove build artifacts
	cargo clean
