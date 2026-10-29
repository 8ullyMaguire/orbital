# Orbital — PLAN

Sequenced so each phase ends in something observable. Dev DB on this host is a
throwaway cluster at `127.0.0.1:5498` (`pg_ctl -D ~/code-local/pg/orbital-dev`);
the deployed DB is `orbital` on thinkcentre's system cluster.

Crate layout (single binary, no workspace — nothing here needs a second crate):

```
Cargo.toml
migrations/0001_init.sql
src/main.rs            clap: serve | migrate | seed | bot | rehash
src/config.rs          env: DATABASE_URL, BIND_ADDR, PUBLIC_BASE, NSFW_BASE, ...
src/error.rs           AppError -> IntoResponse
src/db.rs              pool + migrations
src/auth.rs            argon2, sessions, CurrentUser extractor, cookie
src/models.rs          row structs + DTOs
src/render.rs          askama templates (public pages), escaped html helpers
src/templates/*.html   base, home, vertical, profile, search, embed, gate, 404
src/ranking.rs         top/trending/similar/ring (pure fns + SQL)
src/routes/mod.rs      router
src/routes/public.rs   /, /v/*, /@*, /search, feeds, sitemap, robots, gate
src/routes/api.rs      /api/* (auth, me, links, analytics, flags, admin)
src/routes/track.rs    /l/:id
src/routes/assets.rs   embedded SPA + static
src/bot.rs             bluesky spotlight + roundup
src/seed.rs            starter verticals + accounts
web/                   Svelte 5 dashboard (vite), built to web/dist
deploy.sh              sync, build, install unit, nginx, verify
```

---

## Phase 0 — skeleton that boots

Files: `Cargo.toml`, `src/main.rs`, `src/config.rs`, `src/db.rs`, `src/error.rs`,
`migrations/0001_init.sql`.

Deps pinned: axum 0.8, askama 0.16 + askama_web 0.16 (`axum-0.8` feature — probe
compiled), sqlx 0.8 (postgres, runtime-tokio-rustls, uuid, chrono, macros), clap 4,
tracing, argon2 0.5, sha2, uuid, chrono, rand, rust-embed 8, tower-http.

Verify:
```
cargo build 2>&1 | tail -3          # Finished `dev` profile
DATABASE_URL=postgres://alvaro@127.0.0.1:5498/orbital cargo run -- migrate
psql -h 127.0.0.1 -p 5498 -U alvaro orbital -tAc 'select count(*) from verticals'  # >= 10
```

## Phase 1 — auth and profile CRUD

argon2id hash; `sessions` rows; `CurrentUser` extractor reads the cookie and
loads the user; register/login/logout; PATCH /api/me; links CRUD + reorder;
tags replace.

Verify:
```
curl -si -X POST localhost:8090/api/auth/register -H 'content-type: application/json' \
  -d '{"handle":"tester","password":"x","display_name":"T","vertical":"dev"}'   # 201 + Set-Cookie
curl -s localhost:8090/api/me -b "$JAR"                                        # the profile JSON
```
Unit tests: handle normalisation, password round-trip, cookie flags.

## Phase 2 — public pages, real HTML

askama templates: base + home + vertical + profile + search + 404. Every page
has title/description/canonical/OG/JSON-LD. Links render through `/l/{id}?...`.

Verify:
```
curl -s localhost:8090/ | grep -c 'orbital-card'        # > 0
curl -s localhost:8090/@tester | grep -o '<title>[^<]*'  # <title>Tester — Orbital
curl -s localhost:8090/v/dev | grep -c 'trending'        # tab absent when empty
```

## Phase 3 — tracking + rankings

`/l/{id}` records a `source_hits` row keyed on a session hash and 302s.
`/u/{handle}` render records `profile_views` (deduped per day). Top/Trending/New/
Curated queries in `ranking.rs`, with the aggregate cast `::bigint` in SQL.

Verify:
```
curl -si -b "$JAR" 'localhost:8090/l/<id>?s=vertical&c=dev&p=1' | head -3   # 302 + Location
psql -tAc 'select count(*) from source_hits'                               # 1
psql -tAc "select unique_visitors from ... "                               # 1, not 0
```
Tests: one session clicking 5× counts once; two sessions count twice; trending
is empty (tab hidden) with no traffic; member with traffic outranks one with none.

## Phase 4 — discovery plumbing

Sitemap, robots, JSON/RSS feeds (global + per vertical), embed widget, ring
footer (vertical + global), similar-profiles sidebar, Bluesky claim, flags.

Verify:
```
curl -s localhost:8090/sitemap.xml | grep -c '<url>'       # matches members+verticals
curl -s localhost:8090/feed.json | python3 -m json.tool | head
curl -s 'localhost:8090/v/dev/embed?view=top&count=5' | grep -c 'Join'
```

## Phase 5 — NSFW host + age gate

Host header selects the surface. `nsfw.*` → gate (POST sets cookie, 30 days) →
directory of nsfw members only. No nsfw vertical or member URL in the SFW
sitemap/feeds/rails; `X-Robots-Tag: noindex, nofollow` on every response.

Verify:
```
curl -s -H 'Host: nsfw.links.polarisocial.xyz' localhost:8090/@steeltitan | grep -c '18'  # gate
curl -sI -H 'Host: nsfw.links.polarisocial.xyz' localhost:8090/ | grep -i robots           # noindex
curl -s localhost:8090/sitemap.xml | grep -c nsfw                                          # 0
```

## Phase 6 — Svelte dashboard

`web/` Svelte 5 + vite, `base: '/app/'`, history routing, embedded via
`rust-embed` (fallback to `index.html`). Pages: login, join, profile editor
(links drag-order, tags, verticals), analytics, admin (flags, users, verticals,
curation). Served at `/app/`.

Verify:
```
cd web && node node_modules/vite/bin/vite.js build     # dist/index.html exists
curl -s localhost:8090/app/ | grep -c '<div id="app">'
cargo build   # assets re-embedded
```
Browser check: register in a real browser, add a link, see it on the public page.

## Phase 7 — bot, seeds, deploy

`orbital bot spotlight|roundup` posts to Bluesky (app password from env file),
records in `bot_posts`, refuses to repeat within N days. `orbital seed` creates
verticals + owner accounts idempotently. `deploy.sh` builds here, rsyncs, installs
the unit and nginx site, restarts, and verifies every route against the loopback.

Verify:
```
./deploy.sh                       # prints each route check, all 200/short bodies
curl -s https://links.polarisocial.xyz/ | grep -o '<title>[^<]*'
systemctl --user is-active orbital                                # active
psql -tAc "select count(*) from users"                            # 2
```

## Phase 8 — handover

Footer links on every live SFW site; Bluesky announcement + pin (SFW and NSFW);
credential block in `~/.hermes/.env`; docs committed; SecondBrain note.

---

## Definition of done

1. `cargo test` green on this host (unit + integration against 5498).
2. `./deploy.sh` prints OK for every route and the service is `active`, `NRestarts=0`.
3. `https://links.polarisocial.xyz/@criticalnexus` renders all 6 live SFW sites as
   links; `https://nsfw.links.polarisocial.xyz/@steeltitan` renders the 6 NSFW ones
   behind the gate and carries `noindex`.
4. A click through a profile link is recorded and visible in that member's
   analytics.
5. Footer of each SFW site links to the profile page, verified by fetching the
   rendered page on the host.
6. Both Bluesky accounts posted and pinned.
