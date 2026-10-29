#!/usr/bin/env bash
# Prove the orbital backup restores, not just that pg_dump exits 0.
#
# A dump nobody has restored is a file. This creates a scratch database from the newest dump,
# counts the rows that would be missed if the restore were wrong, and drops it again. Run monthly,
# and always before a migration you would rather not repeat.
#
# Why the sudo dance: the application's role owns its database but cannot CREATE DATABASE (it does
# not need to, and granting it would widen what a compromised app can do). The scratch database is
# therefore created by the postgres superuser over the local socket, while the live-database check
# still runs with the application's own credentials.
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

# The scratch database lives on the same server as the source.
BASE=${SRC%/*}
APP_DB=${SRC##*/}

# Can the application's own role create a database? If yes, no privileges are borrowed.
CAN_CREATE=$(psql "$BASE/postgres" -tAc \
  "select coalesce((select rolcreatedb from pg_roles where rolname = current_user), false)" 2>/dev/null \
  | tr -d ' ' || echo f)

if [ "$CAN_CREATE" = "t" ]; then
  AS_ADMIN=""
else
  AS_ADMIN="sudo -n -u postgres"
  echo "note:   role cannot createdb, borrowing postgres over the local socket"
fi

$AS_ADMIN psql -q -d postgres -c "DROP DATABASE IF EXISTS $SCRATCH" >/dev/null
$AS_ADMIN psql -q -d postgres -c "CREATE DATABASE $SCRATCH" >/dev/null
trap '$AS_ADMIN psql -q -d postgres -c "DROP DATABASE IF EXISTS $SCRATCH" >/dev/null || true' EXIT

$AS_ADMIN pg_restore --no-owner --no-privileges -d "$SCRATCH" "$DUMP"
echo "restored into $SCRATCH"

# Counts that must be non-zero for the restore to mean anything: the product is members and links.
fail=0
for table in users links verticals user_verticals source_hits; do
  n=$($AS_ADMIN psql -tA -d "$SCRATCH" -c "select count(*) from $table" | tr -d ' ')
  printf '  %-16s %s\n' "$table" "$n"
  case "$table" in
    users|links|verticals) [ "$n" -gt 0 ] || { echo "FAIL: $table is empty in the restore" >&2; fail=1; } ;;
  esac
done

migrations=$($AS_ADMIN psql -tA -d "$SCRATCH" -c "select coalesce(max(version),0) from _sqlx_migrations" | tr -d ' ')
echo "  migrations       $migrations"
[ "$migrations" -gt 0 ] || { echo "FAIL: no migration history restored" >&2; fail=1; }

# The backup is only half the claim: the live database must still be intact.
live=$(psql "$BASE/$APP_DB" -tAc "select count(*) from links" | tr -d ' ')
echo "  live links       $live"
[ "$live" -gt 0 ] || { echo "FAIL: the live database lost its links" >&2; fail=1; }

[ "$fail" -eq 0 ] && echo "OK: backup restores and holds data"
exit "$fail"
