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

## D14 — The anonymous preference vector lives in Postgres, not in a cookie

**Context.** SPEC v3 §5 stores the visitor's discovery weights in "an encrypted cookie, 30-day TTL"
and keeps a `PreferenceWeight` table whose identity column is a nullable session hash. Both halves of
that are worse than they look. A cookie is a client-controlled blob: signing it correctly is real
cryptography we would have to write twice (once to sign, once to verify), and anything we get wrong
is a viewer choosing their own ranking weights at best. It is also size-capped — roughly forty tag
weights before browsers start dropping it, which is a silent failure at exactly the point the feature
becomes interesting.

**Decision.** One table, two owners: `preference_weights(owner_kind ∈ {user, visitor}, owner_id, tag,
weight, source)`. The visitor's `owner_id` is a SHA-256 of the existing `orbital_v` cookie, truncated
to 32 hex characters. The cookie keeps doing the only thing it ever did — naming a returning browser.
Nothing about the preference vector is trusted from the client, so nothing needs encrypting.

**Consequences.**

- A visitor who clears cookies loses their lens, and that is the correct amount of loss: the same is
  true of the visitor id itself, and `visitor_profiles` already assumed it.
- Members and anonymous visitors share one code path — `prefs::load` differs only in which owner it
  reads, so "my weights" and "your weights" cannot drift apart.
- The hashed owner is what makes the audience spelling safe: `source_hits.viewer_hash` and
  `preference_weights.owner_id` join, and no query ever returns a cookie value.
- Personalisation is not applied until the fifth page view unless the visitor has explicit weights.
  A list that reshuffles under someone who has seen four pages reads as broken, not tailored.

## D15 — The tuner is a form, not a widget

**Context.** The obvious implementation is a slider panel that `fetch`es `/api/prefs` on mouse-up.

**Decision.** `<details>` + `<form method=post action=/prefs>`, sliders included, no JavaScript.

**Consequences.**

- It works before the SPA has loaded, with JavaScript disabled, and in a text browser — which
  matters because the tuner is the front door for people who have not signed up yet.
- Server-side render means the weights are the ones the ranking will use: there is no second
  representation to drift.
- The cost is a page reload on Apply. For a control a person touches once or twice, that is not a
  cost worth a build step.

## D16 — A token, not a session, is what a script gets

**Context.** The public API is read-only and needs no credential. Writing needed a member's browser
session, which made the MCP server read-only by construction and made "update my links from a
script" impossible.

**Decision.** `POST /api/me/tokens` mints `orb_…` once; only `sha256(salt || token)` is stored, and
`Authorization: Bearer` unlocks `GET/POST/DELETE /api/v1/me/links`.

**Consequences.**

- A database dump cannot be replayed, and a token can be revoked without touching the password.
- Ten live tokens and forty links per member, both refused with a sentence. An API with no ceiling
  is a table with no ceiling.
- The token is shown once. Anyone who loses it revokes it; there is no "show it again" path, because
  that path requires storing the thing that makes it secret.
- The MCP server still exposes only read tools: the API half is done, the Python half is backlog
  (`docs/ROADMAP.md`). Stating that is better than shipping a half-wired client.

## D17 — Three votes decide a vertical, and a merge moves people

**Context.** D4 deferred quorum moderation "until there are enough TL3s to constitute one". The
directory now has enough to test the mechanism, and the taxonomy is the one thing that stays frozen
without a rule.

**Decision.** Any member proposes a vertical or a merge; three TL3 votes accept it; the deciding vote
applies the change inside the same transaction. The owner can decide by hand. A merge moves members
to the target with their tags, re-points their links, and deletes the retired vertical.

**Consequences.**

- The vote that reaches quorum cannot be raced: a second voter either sees `accepted` or gets a
  conflict, never a second merge.
- Nobody leaves the directory when a vertical does, and no primary listing silently becomes a
  secondary one — both of those would be moderated removals dressed as taxonomy.
- Flag adjudication stays with the owner (D4). A quorum over flags would let three accounts remove a
  fourth; a quorum over taxonomy only adds rooms.
- The quorum number lives in code (`governance::QUORUM`) and in the test that votes twice and
  asserts `accepted: false`.

## D18 — An endorsement is a separate term, not a bigger relevance

**Context.** The obvious implementation is `relevance += 0.2 × endorsed_share`. The test written for
it failed: relevance saturates at 1.0 for a member who holds every tag in the viewer's lens, which is
precisely the member an endorsement is meant to distinguish.

**Decision.** Endorsements score as their own additive term, `0.4 × (0.2 × share)`, outside
`relevance`, capped at three endorsements per tag.

**Consequences.**

- The lift is +0.08 against clicks weighted 0.6, so an endorsement refines a lens and cannot replace
  attention. That ratio is the promise the ranking page makes.
- The boost is computed from the member's best-endorsed tag (`max`, not `sum`), so a member with ten
  lightly-endorsed tags is not outranked by one very popular one.
- Endorsements do not apply to the un-tuned order, which stays clicks and visitors. "Honest default"
  means one thing only.

## D19 — A static dictionary, guarded by a test that reads the templates

**Context.** Full i18n needs strings in two languages and templates that ask for them by key. A
runtime catalog (JSON, fluent, gettext) adds a file to deploy and a parser to trust.

**Decision.** A `static` table in `src/i18n.rs` (`(key, en, es)`), `layout.t("key")` in templates, and
a test that greps `templates/` for every `layout.t("…")` and fails if the key is missing. A second
test forbids empty or accidentally-English entries.

**Consequences.**

- Adding a string is one line in one file, and forgetting one is a red test rather than a Spanish
  page with an English sentence in the middle.
- The fallback is a visible `(missing translation)` rather than a blank or a crash: a bug you can
  see in review.
- The templates still carry literal English in their bodies; that is a mechanical backlog, and the
  key test makes the remaining work enumerable rather than a memory exercise.

## D20 — The status page is generated from nginx, not hand-kept

**Context.** "Is the estate up" had no answer that did not involve me being asked. A hard-coded list
of hostnames would drift the first time a site was added.

**Decision.** The deploy writes `static/estate.txt` from the `server_name` directives nginx actually
serves; `/status` probes that list with a 60-second cache and shows code and latency per host.

**Consequences.**

- The page cannot disagree with the network it describes. It also cannot list a host that nginx does
  not serve, which is the failure a hand-kept list hides.
- The first version was wrong in a way that looked like an outage: it kept only the first label of
  each hostname, so every probe asked DNS for `https://ascend` and the page read "0 of 12" while
  every site answered 200. The fix is in the deploy, and the lesson is that a generated list still
  needs its contents checked once.
- Probing is cached and non-blocking per host (a `JoinSet`), so a dead host costs one timeout on a
  minute-stale page, not the whole page.

