# Orbital

A directory of makers, sorted by what people actually click.

Live: **https://orbital.polarisocial.xyz** · state and next steps: [`docs/ROADMAP.md`](docs/ROADMAP.md) — one directory. `nsfw` is a vertical like `gaming`
(DECISIONS D9): the platform stores links, never media, so the destination site owns age
verification. `orbitalnsfw.polarisocial.xyz` 301s to the same path here, so shared links keep working.

Every member gets a profile with their links and taste tags, one **primary
vertical** (the leaderboard they live on) and up to two secondaries (ranked at
0.7×). Each vertical has its own Top / Trending / New / Curated lists, its own
webring, its own JSON feed, and an embeddable leaderboard widget that anyone can
drop on their own site.

## Why it exists

The north star is **the number of people who discover a member's profile** — not
signups, not page views.

That single sentence decides the shape of the thing:

- **Public pages are server-rendered.** Askama templates, no client rendering,
  real HTML with real `<title>`s, JSON-LD, a sitemap and per-vertical feeds.
  A directory that search engines cannot read is not a directory.
- **Ranking counts distinct visitors, not clicks.** A click loop is ten lines of
  `curl`; a session farm is not. Clicks are still stored and shown next to the
  visitor count.
- **Every outbound link is attributed.** A profile link points at
  `/l/<id>?s=<surface>&c=<vertical>`, which records the click and redirects. The
  member's dashboard can then answer "which vertical sent these people" instead
  of "you had 412 clicks".
- **Embeds are funnels.** Each vertical's leaderboard can be embedded anywhere,
  and it carries a Join button. A visitor who arrives from an embed is already
  attributed.
- **The dashboard shows rank, not vanity.** "3rd of 47 in Developers, #2 on
  trending this week" is a decision. A lifetime click count is trivia.

## Stack

| Layer | Choice |
|---|---|
| HTTP | Axum 0.8, one binary |
| Templates | Askama 0.16, compiled into the binary |
| Data | PostgreSQL via SQLx (runtime-checked queries), `pg_trgm` |
| Auth | Argon2id passwords, opaque session cookie, no email, no OAuth |
| Dashboard | Svelte 5 + Vite, static bundle, **embedded in the binary** |
| Assets | `rust-embed`; nothing on disk to go stale |

There is no workspace, no second crate, no node process at runtime.

## Run it

```bash
createdb orbital
export DATABASE_URL=postgres://$USER@127.0.0.1:5432/orbital
cargo run -- migrate
cargo run -- seed        # prints two owner passwords — save them
cargo run -- serve       # http://127.0.0.1:8090
```

The dashboard bundle is built separately and embedded at compile time:

```bash
cd web && npm ci && node node_modules/vite/bin/vite.js build && cd ..
cargo build --release      # now embeds web/dist
```

## Test

```bash
export DATABASE_URL=postgres://$USER@127.0.0.1:5432/postgres   # needs CREATEDB
cargo test
./scripts/smoke.sh                                            # against a running server
```

CI runs the same suite on both forges (`.github/workflows/test.yml`, `.forgejo/workflows/test.yml`)
against a Postgres service. That is not ceremony: a brace lost in an edit once nested five ranking
tests inside another test function, so they compiled, never ran, and the count looked plausible.

`sqlx::test` gives each test its own database, so the tests run against the real
schema, the real migrations and a real Postgres.

The suite is built so that a broken layer fails a test that names it: HTTP
behaviour in `tests/http.rs`, the SQL underneath in `tests/store.rs`, and
per-rule unit tests in `src/ranking.rs` and `src/routes/public.rs`. Each of those
was verified by breaking it and watching the right test go red.

## Operator commands

```bash
orbital doctor                        # database, migrations, healthz, backup age, link checker,
                                      # uptime stamp, visitor growth; exit code = failures
orbital import --handle ana --file links.txt [--dry-run]   # "url | label" per line, idempotent
orbital snapshot                      # today's rank for everyone (weekly timer)
orbital fetch-titles --limit 200      # page titles + favicons (daily timer)
orbital og --out static/og            # social cards; PNG needs rsvg-convert
orbital retag [--dry-run]             # apply the reviewed tag_aliases table
orbital prune --days 180              # forget quiet anonymous visitors (weekly timer)
orbital check-links --limit 500       # link health (daily timer)
```

