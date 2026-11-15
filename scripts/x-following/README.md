# Seeding Orbital from an X account's follows

The four steps that filled the directory with 2,130 members on 2026-11-15, kept because a roster
like this gets refreshed, not re-invented:

```bash
./harvest.py ~/Downloads/cookies.txt following.jsonl   # 2,194 follows, cookie auth, no API key
./enrich.py                                            # expand linktr.ee/allmylinks/beacons, resolve t.co
./build.py                                             # records.json + stage.csv (classify, tags, links)
scp stage.csv thinkcentre:/tmp/orbital-stage.csv
ssh thinkcentre 'sudo -u postgres psql -d orbital -f /tmp/orbital-import.sql'   # idempotent
```

`import.sql` reads `/tmp/orbital-stage.csv`, upserts members/verticals/tags/links and never
duplicates: re-running it after a re-harvest adds the new follows and updates nothing else.

`rollback.sql` + `orbital-undo.csv` (the handle list) remove exactly what an import created, and
only members nobody has logged into or claimed.

Two rules the scripts encode, both learned the hard way:

* handles are normalised to the site's grammar (`_` → `-`), because `users_handle_shape` refuses
  underscores; the exact X handle stays in the profile's link URL.
* an account is `nsfw` unless nothing adult matched at all — the expensive mistake is an adult
  account in a SFW vertical, not the reverse.
