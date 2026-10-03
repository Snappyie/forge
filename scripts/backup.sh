#!/bin/bash
set -e

# Usage: ./backup.sh <db_url> <backup_dir>
DB_URL=$1
BACKUP_DIR=$2

if [ -z "$DB_URL" ] || [ -z "$BACKUP_DIR" ]; then
  echo "Usage: ./backup.sh <db_url> <backup_dir>"
  exit 1
fi

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/forge_backup_${TIMESTAMP}.sql"

echo "Creating backup at ${BACKUP_FILE}..."
pg_dump "${DB_URL}" -F c -f "${BACKUP_FILE}"
echo "Backup successful!"