`doctor` is the one to run after a deploy and when something feels wrong: it is the runbook as
code, it only reads, and it says PASS or FAIL per check rather than dumping a log.

## Deploy

```bash
./scripts/deploy-thinkcentre.sh     # build, install unit + nginx, migrate, seed, verify
./scripts/footers-thinkcentre.sh    # put the Orbital footer on every SFW site
./scripts/populate-accounts.py      # links + profile for the live accounts
```

The deploy script owns the nginx site file and the systemd user units, so a hand edit survives only
until the next run. It also generates `static/estate.txt` from the nginx `server_name`s (the status
page lists what nginx actually serves), renders the social cards, and runs `orbital doctor` as its
last step.

Seven timers, all user units: checklinks (daily 04:20), backup (nightly 03:40), restore-test
(monthly, 1st), prune (Sun 03:20), snapshot (Sun 03:50), metadata + cards (daily 05:10), uptime
probe (every 10 minutes, writes `~/backups/orbital/uptime.txt` for `doctor` to read).

## Config

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | Postgres DSN |
| `BIND_ADDR` | default `127.0.0.1:8090` |
| `PUBLIC_BASE` | absolute SFW origin, e.g. `https://orbital.polarisocial.xyz` |
| `SESSION_SALT` | mixed into the visitor hash |
| `SECURE_COOKIES` | `1` in production |
| `HUB_HANDLE` | the hub's own account |
| `BSKY_PDS`, `BSKY_IDENTIFIER`, `BSKY_PASSWORD` | only for the optional spotlight bot |
| `ORBITAL_ESTATE_FILE` | default `static/estate.txt`; what `/status` probes |
| `ORBITAL_OG_DIR` | default `static/og`; where `orbital og` writes cards |
| `ORBITAL_BACKUP_DIR` | default `~/backups/orbital`; what `doctor` inspects |
| `ORBITAL_TOKEN` | read by the MCP server when a write tool is added |

## How it is put together

```
src/
  main.rs        CLI: serve | migrate | seed | doctor | import | snapshot | retag |
                 fetch-titles | og | prune | check-links | bot
  config.rs      environment, and the NSFW-host derivation
  auth.rs        sessions, cookies, the visitor id
  store.rs       every query, in one place
  ranking.rs     top / trending / similarity / ring, pure functions
  text.rs        slugs, tags, URL safety, truncation
  render.rs      layout data, escaping, tracked-link building
  routes/
    public.rs    pages a crawler sees
    api.rs       JSON for the dashboard, and the admin/governance routes
    apiv1.rs     the public read API
    tokens.rs    API tokens and the write API they unlock
    prefs.rs     the tuner, /lens, the language switch
    estate.rs    /status and /og/{handle}
    track.rs     the click recorder
    assets.rs    embedded static + the dashboard bundle
  governance.rs  proposals, quorum, merges, endorsements
  i18n.rs        the dictionary and the language decision
  meta.rs        page title / favicon fetching, with the same SSRF guard
  og.rs          the social card, as SVG
  ops.rs         doctor, import, snapshot, retag, fetch-titles, og
  bot.rs         optional Bluesky spotlight poster
  seed.rs        the vertical catalogue and the owner accounts
templates/       askama, compiled at build time
web/             the Svelte 5 dashboard
docs/SPEC.md     the specification, including what the first draft got wrong
docs/SPEC-V3.md  personalisation, which supersedes the ranking section of SPEC.md
docs/PLAN.md     the build order it was implemented from
docs/PLAN-V3.md  the implementation plan for personalisation
docs/ROADMAP.md  what is done, what is next, and what is deliberately not done
docs/DECISIONS.md  the choices that are expensive to reverse, and what was rejected
docs/LAUNCH.md   deployment record, social posts, what is deliberately unlisted
docs/monetization.md  where an ad slot would go, and the rules it would have to obey
```

