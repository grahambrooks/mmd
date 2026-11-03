# mmd — Mermaid diagrams to SVG, in pure Rust. `make help` lists targets.
# `make check` is the gate (the same commands as the ci.yml check job); prek runs it
# before every push.

PREFIX ?= /usr/local

.DEFAULT_GOAL := help

.PHONY: help check fmt fmt-check lint test build-no-default hooks pre-commit build showcase showcase-pages install uninstall update outdated clean

help: ## List targets
	@grep -hE '^[a-zA-Z0-9_-]+:.*## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*## "}; {printf "  %-16s %s\n", $$1, $$2}'

# ── The gate ─────────────────────────────────────────────────────────────────

check: fmt-check lint test build-no-default ## fmt-check + clippy + test + no-default build: exactly what CI's check job runs
	@echo "All pre-commit checks passed."

pre-commit: check ## Alias for check

fmt: ## Format the code
	cargo fmt --all

fmt-check: ## Check formatting without modifying
	cargo fmt --all --check

lint: ## Clippy with warnings denied
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test: ## Run all tests
	cargo test --workspace --all-features --locked

build-no-default: ## The library must stay usable without the bundled font payload
	cargo build --no-default-features --locked

hooks: ## Install the git hooks (pre-commit and pre-push) with prek
	prek install

# ── Build and showcase ───────────────────────────────────────────────────────

build: ## Build the release binary
	cargo build --release --features cli

showcase: ## Coverage matrix, samples and gaps into target/showcase.pdf
	cargo run --example showcase

showcase-pages: showcase ## Also render each page to target/showcase/pageN.png (macOS)
	swift scripts/showcase-pages.swift target/showcase.pdf target/showcase

install: build ## Build and install to $(PREFIX)/bin
	install -d $(PREFIX)/bin
	install -m 755 target/release/mmd $(PREFIX)/bin/mmd
	@echo "Installed $(PREFIX)/bin/mmd"

uninstall: ## Remove from $(PREFIX)/bin
	rm -f $(PREFIX)/bin/mmd

# ── Maintenance ──────────────────────────────────────────────────────────────

update: ## Update dependencies
	cargo update

outdated: ## Show outdated dependencies
	cargo outdated --root-deps-only

clean: ## Remove build artifacts
	cargo clean

# ── Release (release-kit): tag-only CalVer ───────────────────────────────────

include release.mk
