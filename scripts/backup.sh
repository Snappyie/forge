#!/usr/bin/env bash
#
# Backs up a Forge database.
#
#   ./scripts/backup.sh <db_url> [backup_dir]
#
# Writes a custom-format dump, which `pg_restore` can restore selectively and
# which compresses by default. The dump is a single file so a backup can be
# copied off the host without a second step.
set -euo pipefail

DB_URL="${1:-}"
BACKUP_DIR="${2:-./backups}"

if [ -z "$DB_URL" ]; then
    echo "Usage: ./backup.sh <db_url> [backup_dir]" >&2
    exit 2
fi

# A client older than the server refuses outright ("server version mismatch"),
# which is the right refusal: a dump it cannot write is not a backup. So the
# client is overridable - a machine whose PATH `pg_dump` predates its server needs
# to point at a matching one, and failing with Postgres' own message is better than
# silently producing nothing.
PG_DUMP="${FORGE_PG_DUMP:-$(command -v pg_dump 2>/dev/null || true)}"
if [ -z "${PG_DUMP}" ] || [ ! -x "${PG_DUMP}" ]; then
    echo "pg_dump not found; install the PostgreSQL client tools or set FORGE_PG_DUMP" >&2
    exit 3
fi

# Created rather than assumed. Without this the dump fails with "could not open
# output file", which reads like a permissions problem rather than a missing
# directory - and the first run on a fresh checkout always hit it.
mkdir -p "$BACKUP_DIR"

TIMESTAMP=$(date -u +%Y%m%dT%H%M%SZ)
BACKUP_FILE="${BACKUP_DIR}/forge_${TIMESTAMP}.dump"

echo "Backing up to ${BACKUP_FILE}..."
"${PG_DUMP}" "$DB_URL" --format=custom --file="$BACKUP_FILE"

# A dump file that exists but is empty is worse than no backup: it looks like
# one, and it is only discovered during a restore, which is the worst time.
if [ ! -s "$BACKUP_FILE" ]; then
    echo "the dump at ${BACKUP_FILE} is empty; treating this as a failure" >&2
    rm -f "$BACKUP_FILE"
    exit 4
fi

SIZE=$(wc -c <"$BACKUP_FILE" | tr -d ' ')
echo "Backup complete: ${BACKUP_FILE} (${SIZE} bytes)"