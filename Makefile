.PHONY: help build test clean fmt lint check ci-check install run dev doc bench audit deps update watch all release

# Default target
.DEFAULT_GOAL := help

# Colors for output
BLUE := \033[0;34m
GREEN := \033[0;32m
YELLOW := \033[0;33m
RED := \033[0;31m
NC := \033[0m # No Color

help: ## Show this help message
	@echo "$(BLUE)Janus - RDF Stream Processing Engine$(NC)"
	@echo ""
	@echo "$(GREEN)Available targets:$(NC)"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  $(YELLOW)%-15s$(NC) %s\n", $$1, $$2}'

build: ## Build the project in debug mode
	@echo "$(BLUE)Building project...$(NC)"
	cargo build

release: ## Build the project in release mode
	@echo "$(BLUE)Building release version...$(NC)"
	cargo build --release

test: ## Run all tests
	@echo "$(BLUE)Running tests...$(NC)"
	cargo test --all-features

test-verbose: ## Run tests with verbose output
	@echo "$(BLUE)Running tests (verbose)...$(NC)"
	cargo test --all-features -- --nocapture --test-threads=1

test-integration: ## Run integration tests only
	@echo "$(BLUE)Running integration tests...$(NC)"
	cargo test --test '*' --all-features

test-unit: ## Run unit tests only
	@echo "$(BLUE)Running unit tests...$(NC)"
	cargo test --lib --all-features

coverage: ## Generate test coverage report
	@echo "$(BLUE)Generating coverage report...$(NC)"
	cargo llvm-cov --html --open

fmt: ## Format code using rustfmt
	@echo "$(BLUE)Formatting code...$(NC)"
	cargo fmt --all

fmt-check: ## Check code formatting
	@echo "$(BLUE)Checking code formatting...$(NC)"
	cargo fmt --all -- --check

lint: ## Run clippy lints
	@echo "$(BLUE)Running clippy...$(NC)"
	cargo clippy --all-targets --all-features -- -D warnings

check: fmt-check lint ## Run all checks (formatting and linting)
	@echo "$(GREEN)All checks passed!$(NC)"

ci-check: ## Run full CI/CD checks locally before pushing
	@echo "$(BLUE)Running CI/CD checks...$(NC)"
	@./scripts/ci-check.sh

clean: ## Clean build artifacts
	@echo "$(BLUE)Cleaning build artifacts...$(NC)"
	cargo clean
	rm -rf target/

doc: ## Generate and open documentation
	@echo "$(BLUE)Generating documentation...$(NC)"
	cargo doc --no-deps --open

doc-all: ## Generate documentation with dependencies
	@echo "$(BLUE)Generating documentation with dependencies...$(NC)"
	cargo doc --open

bench: ## Run benchmarks
	@echo "$(BLUE)Running benchmarks...$(NC)"
	cargo bench

bench-save: ## Run benchmarks and save baseline
	@echo "$(BLUE)Running benchmarks and saving baseline...$(NC)"
	cargo bench -- --save-baseline

audit: ## Run security audit
	@echo "$(BLUE)Running security audit...$(NC)"
	cargo audit

install: ## Install the binary
	@echo "$(BLUE)Installing janus...$(NC)"
	cargo install --path .

run: ## Run the main binary
	@echo "$(BLUE)Running janus...$(NC)"
	cargo run

run-release: ## Run the release binary
	@echo "$(BLUE)Running janus (release)...$(NC)"
	cargo run --release

example-basic: ## Run basic example
	@echo "$(BLUE)Running basic example...$(NC)"
	cargo run --example basic

dev: ## Start development mode with auto-reload
	@echo "$(BLUE)Starting development mode...$(NC)"
	cargo watch -x check -x test -x run

watch: ## Watch for changes and run tests
	@echo "$(BLUE)Watching for changes...$(NC)"
	cargo watch -x test

deps: ## Check for outdated dependencies
	@echo "$(BLUE)Checking dependencies...$(NC)"
	cargo outdated

update: ## Update dependencies
	@echo "$(BLUE)Updating dependencies...$(NC)"
	cargo update

all: check test build ## Run checks, tests, and build

ci: fmt-check lint test ## Run all CI checks
	@echo "$(GREEN)CI checks passed!$(NC)"

# Docker targets
docker-oxigraph: ## Start Oxigraph server in Docker
	@echo "$(BLUE)Starting Oxigraph server...$(NC)"
	@docker ps | grep -q oxigraph-server || \
		docker run -d -p 7878:7878 --name oxigraph-server oxigraph/oxigraph
	@echo "$(GREEN)Oxigraph server running on http://localhost:7878$(NC)"

docker-jena: ## Start Apache Jena Fuseki in Docker
	@echo "$(BLUE)Starting Apache Jena Fuseki...$(NC)"
	@docker ps | grep -q jena-server || \
		docker run -d -p 3030:3030 --platform linux/amd64 \
		-v $(PWD)/fuseki-config:/fuseki/configuration \
		-v $(PWD)/fuseki-config/shiro.ini:/fuseki/shiro.ini \
		--name jena-server stain/jena-fuseki
	@echo "$(GREEN)Jena Fuseki running on http://localhost:3030$(NC)"

docker-start: docker-oxigraph docker-jena ## Start all Docker services

docker-stop: ## Stop all Docker services
	@echo "$(BLUE)Stopping Docker services...$(NC)"
	@docker stop oxigraph-server jena-server 2>/dev/null || true
	@echo "$(GREEN)Docker services stopped$(NC)"

docker-clean: docker-stop ## Stop and remove all Docker containers
	@echo "$(BLUE)Cleaning Docker containers...$(NC)"
	@docker rm oxigraph-server jena-server 2>/dev/null || true
	@echo "$(GREEN)Docker containers removed$(NC)"

docker-logs-oxigraph: ## Show Oxigraph logs
	@docker logs -f oxigraph-server

docker-logs-jena: ## Show Jena Fuseki logs
	@docker logs -f jena-server

# Development setup
setup: ## Setup development environment
	@echo "$(BLUE)Setting up development environment...$(NC)"
	@rustup component add rustfmt clippy
	@cargo install cargo-watch cargo-audit cargo-outdated cargo-llvm-cov 2>/dev/null || true
	@echo "$(GREEN)Development environment ready!$(NC)"

setup-check: ## Check development environment setup
	@echo "$(BLUE)Checking development environment...$(NC)"
	@which rustc >/dev/null && echo "$(GREEN)✓ Rust installed$(NC)" || echo "$(RED)✗ Rust not installed$(NC)"
	@which cargo >/dev/null && echo "$(GREEN)✓ Cargo installed$(NC)" || echo "$(RED)✗ Cargo not installed$(NC)"
	@which rustfmt >/dev/null && echo "$(GREEN)✓ rustfmt installed$(NC)" || echo "$(YELLOW)⚠ rustfmt not installed$(NC)"
	@which cargo-clippy >/dev/null && echo "$(GREEN)✓ clippy installed$(NC)" || echo "$(YELLOW)⚠ clippy not installed$(NC)"
	@which docker >/dev/null && echo "$(GREEN)✓ Docker installed$(NC)" || echo "$(YELLOW)⚠ Docker not installed$(NC)"

# Utility targets
tree: ## Show project structure
	@tree -I 'target|node_modules' -L 3

loc: ## Count lines of code
	@echo "$(BLUE)Lines of code:$(NC)"
	@find src -name '*.rs' | xargs wc -l | tail -n 1

bloat: ## Check binary size and dependencies
	@cargo bloat --release

version: ## Show version information
	@echo "$(BLUE)Janus version:$(NC) $(shell cargo pkgid | cut -d# -f2)"
	@echo "$(BLUE)Rust version:$(NC) $(shell rustc --version)"
	@echo "$(BLUE)Cargo version:$(NC) $(shell cargo --version)"
