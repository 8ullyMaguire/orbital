#!/usr/bin/env bash
# Prove the orbital backup restores, not just that pg_dump exits 0.
#
# A dump nobody has restored is a file. This creates a scratch database from the newest dump,
# counts the rows that would be missed if the restore were wrong, and drops it again. Run monthly,
# and always before a migration you would rather not repeat.
set -euo pipefail

DB_ENV="$HOME/.config/systemd/user/orbital-db.env"
SRC=$(grep '^DATABASE_URL=' "$DB_ENV" | cut -d= -f2-)
BACKUP_DIR="$HOME/backups/orbital"
SCRATCH="orbital_restore_test"

DUMP=$(ls -t "$BACKUP_DIR"/*.dump 2>/dev/null | head -1)
if [ -z "${DUMP:-}" ]; then
  echo "FAIL: no dump in $BACKUP_DIR" >&2
  exit 1
fi
echo "dump:   $DUMP ($(du -h "$DUMP" | cut -f1)), $(date -r "$DUMP" '+%F %H:%M')"

# The scratch database is derived from the dump's own DSN so it lands on the same server.
BASE=${SRC%/*}
psql "$BASE/postgres" -q -c "DROP DATABASE IF EXISTS $SCRATCH" >/dev/null
psql "$BASE/postgres" -q -c "CREATE DATABASE $SCRATCH" >/dev/null
trap 'psql "$BASE/postgres" -q -c "DROP DATABASE IF EXISTS $SCRATCH" >/dev/null || true' EXIT

pg_restore --no-owner --no-privileges -d "$BASE/$SCRATCH" "$DUMP"
echo "restored into $SCRATCH"

# Counts that must be non-zero for the restore to mean anything. Members and links are the product;
# migrations prove the schema came back with the data.
for table in users links verticals user_verticals source_hits; do
  n=$(psql "$BASE/$SCRATCH" -tAc "select count(*) from $table" | tr -d ' ')
  printf '  %-16s %s\n' "$table" "$n"
  if [ "$table" = "vertical" ] || [ "$table" = "verticals" ] || [ "$table" = "links" ] || [ "$table" = "users" ]; then
    [ "$n" -gt 0 ] || { echo "FAIL: $table is empty in the restore" >&2; exit 1; }
  fi
done

migrations=$(psql "$BASE/$SCRATCH" -tAc "select coalesce(max(version),0) from _sqlx_migrations" | tr -d ' ')
echo "  migrations       $migrations"

echo "OK: backup restores and holds data"
