#!/usr/bin/env bash
set -e

echo "Starting PostgreSQL via Docker (if not already running)..."
# We only spin up Postgres in docker to avoid manual DB installation
docker run -d --name forge-postgres \
    -e POSTGRES_USER=forge \
    -e POSTGRES_PASSWORD=forgepassword \
    -e POSTGRES_DB=forgedb \
    -p 5432:5432 postgres:15-alpine 2>/dev/null || echo "Postgres container already exists, ensuring it's started..." && docker start forge-postgres

echo "Waiting for PostgreSQL to be ready..."
sleep 3

# Export environment variables required by the Rust API Server
export FORGE_DATABASE_URL="postgres://forge:forgepassword@localhost:5432/forgedb"
export FORGE_SERVER_HOST="0.0.0.0"
export FORGE_SERVER_PORT="3000"
export FORGE_PUBLIC_BASE_URL="http://localhost:3000/api/v1"
export FORGE_AUTH_SESSION_SECRET="local-development-session-secret"
export FORGE_API_KEY_HASHING_SECRET="local-development-hash-secret"
export FORGE_LOG_LEVEL="info"
export FORGE_RATE_LIMIT_PER_SECOND="1000"
export FORGE_RATE_LIMIT_BURST="5000"

echo "Starting Forge API Server (Rust) in the background..."
# This will compile and run the API server natively
cargo run -p forge-server &
SERVER_PID=$!

echo "Starting Forge Web UI (Next.js) in the background..."
cd forge-web
# Ensure dependencies are installed
if [ ! -d "node_modules" ]; then
    npm install
fi

export NEXT_PUBLIC_API_URL="http://localhost:3000/api/v1"
# npm run dev typically starts on 3000, so we force 3001 to match docker-compose mapping
export PORT=3001
npm run dev &
WEB_PID=$!

echo ""
echo "================================================="
echo "🚀 Forge is running locally!"
echo "📡 API Server: http://localhost:3000/api/v1"
echo "🌐 Web UI:     http://localhost:3001"
echo "🛑 Press Ctrl+C to stop both services."
echo "================================================="
echo ""

# Handle graceful shutdown on Ctrl+C
trap "echo 'Stopping services...'; kill $SERVER_PID $WEB_PID 2>/dev/null; exit 0" SIGINT SIGTERM

# Wait for background processes
wait
