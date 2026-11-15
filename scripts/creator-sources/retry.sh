#!/usr/bin/env bash
# A slow second pass over the link-in-bio pages that answered nothing.
#
# linktr.ee serves cached profiles to anyone and answers 429 to a burst of uncached
# ones, so a first pass with eight workers reads about 45% of the pages it asks for.
# The block is soft and decays: a small batch every few hours recovers the rest without
# asking for a ban. A page that answers nothing is recorded as `null` and retried again
# on the next run, so nothing is lost by stopping early.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"

BATCH="${BATCH:-250}"
python3 enrich.py --stage stage-full.csv --out enrich.json \
    --retry-nulls --limit "$BATCH" --workers 1
python3 merge.py --stage stage-full.csv --enrich enrich.json --out merge.csv
if [ -s merge.csv ]; then
  set -a; . "$HOME/.config/systemd/user/orbital-db.env"; set +a
  psql "$DATABASE_URL" -f merge.sql | grep -E '^(INSERT|COMMIT)'
fi
