.PHONY: setup db-up db-down test check fmt lint clean

setup:
	@echo "Checking dependencies..."
	@command -v cargo >/dev/null 2>&1 || { echo >&2 "Rust (cargo) is required but not installed. Aborting."; exit 1; }
	@command -v docker-compose >/dev/null 2>&1 || { echo >&2 "docker-compose is required but not installed. Aborting."; exit 1; }
	@echo "Setup complete. Run 'make db-up' to start the database."

db-up:
	docker-compose up -d
	@echo "Waiting for database to be ready..."
	@sleep 3

db-down:
	docker-compose down

test:
	DATABASE_URL=postgres://forge:forgepassword@localhost:5432/forgedb cargo test

check:
	cargo check

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

clean:
	cargo clean
	docker-compose down -v
