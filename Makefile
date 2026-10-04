.PHONY: help setup db-up db-down db-reset seed dev-api dev-web test check fmt fmt-check lint web-check clean migrate backup restore

DATABASE_URL ?= postgres://forge:forgepassword@localhost:5432/forgedb

help: ## Show the available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'

setup: ## Verify the toolchain is present
	@command -v cargo >/dev/null 2>&1 || { echo >&2 "Rust (cargo) is required."; exit 1; }
	@command -v docker >/dev/null 2>&1 || { echo >&2 "Docker is required for the database."; exit 1; }
	@echo "Toolchain ready. Run 'make db-up' to start PostgreSQL."

db-up: ## Start PostgreSQL and wait for it
	docker compose up -d postgres
	@echo "Waiting for PostgreSQL..."
	@until docker compose exec -T postgres pg_isready -U forge -d forgedb >/dev/null 2>&1; do sleep 1; done
	@echo "PostgreSQL ready."

db-down: ## Stop all services
	docker compose down

db-reset: ## Drop and recreate the database volume
	docker compose down -v

seed: ## Load demo data (idempotent)
	psql "$(DATABASE_URL)" -v ON_ERROR_STOP=1 -f scripts/seed.sql

migrate: ## Note about migrations
	@echo "Migrations are applied automatically when forge-server starts."

dev-api: ## Run the API and background runtime
	FORGE_DATABASE_URL="$(DATABASE_URL)" cargo run -p forge-server

dev-web: ## Run the console in development
	cd forge-web && npm run dev

test: ## Run the whole test suite
	DATABASE_URL="$(DATABASE_URL)" cargo test --workspace

check: ## Type-check the workspace
	cargo check --workspace --all-targets

fmt: ## Format the workspace
	cargo fmt

fmt-check: ## Verify formatting without changing files
	cargo fmt -- --check

lint: ## Lint with CI's settings
	cargo clippy --workspace --all-targets -- -D warnings

web-check: ## Type-check the console
	cd forge-web && npm run typecheck

clean: ## Remove build output and stop services
	cargo clean
	docker compose down -v

backup: ## Dump the database (BACKUP_FILE=path)
	./scripts/backup.sh "$(DATABASE_URL)" "$(BACKUP_FILE)"

restore: ## Restore a dump (BACKUP_FILE=path)
	./scripts/restore.sh "$(DATABASE_URL)" "$(BACKUP_FILE)"
