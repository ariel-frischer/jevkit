.PHONY: help install i build bin run test test-v lint format check clean install-global uninstall audit release patch minor major

BIN_NAME=jev
INSTALL_DIR?=$(HOME)/.local/bin

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

install: ## Fetch dependencies
	cargo fetch

i: install ## Alias for install

build: ## Build release binary
	cargo build --release

bin: build ## Alias for build

debug: ## Build debug binary
	cargo build

run: ## Run the CLI (usage: make run ARGS="lint questions.yaml")
	cargo run -- $(ARGS)

test: ## Run tests
	cargo test

test-v: ## Run tests (verbose)
	cargo test -- --nocapture

lint: ## Run clippy
	cargo clippy --all-targets -- -D warnings

format: ## Format code
	cargo fmt

check: ## Verify formatting, lints, and tests
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cargo test

audit: ## Check dependencies for advisories
	@if command -v cargo-deny >/dev/null 2>&1; then \
		cargo deny check; \
	elif command -v cargo-audit >/dev/null 2>&1; then \
		cargo audit; \
	else \
		echo "Neither cargo-deny nor cargo-audit is installed."; \
		echo "Prefer: cargo install cargo-deny && cargo deny --init"; \
	fi

clean: ## Remove build artifacts
	cargo clean

install-global: build ## Install the binary to INSTALL_DIR
	@mkdir -p $(INSTALL_DIR)
	install -m 0755 target/release/$(BIN_NAME) $(INSTALL_DIR)/$(BIN_NAME)
	@echo "Installed $(INSTALL_DIR)/$(BIN_NAME)"

uninstall: ## Remove the installed binary
	@rm -f $(INSTALL_DIR)/$(BIN_NAME)
	@echo "Removed $(INSTALL_DIR)/$(BIN_NAME)"

##@ Release
release: ## Tag and push a release (usage: make release VERSION=v0.1.0)
ifndef VERSION
	$(error VERSION is required, e.g. make release VERSION=v0.1.0)
endif
	@test -z "$$(git status --porcelain)" || (echo "worktree is dirty"; exit 1)
	$(MAKE) check
	git tag -a $(VERSION) -m "Release $(VERSION)"
	git push origin $(VERSION)

patch: ## Bump the patch version and release
	$(eval CURRENT=$(shell git tag --sort=-v:refname | head -1 | sed 's/^v//'))
	$(eval NEXT=v$(shell echo $(CURRENT) | awk -F. '{printf "%d.%d.%d", $$1, $$2, $$3+1}'))
	@echo "Bumping $(CURRENT) -> $(NEXT)"
	$(MAKE) release VERSION=$(NEXT)

minor: ## Bump the minor version and release
	$(eval CURRENT=$(shell git tag --sort=-v:refname | head -1 | sed 's/^v//'))
	$(eval NEXT=v$(shell echo $(CURRENT) | awk -F. '{printf "%d.%d.0", $$1, $$2+1}'))
	@echo "Bumping $(CURRENT) -> $(NEXT)"
	$(MAKE) release VERSION=$(NEXT)

major: ## Bump the major version and release
	$(eval CURRENT=$(shell git tag --sort=-v:refname | head -1 | sed 's/^v//'))
	$(eval NEXT=v$(shell echo $(CURRENT) | awk -F. '{printf "%d.0.0", $$1+1}'))
	@echo "Bumping $(CURRENT) -> $(NEXT)"
	$(MAKE) release VERSION=$(NEXT)
