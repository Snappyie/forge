#!/usr/bin/env bash
#
# Restores a Forge database from a backup.
#
#   ./scripts/restore.sh <db_url> <backup_file>
#
# Restores into an existing database. `-c` drops the schema first, so a restore
# is a replacement rather than a merge: restoring on top of a populated database
# would leave objects from the newer schema behind, which is exactly the state a
# restore is meant to escape.
set -euo pipefail

DB_URL="${1:-}"
BACKUP_FILE="${2:-}"

if [ -z "$DB_URL" ] || [ -z "$BACKUP_FILE" ]; then
    echo "Usage: ./restore.sh <db_url> <backup_file>" >&2
    exit 2
fi

# Same reasoning as the client selection in backup.sh.
PG_RESTORE="${FORGE_PG_RESTORE:-$(command -v pg_restore 2>/dev/null || true)}"
if [ -z "${PG_RESTORE}" ] || [ ! -x "${PG_RESTORE}" ]; then
    echo "pg_restore not found; install the PostgreSQL client tools or set FORGE_PG_RESTORE" >&2
    exit 3
fi

if [ ! -f "$BACKUP_FILE" ]; then
    echo "no such backup: ${BACKUP_FILE}" >&2
    exit 4
fi

echo "Restoring ${BACKUP_FILE} into the database behind ${DB_URL}..."
# `-1` restores in a single transaction: either the whole restore lands or none
# of it does. A partially restored database is indistinguishable from a corrupt
# one until something fails at runtime.
"${PG_RESTORE}" --dbname="$DB_URL" --single-transaction --clean --if-exists "$BACKUP_FILE"

echo "Restore complete."