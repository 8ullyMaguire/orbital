# Decisions

Short records of the choices that are expensive to reverse. Each one names the alternative that
was rejected, because the alternative is what a future reader will propose again.

## D1 — Rank by distinct visitors, not by clicks

**Context.** The leaderboard is the product, so whatever it counts is what members will optimise.
A raw click counter costs one `curl` per second to inflate.

**Decision.** Every click writes a `source_hits` row carrying a `session_hash` (an HMAC of the
visitor cookie, not an IP). Top counts *distinct sessions per profile*; Trending counts distinct
sessions inside the trailing window.

**Consequences.** A refresh loop buys nothing, so the cheap attack is gone. A session farm is still
possible — that is what D4's trust multiplier and the proposed anomaly detection address. Clicks
are still stored, and shown beside distinct visitors, because "12 clicks from 3 people" is a
different signal from "12 clicks from 12 people" and both are worth seeing.

## D2 — Two surfaces, one process *(superseded by D9)*

**Context.** Adult profiles must not appear beside all-ages ones. CSS is not a wall: a crawler, a
link preview or a reader without the stylesheet sees everything.

**Decision.** The `Host` header decides the surface before any handler runs. One binary, one port,
one database; `orbital.polarisocial.xyz` serves the SFW directory and `orbitalnsfw.polarisocial.xyz`
serves the adult one behind `/gate`, with `X-Robots-Tag: noindex` and an empty sitemap.

**Consequences.** Nothing to keep in sync, and no way for a page to render the wrong list — the
middleware decides before rendering, so a bug in a template cannot leak. The adult hostname is
`orbitalnsfw.polarisocial.xyz` rather than `nsfw.orbital.polarisocial.xyz` because Cloudflare's
Universal SSL covers one label; the two-level name fails the TLS handshake with no certificate and
no useful error.

**Why it was retired.** Read D9. In short: the directory stores a URL and a label, never media, so
there was no adult content for a wall to hold back — only an adult *category*, which the destination
site already gates. The wall cost a second hostname, a second cookie jar, a split sitemap, and one
real bug (the `X-Robots-Tag` check matched a `nsfw.` prefix and never fired for the real host, so a
crawler could index the surface the middleware believed it was hiding).

## D3 — Server-rendered public pages, SPA only where it earns its weight

**Context.** A directory of links is read on phones, on slow connections, and by unfurlers.

**Decision.** Every public page is HTML from the server (Askama-style rendering in `src/render.rs`,
`templates/*.html`). The only JavaScript application is the member dashboard at `/app/`, and its
assets — like every template and the CSS — are embedded in the binary with `rust-embed`.

**Consequences.** A profile link unfurls correctly, a profile is readable with JS disabled, and
"deployed" means "built": there is no on-disk frontend bundle to go stale. The cost is that
interactive edits need the dashboard; a profile cannot be edited in place.

## D4 — Trending is velocity × recency boost × trust

```
trending = (distinct_visitors_7d / max_in_vertical)
         × (1 + 0.15 × (1 / log2(account_age_days + 2)))
         × trust_multiplier(TL0 0.5, TL1 0.8, TL2 1.0, TL3 1.1)
```

**Context.** An all-time counter is a monument to whoever arrived first; a naive 7-day window is a
monument to whoever arrived first *and* is still posting.

**Decision.** Normalise the window against the vertical's own maximum, add a logarithmic boost that
is meaningful in week one and negligible by month two, and multiply by trust.

**Consequences.** New members can rise, and the boost cannot be farmed into permanence because it
decays. Trust is set by verification (D7), so the multiplier is anchored in something a sockpuppet
cannot manufacture quickly.

## D5 — Verticals are a curated enum; tags are freeform

**Context.** Freeform tags make discovery a spelling problem ("game-dev", "gamedev", "games"), and a
directory whose categories are whatever members typed has no leaderboard worth reading.

**Decision.** Ten starting verticals (`gaming`, `podcasters`, `youtube`, `music`, `art`, `dev`,
`writers`, `vtubers`, `indie-web`, `nsfw`) are a database table with a slug; a profile has one
primary vertical and up to two secondaries. Freeform taste tags still exist (3–10) for the
similar-profiles score, not for navigation.

**Consequences.** Every leaderboard compares like with like. Expanding the list needs a migration or
the proposed TL3 proposal + quorum flow; it is not a form field. The enum is also what makes the
`nsfw` vertical a real boundary rather than a label someone can mistype.

