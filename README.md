# Orbital

A directory of makers, sorted by what people actually click.

Live: **https://orbital.polarisocial.xyz** — one directory. `nsfw` is a vertical like `gaming`
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

`sqlx::test` gives each test its own database, so the tests run against the real
schema, the real migrations and a real Postgres.

The suite is built so that a broken layer fails a test that names it: HTTP
behaviour in `tests/http.rs`, the SQL underneath in `tests/store.rs`, and
per-rule unit tests in `src/ranking.rs` and `src/routes/public.rs`. Each of those
was verified by breaking it and watching the right test go red.

## Deploy

```bash
./scripts/deploy-thinkcentre.sh     # build, install unit + nginx, migrate, seed, verify
./scripts/footers-thinkcentre.sh    # put the Orbital footer on every SFW site
./scripts/populate-accounts.py      # links + profile for the live accounts
```

The deploy script owns the nginx site file and the systemd unit, so a hand edit
survives only until the next run.

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

## How it is put together

```
src/
  main.rs        CLI: migrate | seed | serve | bot
  config.rs      environment, and the NSFW-host derivation
  auth.rs        sessions, cookies, the visitor id
  store.rs       every query, in one place
  ranking.rs     top / trending / similarity / ring, pure functions
  text.rs        slugs, tags, URL safety, truncation
  render.rs      layout data, escaping, tracked-link building
  routes/
    public.rs    pages a crawler sees
    api.rs       JSON for the dashboard
    track.rs     the click recorder
    assets.rs    embedded static + the dashboard bundle
  bot.rs         optional Bluesky spotlight poster
  seed.rs        the vertical catalogue and the owner accounts
templates/       askama, compiled at build time
web/             the Svelte 5 dashboard
docs/SPEC.md     the specification, including what the first draft got wrong
docs/PLAN.md     the build order it was implemented from
docs/DECISIONS.md  the choices that are expensive to reverse, and what was rejected
docs/LAUNCH.md   deployment record, social posts, what is deliberately unlisted
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
