#!/bin/bash
set -e

# Usage: ./restore.sh <db_url> <backup_file>
DB_URL=$1
BACKUP_FILE=$2

if [ -z "$DB_URL" ] || [ -z "$BACKUP_FILE" ]; then
  echo "Usage: ./restore.sh <db_url> <backup_file>"
  exit 1
fi

echo "Restoring backup from ${BACKUP_FILE}..."
pg_restore -d "${DB_URL}" -1 -c "${BACKUP_FILE}"
echo "Restore successful!"