## D6 — One transport for posting, shared with everything else

**Context.** Two implementations of the same API is the bug: they drift, and the one you are not
reading is the one that breaks.

**Decision.** Announcements go out through the `socialite` crate's `socialctl` (Bluesky, Mastodon,
Lemmy, and eight more providers), configured from `~/.config/socialite/accounts.toml`. Orbital
itself never speaks to a social API.

**Consequences.** A credential lives in one place and a provider is written once. When the daily
repost needed a refresh (unrepost + repost, because a repost keeps its original timestamp), the
capability went into the crate rather than into a script beside it.

## D7 — Verification is a token you publish

**Context.** A directory of handles that anyone can claim proves nothing: impersonation costs one
form submission.

**Decision.** A member gets a `verify_token`, puts `orbital-verify:<token>` in their Bluesky bio, and
the server reads the public profile through the AppView to confirm it. Verifying sets the trust
level that D4's multiplier uses.

**Consequences.** Trust is anchored in an identity the member already controls elsewhere, without
Orbital holding an OAuth grant for anyone. The limitation is honest: only Bluesky is verifiable
today, so a self-hosted identity is second class until the proposed domain-claim flow exists.

## D8 — The SFW surface never links an adult property *(narrowed by D9)*

**Context.** The two accounts (`@criticalnexus`, `@steeltitan`) and the two hostnames exist to keep
audiences apart. A single cross-link defeats both.

**Decision.** The SFW profile lists only SFW sites; adult properties live on the adult account and
carry no SFW footer. The check is mechanical: the SFW page's bytes are grepped for the adult
hostnames, and the adult page's for the SFW ones.

**Narrowed by D9.** With one surface, this is a *profile* convention for `@criticalnexus`, not a
rule the platform enforces — a member may list whatever they choose, adult or not. The mechanical
grep still applies to `@criticalnexus`'s page and to the footer script, because that account's whole
value is being safe to open at work.

**Consequences.** Some traffic is left on the table by design. A reader who wants both has to find
them separately, which is the point.

## D9 — One surface: `nsfw` is a vertical, not a wall

**Context.** D2 put adult profiles behind a second hostname and an age gate. The premise was that
adult listings must not sit beside all-ages ones. Two measurements undermined it, and one incident
did:

- The schema has no media column. A link is `url`, `label`, `icon`, `position`. The platform hosts
  no adult content at all, so the gate protected a category name, not a corpus.
- The gate was enforced in one place and *encoded* in five others (`visible()`, `verticals()`,
  `all_members()`, the sitemap, the feeds) — five places for a rule to be forgotten, and one of them
  already was: `X-Robots-Tag` compared against a `nsfw.` prefix, never matched
  `orbitalnsfw.polarisocial.xyz`, and shipped.

**Decision.** One hostname, one surface. `nsfw` is a row in `verticals` like `gaming`; any profile
may hold it as a primary or a secondary and appears in the directory, the leaderboards, the search,
the feeds and the sitemap on the same terms as everyone else. There is no age gate, no second
cookie, no `X-Robots-Tag`, and no `Host`-dependent behaviour anywhere. Age verification belongs to
the destination site, reachable only by an explicit click on a link.

**Consequences, recorded honestly.**

- The all-ages surface now contains the words its members choose, including adult-vertical labels.
  Where D2 could claim "zero adult strings on this host", D9 cannot. That is the price of the
  decision, not an oversight: the mitigation is a content policy (no imagery, no explicit labels),
  not a hostname.
- A reader who wants the adult side blocked can no longer block one hostname. That lever is gone;
  blocking the directory also blocks the all-ages parts. This was the strongest argument for D2 and
  it was overruled deliberately.
- Search classification is now a domain-wide risk rather than a per-host one. Keeping labels
  descriptive-but-not-explicit and never storing images is what keeps the domain out of adult
  classification.
- Reversal is one-way in practice: if thumbnails or uploads ever arrive, a wall has to be rebuilt
  onto an already-classified domain rather than starting clean.
- `orbitalnsfw.polarisocial.xyz` keeps answering with a 301 to the same path on the main host, so
  links already shared are not orphaned.

## D10 — A link's health is recorded, and a failed HEAD is not evidence

**Context.** The directory sells one thing: links that work. Nothing recorded whether a URL
resolved, so a dead link looked exactly like a live one until a visitor clicked it and found out.