## Link health

`orbital check-links --limit 500` walks every live link, oldest-checked first, and records the
answer in `last_status` / `last_checked` / `fail_count`. The profile shows a muted card with the
reason at three consecutive failures, and a success clears it. `deploy-thinkcentre.sh` installs
`orbital-checklinks.timer` (daily, 04:20 with a 30-minute jitter).

Private, loopback, link-local and cloud-metadata addresses are refused, including on redirect
hops, because member-supplied URLs would otherwise point the checker at this host.

## Referrer attribution

A click records the host of its `Referer`, never the path, and never our own hosts.
`GET /api/analytics` returns `by_referrer`, which is the only way to tell whether an announcement
on one network did anything.

## Personalisation (SPEC v3)

Every visitor — signed in or not — can say what they want to discover, and that lens reorders Top,
Trending, New, the similar-profiles sidebar and the webring step for them alone. It is stored as
`tag → weight` per member or per anonymous visitor, explicit weights never decay, passive ones
(browsing) fade with a 30-day half-life, and passive personalisation does not engage until the fifth
page view.

```
GET  /            the tuner sits above the vertical grid
POST /prefs       tag=<slug>&w=<0..10> pairs, plus new_tag/new_w; redirects back
POST /prefs/reset clears the explicit weights
GET  /lens        the weights as an editable table: what you actually said, in words
GET  /prefs/off   turn personalisation off for this browser; /prefs/on turns it back on
```

A lens holds at most 24 weights, strongest first — the rate limiter bounds requests, not size. A
`v:music` key is a category and is validated against the vertical table (a tag key may not contain a
colon; that is what `normalise_tag` enforces). Signing up adopts the anonymous lens, so the tuning
you did before registering is not thrown away at the moment it becomes worth keeping.

The model is in `docs/SPEC-V3.md`, the implementation plan in `docs/PLAN-V3.md`, the storage decision
in `docs/DECISIONS.md` D14, and why it is a plain form in D15.

## Public read API

`/api/v1` is anonymous for every read and returns what the pages show — nothing more. The three
`/api/v1/me/*` routes are the exception: they are yours, and they need a token (next section).

```
GET /api/v1/stats                      totals, plus link counts (total / failing / unchecked)
GET /api/v1/verticals                  every category with its member count
GET /api/v1/members?vertical=&q=&limit=&offset=
GET /api/v1/members/{handle}           profile, tags, Bluesky claim (+ verified), links
GET /api/v1/members/{handle}/links     links with last_status / last_checked / failing
GET /api/v1/links/health               links that failed 3+ checks, worst first
GET /api/v1/ring/{slug}?from=handle    the webring walk, as JSON
```

Read responses carry a weak `ETag` (send it back in `If-None-Match` for a 304) and
`Access-Control-Allow-Origin: *`, and deliberately **no `Set-Cookie`**: a response that sets a cookie
is uncacheable at nginx and at Cloudflare, which made the advertised `max-age=60` a lie and turned
every scrape into an origin hit. nginx limits the API to 60 requests/minute per client (keyed on
`CF-Connecting-IP`, since `$remote_addr` is a Cloudflare edge address for every visitor) with a
`Retry-After` on 429.

## MCP server

`mcp/orbital_mcp.py` exposes that API as seven MCP tools (`orbital_stats`, `orbital_verticals`,
`orbital_members`, `orbital_profile`, `orbital_links`, `orbital_broken_links`, `orbital_ring`).
It speaks HTTP, not SQL, so it needs no database credentials and cannot drift from the app's line
between public and private. The seven tools are read-only; the API they call is not. Writing needs
a member token (`POST /api/me/tokens`), and the MCP server has not been extended with write tools
yet — that is the remaining half of the "script your own links" story, and the API side is done.

