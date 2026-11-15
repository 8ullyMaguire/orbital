# Creator sources — folding a registry into the directory

Orbital's roster began as the accounts you followed on X. This pipeline widens it to the closest
thing the adult web has to an authoritative performer registry: **StashDB** and **FansDB**, the two
public stash-box instances, plus whatever socials each performer's record points at.

    sweep.py   → new_<site>.jsonl     read the instance (full or since a cursor)
    build.py   → stage-creators.csv   decide who is a creator, what their profile says
    import.sql → orbital              write rows once, idempotently
    crawl.py   → all of the above     what the weekly timer runs

## The one rule that decides who gets a profile

A performer is imported when their record carries **at least one surface of their own** — an X or
Instagram handle, OnlyFans, Fansly, ManyVids, TikTok, Reddit, a link-in-bio page, a personal site.

IAFD, Boobpedia, IMDb, Data18, ThePornDB and studio pages are excluded on purpose: they are evidence
that a person exists, not a place to send a reader. Of 133,352 performer records, 86,534 have nothing
but those, and they are not imported. That single rule is the difference between a directory and a
dump of a database.

## Identity, and the bug that rule found

Deduping against Orbital means comparing link identities, not names. The first cut treated the first
path segment of any link as a handle, so `manyvids.com/profile` and `reddit.com/user` matched each
other and 15,132 performers looked like they were already on the site. The real overlap is 2,001.
Route words (`profile`, `user`, `u`, `p`, `add`, `model`, …) are in `ROUTE_WORDS` and produce no
identity at all.

## Running it

    # the weekly run: read only what changed since the cursor
    ./crawl.py

    # the first run, or after a schema change: read everything
    ./crawl.py --full

    # decide, report, write nothing
    ./crawl.py --dry-run

    # one instance, ad hoc
    ./sweep.py --site stashdb --out all.jsonl --workers 10
    ./build.py --inputs stashdb:all.jsonl --stage stage-creators.csv
    sed "s|__STAGE__|$PWD/stage-creators.csv|" import.sql > /tmp/import.sql
    psql "$DATABASE_URL" -f /tmp/import.sql

`crawl.py` needs `DATABASE_URL` (falls back to `~/.config/systemd/user/orbital-db.env`) and, for the
API keys, `~/.config/orbital-creators.env` holding `STASHDB_KEY` and `FANSDB_KEY`. The keys are
deliberately **not** in this repo and not in the systemd unit; `sweep.py --env-file` points at them.

The cursor is `~/.local/state/orbital-creators.json`, one `updated` stamp per instance. It decides how
much of the source to read; `build.py`'s identity check decides what is actually new. Neither alone is
enough — the cursor would re-read nothing after a failed import, and identity alone would need a full
sweep every week.

## What it writes, and how to take it back

Members get: handle (the X handle when there is one), display name, a factual bio built only from
columns the source actually has, avatar (StashDB image, hotlinked), the `nsfw` vertical, and tags
derived from the links present (`onlyfans`, `instagram`, `amateur`, `pornstar`, `trans`, …). Every
profile also links back to its source record, so a reader can see where it came from.

`import.sql` is idempotent — `NOT EXISTS` guards on members, verticals, tags and links, so a second
run inserts nothing. To reverse a run, delete the handles listed in the staging CSV:

    cut -d, -f1 stage-creators.csv > /tmp/handles
    psql "$DATABASE_URL" -c "\copy (SELECT 1) TO /dev/null"   # no-op guard, then:
    psql "$DATABASE_URL" -c "DELETE FROM users WHERE handle IN (SELECT handle FROM tmp_handles)"

Only imported rows are touched: nothing here sets `password_hash`, so a claimed profile that a real
person later takes over is never deleted by a re-run.
