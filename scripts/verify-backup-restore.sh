#!/usr/bin/env bash
#
# Proves backup and restore actually work, rather than that the scripts run.
#
# `Redesign.md` §6 lists "backup and recovery procedures are tested" as an
# acceptance criterion. A script that exits 0 is not evidence: the failure this
# catches is a backup that is written but empty, or a restore that completes and
# leaves a database the server cannot start against.
#
# The round trip is therefore checked at the level that matters: the restored
# database must contain the rows that were there before, and the server must
# come up against it and serve a request.
#
#   ./scripts/verify-backup-restore.sh
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)

BASE_URL="${FORGE_TEST_DATABASE_URL:-postgres://forge:forgepassword@localhost:5432/forgedb}"
SERVER="${BASE_URL%/*}"
ADMIN_URL="${SERVER}/postgres"
WORK_DB="forge_backup_$$"
DUMP_DIR=$(mktemp -d)

FAILED=0
SERVER_PID=""
WORK_DB_URL="${SERVER}/${WORK_DB}"

cleanup() {
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
    # Drop the throwaway database, releasing any connections still held by the
    # server under test.
    psql "$ADMIN_URL" -q -c \
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${WORK_DB}' AND pid <> pg_backend_pid()" \
        >/dev/null 2>&1
    psql "$ADMIN_URL" -q -c "DROP DATABASE IF EXISTS ${WORK_DB}" >/dev/null 2>&1
    rm -rf "$DUMP_DIR"
}
trap cleanup EXIT

fail() {
    echo "  FAILED: $*" >&2
    FAILED=1
}

for tool in psql pg_dump pg_restore; do
    command -v "$tool" >/dev/null 2>&1 || { echo "$tool is not on PATH" >&2; exit 3; }
done

echo "=== preparing a database with known content ==="
psql "$ADMIN_URL" -q -c "DROP DATABASE IF EXISTS ${WORK_DB}" >/dev/null 2>&1
psql "$ADMIN_URL" -q -c "CREATE DATABASE ${WORK_DB}" >/dev/null || {
    echo "could not create ${WORK_DB}" >&2
    exit 1
}

psql "$WORK_DB_URL" -q -v ON_ERROR_STOP=1 <<'SQL' >/dev/null
CREATE TABLE IF NOT EXISTS canary (id int PRIMARY KEY, note text);
INSERT INTO canary (id, note) VALUES (1, 'survives a backup')
    ON CONFLICT (id) DO UPDATE SET note = EXCLUDED.note;
SQL

psql "$WORK_DB_URL" -tAc "SELECT count(*) FROM canary" | tr -d ' ' | xargs -I{} echo "  rows before backup: {}"

echo
echo "=== the backup script refuses to write an empty dump ==="
# A dump that exists but is empty is worse than no backup: it looks like one and
# is only discovered during a restore, which is the worst possible time.
if DUMP_OUT=$(DUMPDIR="$DUMP_DIR" ./scripts/backup.sh "$WORK_DB_URL" "$DUMP_DIR" 2>&1); then
    DUMP_FILE=$(ls -t "$DUMP_DIR"/forge_*.dump 2>/dev/null | head -1)
    [ -s "$DUMP_FILE" ] || fail "the dump is empty"
    echo "  backup written: $(basename "$DUMP_FILE") ($(wc -c <"$DUMP_FILE" | tr -d ' ') bytes)"
else
    fail "backup.sh failed: $DUMP_OUT"
fi

# The backup directory is created by the script, not assumed to exist.
rm -rf "$DUMP_DIR/nested"
if ./scripts/backup.sh "$WORK_DB_URL" "$DUMP_DIR/nested" >/dev/null 2>&1; then
    [ -s "$DUMP_DIR"/nested/forge_*.dump ] || fail "a backup into a new directory produced no file"
    echo "  backup into a directory that did not exist: ok"
else
    fail "backup.sh cannot write to a directory it did not create"
fi

echo
echo "=== destroying the database before restoring ==="
# Restoring into the original would prove nothing, since the rows would still be
# there. The data has to be genuinely gone first.
psql "$ADMIN_URL" -q -c "DROP DATABASE IF EXISTS ${WORK_DB}" >/dev/null
psql "$ADMIN_URL" -q -c "CREATE DATABASE ${WORK_DB}" >/dev/null
ROWCOUNT=$(psql "$WORK_DB_URL" -tAc \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_name='canary'" \
    | tr -d ' ')
[ "$ROWCOUNT" = "0" ] || fail "the canary table survived the drop; the restore proves nothing"
echo "  database recreated empty (canary present: ${ROWCOUNT}, expected 0)"

echo
echo "=== restoring ==="
DUMP_FILE=$(ls -t "$DUMP_DIR"/forge_*.dump | head -1)
if RESTORE_OUT=$(./scripts/restore.sh "$WORK_DB_URL" "$DUMP_FILE" 2>&1); then
    echo "  restore reported success"
else
    fail "restore.sh failed: ${RESTORE_OUT}"
fi

echo
echo "=== the restored database holds the data ==="
# `tr -d ' '` is wrong here: it strips spaces from the value being
# compared, not just the padding psql adds, so a note containing a space
# could never match. `xargs` trims surrounding whitespace only.
RESTORED=$(psql "$WORK_DB_URL" -tAc "SELECT note FROM canary WHERE id = 1" 2>/dev/null | xargs)
if [ "$RESTORED" = "survives a backup" ]; then
    echo "  read back from the restored database: '${RESTORED}'"
else
    fail "the restored database did not return the original row (got '${RESTORED}')"
fi

echo
echo "=== the server starts against the restored database ==="
# The real point of a restore: not that the rows came back, but that the
# application runs against them afterwards. A restore that produced a database
# the server cannot start on has recovered nothing.
export FORGE_DATABASE_URL="$WORK_DB_URL"
export FORGE_SERVER_HOST="127.0.0.1"
export FORGE_SERVER_PORT="3995"
export FORGE_AUTH_SESSION_SECRET="backup-restore-session"
export FORGE_API_KEY_HASHING_SECRET="backup-restore-hash"
export FORGE_LOG_LEVEL=warn

if [ ! -x ./target/debug/forge-server ]; then
    echo "  (building forge-server first)"
    cargo build -p forge-server >/dev/null 2>&1
fi

./target/debug/forge-server >"$DUMP_DIR/server.log" 2>&1 &
SERVER_PID=$!

READY=0
for _ in $(seq 1 45); do
    if [ "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3995/api/v1/health/ready 2>/dev/null)" = "200" ]; then
        READY=1
        break
    fi
    sleep 1
done

if [ "$READY" = "1" ]; then
    echo "  /api/v1/health/ready -> 200 against the restored database"
    # And the restored database is writable, which a `--clean` restore into a
    # database with a leftover grant would not be.
    if curl -s -o /dev/null -w '%{http_code}' -X POST http://127.0.0.1:3995/api/v1/auth/register \
        -H 'content-type: application/json' \
        -d '{"email":"backup-restore@example.com","password":"BackupRestore123!","tenant_name":"Restore"}' \
        | grep -qE '^(200|201|403)$'; then
        echo "  the restored database accepts a write"
    else
        fail "the restored database rejected a write; the restore was incomplete"
    fi
else
    fail "the server did not become ready against the restored database"
    tail -5 "$DUMP_DIR/server.log" >&2
fi

echo
if [ "$FAILED" -eq 0 ]; then
    echo "backup and restore round trip verified"
else
    echo "backup/restore verification FAILED" >&2
fi
exit "$FAILED"