```bash
# one-time, on the machine that will run it
python3 -m venv ~/.hermes/profiles/coding/mcp-venvs/orbital
~/.hermes/profiles/coding/mcp-venvs/orbital/bin/pip install "mcp>=2"

hermes mcp add orbital \
  --command ~/.hermes/profiles/coding/mcp-venvs/orbital/bin/python \
  --args /absolute/path/to/mcp/orbital_mcp.py
```

`ORBITAL_BASE` overrides the origin (default `https://orbital.polarisocial.xyz`).

## The write API, and tokens

Reading needs no credential. Writing does, and a browser session was the wrong credential for a
script, so:

```
POST   /api/me/tokens            session-auth; mints orb_… once and stores only a salted hash
GET    /api/me/tokens            labels, created/last-used (never the token)
POST   /api/me/tokens/{id}/revoke
GET    /api/v1/me/links          Bearer orb_…; your links, hidden ones included
POST   /api/v1/me/links          {"url": "...", "label": "...", "vertical": "dev"} → 201
DELETE /api/v1/me/links?url=…    yours only
```

Adding a link fetches its title and favicon in the background, so the caller does not wait on a
stranger's web server, and the card looks better a moment later. Ten live tokens per member, forty
links per member, both refused with a sentence rather than a shrug.

`GET /api/me/rank-delta` answers "where was I a week ago" from the weekly snapshots, which is the
dashboard sentence worth having ("9 → 3 in Developers").

## Governance

The taxonomy is not only the operator's:

```
GET  /api/verticals/proposals              public; quorum and tally
POST /api/verticals/proposals              any member: slug, label, description, merged_into?
POST /api/verticals/proposals/{id}/vote    TL3; three votes accept it, in the voting transaction
POST /api/admin/proposals/{id}/decide      owner: accept or reject without waiting
POST /api/tags/{tag}/endorse               TL2+, toggles
```

A merge moves members to the target with their tags, re-points their links, and deletes the retired
vertical. An endorsement adds up to +0.2 to the relevance term (capped at three endorsements) as a
separate `0.4 × 0.2` term in the score — not inside `relevance`, which saturates at 1.0 for exactly
the members it was meant to reward. Clicks stay weighted 0.6, so endorsements refine a lens; they do
not replace attention.

## Status page and social cards

`/status` lists every hostname nginx serves — the deploy generates `static/estate.txt` from
`server_name`s — with a per-host HTTP code and latency, cached for a minute and never served from a proxy cache.
It exists because "is the estate up" was a question that only had an answer if someone asked me.

`/og/{handle}.png` is the member's social card, generated from the same data the profile uses, 1200×630,
with the page's own title and favicon. `orbital og` renders them ahead of time; the route renders on
demand when the file is missing, so the first share is never a broken card.

## Language

The site reads in English and Spanish. The choice is a cookie (`/lang/es` sets it, `/lang/en` clears
it), the first visit falls back to `Accept-Language`, and templates ask for keys (`layout.t("…")`)
rather than carrying strings. `src/i18n.rs` holds the table and two tests guard it: every key a
template uses must exist, and no entry may be empty or left in English by accident.

The chrome (nav, footer, call to action, skip link) is translated; the page bodies are the remaining
mechanical pass, listed in `docs/ROADMAP.md`.

## Backups

`orbital-backup.timer` dumps to `~/backups/orbital` nightly (14 kept, written to `.part` first).
`orbital-restore-test.timer` runs on the 1st of the month: it creates a scratch database from the
newest dump, counts users/links/verticals and the migration version, checks the live database is
still intact, and drops the scratch. A dump nobody has restored is a file.

```bash
ssh thinkcentre 'systemctl --user start orbital-restore-test.service'
```

## The adult vertical

Segregation is a property of the request, not of a template: `Host` decides the
surface in one middleware, before any handler runs. On the adult host the SFW
listings do not exist; on the SFW host the adult profiles return 404 and never
appear in a leaderboard, a search result, a feed or the sitemap. A member with a
SFW primary and an adult secondary still ranks on their SFW vertical — their SFW
card simply says nothing about the other listing.

## Licence

MIT.