**Decision.** `links` carries `last_status`, `last_checked` and `fail_count`. A daily checker walks
the directory oldest-checked-first, asks each URL, and records what it saw. The profile shows a
muted card with the reason at three consecutive failures; a success resets the count.

**Consequences.** Three things were learned the hard way and are now rules:

- **A failing HEAD proves nothing.** Measured in production on the first run: `bsky.app` answers
  `HEAD /profile/<handle>` with 404 and `GET` with 200, so the checker marked two live profiles as
  broken. Every status that would stop the walk (0, 400, 403, 404, 405, 501) is now confirmed with
  a one-byte ranged `GET`, and only the GET's answer counts.
- **A link checker is an SSRF gadget.** Member-supplied URLs point at whatever the member likes,
  including `http://127.0.0.1:8041/healthz`. Loopback, private, link-local, carrier-NAT and
  cloud-metadata addresses are refused, the guard is re-applied to every redirect hop (redirects
  are followed by hand, not by the client), and a refusal is recorded as a *skip* that does not
  raise `fail_count` — the member is not punished for our own caution.
- **A counter, not a flag.** Sites go down for an hour. A boolean would have needed a moderator to
  clear it; a counter clears itself.

## D11 — Referrer host only

**Context.** `source_hits` recorded which of our pages a click came from (`surface`) and nothing
about where the visitor came from, so "did the Bluesky announcement or the Mastodon one work" was
unanswerable.

**Decision.** A click stores the *host* of the `Referer` header, lowercased, alphanumerics plus
dots and hyphens, truncated at 100 characters, and nothing else. Our own hosts are dropped (an
internal navigation is not an attribution), and the dashboard shows the result as
`by_referrer`, where a missing referrer reads `direct`.

**Consequences.** The useful question gets an answer and the dangerous one stays unanswerable by
construction: we never learn the path a visitor was on, the post they clicked, or the query they
searched. Many apps strip `Referer` entirely, so `direct` will be large and it means "unknown",
not "typed the URL".

## D12 — A public read API, and an MCP server that is only a client of it

**Context.** The rankings and the directory are worth building on only if someone else can read
them, and the first consumer turned out to be an agent: Hermes, through MCP. Two ways to expose it —
an MCP server that talks to Postgres directly, or one that talks to a public HTTP API.

**Decision.** `/api/v1` is the surface: read-only, unauthenticated, 60-second cache, and the same
facts the HTML pages show. The MCP server (`mcp/orbital_mcp.py`) is a thin client of it and holds no
credentials. Write access stays on the session-cookie API until member tokens exist.

**Consequences.**

- One place decides what is public. An MCP tool cannot leak a hidden member, because the endpoint
  it calls already 404s for one — and that is asserted, not assumed: removing the hidden check
  turns the API test red.
- The server runs anywhere the site is reachable, which is what let it run on this workstation
  against production without database access.
- `api_tokens` still does not exist, so no MCP tool writes, and the honest tool list is seven reads
  rather than a write API wearing a read-only label.

## D13 — The read API carries no identity

**Context.** D12 published `/api/v1` with `Cache-Control: public, max-age=60`. The middleware that
mints the visitor cookie did not know about it, so every API response carried
`Set-Cookie: orbital_v=…`. Measured in production: `cf-cache-status: DYNAMIC` on a response that
claimed a 60-second cache, and nginx refused to cache it for the same reason. The advertised cache
lifetime was a lie, and every scrape became an origin hit.

**Decision.** No `Set-Cookie` under `/api/v1/`. HTML pages still mint the visitor id, because view
deduplication depends on it.

**Consequences.**

- The response is now cacheable by Cloudflare and by any intermediary that respects
  `Cache-Control` — which is what makes a public read API survivable at all.
- A conditional request reaches the app and gets a real 304, so the validator is worth something.
- There is **no nginx `proxy_cache`** in front of the API, and that is deliberate: a cache in front
  of an ETag answers 200 and never consults it, which turns a conditional request into a full one.
  Cloudflare's edge absorbs the burst; the per-client rate limit (60/min, `CF-Connecting-IP`, 429
  with `Retry-After`) absorbs the abuse.
- View counts and referrer attribution never see API reads. That is correct — an API consumer is not
  a visitor — but it means the numbers on a profile describe the site, not the API.
