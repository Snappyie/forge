#!/usr/bin/env bash
set -e

# Starts Forge locally.
#
# The API and the console are one binary, so http://localhost:3000 serves both.
# The console is exported to static files and embedded at compile time, so the
# first `cargo run` builds the export automatically (see crates/forge-server/
# build.rs) and every later start needs no Node.
#
# Pass --dev-ui to run the console through `next dev` instead, for hot reload
# while working on the frontend. That needs Node and serves on port 3001,
# pointed at the API on 3000.

DEV_UI=0
[ "${1:-}" = "--dev-ui" ] && DEV_UI=1

echo "Starting PostgreSQL via Docker (if not already running)..."
# We only spin up Postgres in docker to avoid manual DB installation
docker run -d --name forge-postgres \
    -e POSTGRES_USER=forge \
    -e POSTGRES_PASSWORD=forgepassword \
    -e POSTGRES_DB=forgedb \
    -p 5432:5432 postgres:15-alpine 2>/dev/null || echo "Postgres container already exists, ensuring it's started..." && docker start forge-postgres

echo "Waiting for PostgreSQL to be ready..."
# Polled rather than slept on: `sleep 3` races the database on a cold start, and
# the failure surfaces as a confusing connection error from the server rather
# than as "Postgres was not up yet".
for _ in $(seq 1 30); do
    if docker exec forge-postgres pg_isready -U forge -d forgedb >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

# Export environment variables required by the Rust API Server
export FORGE_DATABASE_URL="postgres://forge:forgepassword@localhost:5432/forgedb"
export FORGE_SERVER_HOST="0.0.0.0"
export FORGE_SERVER_PORT="3000"
export FORGE_PUBLIC_BASE_URL="http://localhost:3000"
export FORGE_AUTH_SESSION_SECRET="local-development-session-secret"
export FORGE_API_KEY_HASHING_SECRET="local-development-hash-secret"
export FORGE_LOG_LEVEL="info"
export FORGE_RATE_LIMIT_PER_SECOND="1000"
export FORGE_RATE_LIMIT_BURST="5000"
export FORGE_ALLOW_OPEN_REGISTRATION="true"

if [ "$DEV_UI" -eq 1 ]; then
    echo "Starting Forge Web UI (Next.js dev server) in the background..."
    cd forge-web
    if [ ! -d "node_modules" ]; then
        npm ci
    fi

    # The dev server runs on 3001 and talks to the API on 3000, so it needs the
    # absolute URL - the same-origin default only applies to the embedded build.
    export NEXT_PUBLIC_API_URL="http://localhost:3000/api/v1"
    export NEXT_PUBLIC_FORGE_ENVIRONMENT="Local"
    # `next dev` defaults to 3000, which the API already holds.
    export PORT=3001
    npm run dev &
    WEB_PID=$!
else
    WEB_PID=""
fi

echo "Starting Forge Server (Rust, with the console embedded) in the background..."
# Compiles and runs natively. On a clean checkout this also builds the console
# export, which is why it can take a while the first time.
cargo run -p forge-server &
SERVER_PID=$!

echo ""
echo "================================================="
echo "🚀 Forge is running locally!"
if [ "$DEV_UI" -eq 1 ]; then
    echo "📡 API Server: http://localhost:3000/api/v1"
    echo "🌐 Web UI:     http://localhost:3001  (hot reload)"
else
    echo "🌐 Console + API, one binary, one port:"
    echo "   http://localhost:3000"
    echo "   (rebuild the console with: cd forge-web && npm run build)"
fi
echo "🛑 Press Ctrl+C to stop."
echo "================================================="
echo ""

# Handle graceful shutdown on Ctrl+C
trap 'echo "Stopping services..."; kill '"$SERVER_PID"' ${WEB_PID:+$WEB_PID} 2>/dev/null; exit 0' SIGINT SIGTERM

# Wait for background processes
wait