#!/usr/bin/env bash
# Proves all four SDKs work against a real server, not just that they compile.
#
# Each SDK's example claims one execution, runs a handler, reports the outcome,
# and the script asserts the server recorded success. The four are driven against
# a single server so the result is a parity claim rather than four separate
# anecdotes.
#
#   ./scripts/verify-sdks.sh
#
# Needs: cargo, go, node (with the SDK's deps installed), and a reachable
# Postgres. Registration is closed so the first account claims the one-time
# bootstrap slot and becomes OWNER, which is what lets the script create a queue.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
PORT="${FORGE_SDK_E2E_PORT:-3990}"
DB="forge_sdk_e2e"
API="http://127.0.0.1:${PORT}/api/v1"
FAILED=0

log() { printf '%s\n' "$*"; }
run() {
    # Runs a step, records failure without stopping, so one broken SDK does not
    # hide the state of the other three.
    local label="$1"; shift
    if "$@"; then
        return 0
    fi
    printf '  FAILED: %s\n' "$label" >&2
    FAILED=1
    return 1
}

docker exec forge-postgres psql -U forge -d postgres -q \
    -c "DROP DATABASE IF EXISTS ${DB};" >/dev/null 2>&1 || true
docker exec forge-postgres psql -U forge -d postgres -q \
    -c "CREATE DATABASE ${DB};" >/dev/null

export FORGE_DATABASE_URL="postgres://forge:forgepassword@localhost:5432/${DB}"
export FORGE_SERVER_HOST="127.0.0.1"
export FORGE_SERVER_PORT="${PORT}"
export FORGE_AUTH_SESSION_SECRET="sdk-e2e-session-secret"
export FORGE_API_KEY_HASHING_SECRET="sdk-e2e-hash-secret"
export FORGE_PUBLIC_BASE_URL="${API}"
export FORGE_ALLOW_OPEN_REGISTRATION="false"

SERVER_PID=""
cleanup() {
    [ -n "${SERVER_PID}" ] && kill "${SERVER_PID}" 2>/dev/null || true
    docker exec forge-postgres psql -U forge -d postgres -q \
        -c "DROP DATABASE IF EXISTS ${DB};" >/dev/null 2>&1 || true
}
trap cleanup EXIT

log "=== building the server ==="
cargo build -p forge-server
./target/debug/forge-server >/tmp/forge-sdk-e2e-server.log 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 60); do
    if [ "$(curl -s -o /dev/null -w '%{http_code}' "${API}/health/live" 2>/dev/null)" = "200" ]; then
        break
    fi
    sleep 1
done
if [ "$(curl -s -o /dev/null -w '%{http_code}' "${API}/health/live" 2>/dev/null)" != "200" ]; then
    log "the server did not come up; see /tmp/forge-sdk-e2e-server.log" >&2
    exit 1
fi

# The first registration claims bootstrap and becomes OWNER, so this account can
# create the queue every probe then shares. Each probe registers its own user,
# but only a VIEWER, which is enough to run a worker: registering a worker needs
# workers:register, and a worker token needs only the three worker permissions.
OWNER_JSON=$(curl -s -X POST "${API}/auth/register" \
    -H 'content-type: application/json' \
    -d '{"email":"sdk-e2e-owner@example.com","password":"SdkE2EOwnerPass123!","tenant_name":"SDK E2E"}')
OWNER_TOKEN=$(printf '%s' "${OWNER_JSON}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["access_token"])')
QUEUE_JSON=$(curl -s -X POST "${API}/queues" \
    -H 'content-type: application/json' -H "authorization: Bearer ${OWNER_TOKEN}" \
    -d '{"name":"sdk-e2e"}')
export FORGE_TEST_QUEUE_ID=$(printf '%s' "${QUEUE_JSON}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["id"])')
export FORGE_TEST_QUEUE_NAME=$(printf '%s' "${QUEUE_JSON}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["name"])')
log "shared queue: ${FORGE_TEST_QUEUE_NAME} (${FORGE_TEST_QUEUE_ID})"

export FORGE_TEST_TENANT=$(printf '%s' "${OWNER_JSON}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["tenant_id"])')
# Every probe reuses this credential: registration is closed, and only the first
# account can claim the bootstrap slot. Registering a worker needs
# workers:register, which OWNER grants.
export FORGE_TEST_TOKEN="${OWNER_TOKEN}"
export FORGE_TEST_API="${API}"

log ""
log "=== each SDK's own tests ==="
run "python tests" bash -c 'cd sdk/python && python3 -m unittest discover -s tests 2>&1 | tail -3'
run "go tests"     bash -c 'cd sdk/go && go test ./... 2>&1 | tail -2'
run "node tests"   bash -c 'cd sdk/node && npm test 2>&1 | tail -2'
run "java compile" bash -c 'cd sdk/java && mvn -q -o compile && echo compiled'

log ""
log "=== each SDK executes one real job ==="

run "python" bash -c "cd sdk/python && python3 examples/e2e.py"
run "go"     bash -c "cd sdk/go && go run ./examples/e2e"
run "node"   bash -c "cd sdk/node && FORGE_SDK=${ROOT}/sdk/node/dist/index.js node examples/e2e.js"
run "java"   bash -c "cd sdk/java && CP=target/classes:\$HOME/.m2/repository/com/fasterxml/jackson/core/jackson-databind/2.15.2/jackson-databind-2.15.2.jar:\$HOME/.m2/repository/com/fasterxml/jackson/core/jackson-core/2.15.2/jackson-core-2.15.2.jar:\$HOME/.m2/repository/com/fasterxml/jackson/core/jackson-annotations/2.15.2/jackson-annotations-2.15.2.jar && javac -cp \$CP -d /tmp/forge-sdk-e2e examples/e2e/E2E.java && java -cp \$CP:/tmp/forge-sdk-e2e E2E"

log ""
if [ "${FAILED}" -eq 0 ]; then
    log "all four SDKs executed real work against one server"
else
    log "at least one SDK failed" >&2
fi
exit "${FAILED}"
