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


## D21 — The catalogue is the product: sites are first-class *(scope superseded by D23, D24, D25)*

**Context.** Orbital modelled a member and their outbound links. That is a directory of *people*, and
after a week it held 2 members, 24 links and 19 distinct domains — an empty room. The useful object
for a reader is the *site*: a destination with a category, a health status, the people behind it, and
neighbours that are similar. The owner asked for a catalogue of sites and resources (the shape ThePornDude
has, but extensive and with a graph), including that site's inventory as a small proportion, and for a
mind map of sites.

**Decision.**

1. `sites` is a table of its own; `links.site_id` is a nullable FK filled by canonical domain, so "who
   links here" is a join and not a string comparison.
2. **Proportion is a surface property.** Adult rows are a vertical, carry `noindex`, and are excluded
   from the home page, the map and default search. Row count may be large while every browsable
   surface keeps them a minority.
3. **The adult catalogue stays on this origin** (D9 holds). A second hostname would reintroduce the
   five enforcement points D9 deleted, and the family has since confirmed NSFW as an audience rather
   than a hostname.
4. **The home page leads with sites**; `/members` keeps the people view.
5. **Metadata is fetched, copy is not copied.** Title, description, favicon, category and health for
   every row; a hand-written line only for the top ~200. No descriptions, ratings or category
   structure lifted from a review site.

**Rejected alternatives.**

- *Reuse `links` as the catalogue row.* It is per-member and cascades on member deletion; a site with
  no member owner cannot exist, and deleting an account would delete the catalogue entry.
- *A `creators` table parallel to `users`.* It forks tags, verticals, preferences, the ring, feeds and
  the sitemap. Shell `users` rows (D22) reuse all seven.
- *A separate adult hostname.* See (3).
- *Cap the adult rows to stay a minority by count.* The owner wants that inventory; capping it trades
  a real asset for a ratio that can be enforced on surfaces instead.

**Consequences.**

- Import becomes a product (resumable, provenance, per-row outcome), which is the estate's existing
  pattern — the cross-move the family review already lists for orbital members.
- The link checker stops being a courtesy and becomes the catalogue's quality gate: 5,200 rows at the
  current 500/day limit is an eleven-day pass.
- The domain's classification exposure changes: 5,200 adult listings on the SFW origin is a different
  risk from one adult vertical. Surface exclusion and `noindex` reduce it; nothing eliminates it.
- Extraction of a curated list is a known EU database-right risk, accepted knowingly and limited to
  facts (name, domain, category label) rather than editorial content.

## D22 — A claim is proved, not voted on

**Context.** The owner's proposal was that creators claim their rows and that high-trust members verify
them. The trust body is one person: `trust_level = 3` is the operator, and `vertical_proposals`,
`vertical_votes` and `tag_endorsements` hold 0 rows between them. D4 already recorded the lesson — a
quorum without a population is a form that never closes. A claim is also a weaker signal than a proof:
the source of truth (the person's own page, handle or domain) is fetchable.

**Decision.**

1. Unclaimed people are **shell rows**: `users.password_hash` becomes nullable, `claimed_at` is added,
   and a login guard refuses a null hash. Every existing join keeps working.
2. A claim is **verified mechanically** — a token on the person's own page (fetched, never mailed),
   the existing Bluesky `verify_token` flow, or a DNS TXT record.
3. **A human witness exists only on contest**: a second claimant escalates to one TL2 witness plus a
   24h dispute window. There is no three-TL3 vote for claims.
4. Claim tokens are single-use, expire, and are capped per curator; a claim whose proof disappears
   within 24h is reversed.
5. Claim, dispute and decision are ledger entries (family `polaris-ledger`), not a bespoke table.

**Rejected alternatives.**

- *Three trusted members confirm each claim.* Unimplementable at one TL3, and slower than a proof that
  can be fetched in one request.
- *Email an invitation to the owner.* The estate has no SMTP transport (D7); the proof is pulled from
  the destination, so nothing needs to be delivered.
- *Require an account before claiming.* It adds a password step to the moment of highest intent; the
  claim creates the account instead.

**Consequences.**

- The claim flow works at zero users, which is the only state available today.
- The witness path is exercised only when two people want the same row — rare, and cheap to defer.
- Trust levels keep their meaning for curation and endorsement without becoming an onboarding gate.

## D23 — Anything is an entity, and one identity joins the fediverse

**Context.** D21 made the *site* the first-class object. Within a day the requirement grew: users
should rate and review anything — a person, a website, a film, a book, "and stuff" — and the family's
federation goal needs one identity that another instance can name. A `sites` table plus a `works`
table plus a `people` table would be three review models, three pages and three ranking paths.

**Decision.** One `entities` table with a `kind` column and a **`canonical_key`**: a normalised URL
(`url:https://example.com`) or an external scheme (`imdb:…`, `tmdb:…`, `isbn:…`, `musicbrainz:…`,
`atproto:…`). `same_as` edges merge two keys that describe one thing, non-destructively
(`merged_into` redirect, reviews moved in the same transaction). `kind` is a hint for display and
filtering, never a behaviour switch.

**Rejected alternatives.**

- *Separate tables per kind.* Three review tables, three moderation paths, three ranking queries, and
  a merge problem at every boundary.
- *Internal UUIDs as the identity.* Then no other instance can address our objects and no import can
  be idempotent. The key has to be derivable by a stranger.
- *A pure URL identity.* Films, books and people have no canonical URL we control; `imdb:` and
  `isbn:` are the stable handles that exist.

**Consequences.** The catalogue can be seeded from several sources at once (ThePornDude for adult
sites, OpenLibrary/IMDb for works, member submissions for everything else) with the same importer. It
also means people are reviewable, which is a moderation surface that did not exist before and now has
one report/queue path shared with everything else.

## D24 — The operator writes nothing; users write everything

**Context.** D21 planned a hand-written one-liner for the top ~200 sites. The owner's answer: no
metadata written by the operator, ever — sovereignty is the ability to override, not the duty to
author. The instance is meant to run itself (family P4: operator attention is the scarcest resource).

**Decision.**

1. Every piece of copy is *fetched* (title, description, image, favicon from the page) or *written by
   a user* (reviews, metadata edits). The operator authors none of it.
2. Metadata edits are user content: a diff, an author, a report path, a revert. Any user may edit;
   trust gates how fast an edit applies (TL0–TL1 review queue, TL2+ applies immediately).
3. Reviews carry 1–5 stars for display; **the taste vector is fed by pairwise ballots**, because at
   this scale a comparison between two things is a statement about the chooser while a star is a
   claim about the object.
4. The operator's power is override: hide, revert, suspend a domain, pin a shelf. No obligation to
   produce content, and no surface that goes stale when they stop.

**Rejected alternatives.**

- *Operator-curated editorial copy.* It does not scale, it makes the operator the bottleneck, and at
  zero audience it is work for nobody. It is also the one thing that would make the catalogue a
  review site with a single reviewer.
- *Stars as the taste signal.* A five-point scale collapses to "everything is 4" as soon as people
  like a site; a pairwise choice cannot be inflated the same way.

**Consequences.** The empty state is now "no reviews yet" rather than "no copy", which is honest and
which the ballot loop can fill without the operator. Legal exposure for user reviews (defamation,
illegal content) becomes a real moderation surface, handled by the report queue and the quarantine
rule in D26.

## D25 — The gravity well includes adult; the filter is per viewer

**Context.** D21 excluded adult rows from the home page, the map and default search to keep them a
minority of what is shown. The owner's product rule is the opposite: the instance's taste gravity
should pull everything — including adult — toward the operator's taste, because that is what makes
the instance feel like a place rather than a catalogue. The owner's own priority list also says "safe
by default, three-layer content filter".

**Decision.** Adult entities are eligible on **every** gravity surface; there is no separate adult
surface and no separate hostname (D9 holds). Eligibility is gated per viewer:

- first-time anonymous: the **safe** layer, adult excluded;
- opted in (a cookie for anonymous visitors, a setting for accounts): adult eligible in ranking,
  search and shelves;
- everything: adult plus the unfiltered long tail, for anyone who asks.

The taste term stays capped at 25% and explainable, with a novelty slot, so "gravity" never means
"only what the operator already likes".

**Rejected alternatives.**

- *Keep the surface exclusion (D21 §2).* It contradicts the product thesis: a gravity well with a
  hole in it is not a gravity well.
- *Adult visible to anonymous first-time visitors.* It contradicts "safe by default", breaks the
  three-layer filter promise, and puts the domain's classification at the mercy of a shared browser.
- *A second hostname for the adult side.* D9 deleted five enforcement points; the family has since
  confirmed NSFW as an audience, not a hostname.

**Consequences.** The domain's classification exposure is now a function of the filter, not of a
surface rule — so the filter must be tested as a security boundary (an anonymous request must never
receive an adult entity, in HTML, JSON, feed or sitemap). `/status`-style measurement of "adult rows
in the top 20 for an anonymous visitor" is the probe that proves it.

## D26 — Federation is gravity's ActivityPub stack, joined on canonical keys

**Context.** The owner wants users to review anything from any instance. The estate already runs two
ActivityPub implementations: gravity (Rust/Axum, ~7,150 lines of discovery, WebFinger, HTTP
signatures and federation services) and PieFed (Python, vendored). Writing a third would be the exact
duplication the family review exists to prevent.

**Decision.**

1. Orbital federates as an AP server using gravity's layer as a dependency (or as `polaris-core` once
   a second crate needs it) — never a fresh implementation.
2. **Per-user actors**, created on first federation act; plus one instance actor.
3. A review is published as a custom `Review` object and, for peers that do not know the extension,
   as a `Note` carrying `schema:reviewRating`/`schema:itemReviewed` (BookWyrm's downgrade pattern).
4. **Inbound reviews join on `canonical_key`**, so another instance can review the same entity
   without knowing our UUIDs; an unknown key creates the entity from the review's own metadata.
5. Inbound content from an unseen domain is **quarantined** — invisible to search and ranking until a
   local `helpful` threshold or one TL2 approval. The operator allowlists domains; no per-item
   approval.

**Rejected alternatives.**

- *One instance actor for everything.* Cheapest, but reviews lose attribution and nobody can follow a
  reviewer, which is the entire point of federating reviews.
- *Read-only federation first, inbound later.* Attractive, but the inbound path is what proves the
  canonical-key join; deferring it defers the only hard part.
- *Federate only with other Orbital instances.* Then "review anything from any of their instances"
  fails, and Mastodon followers cannot see a review at all.

**Consequences.** Moderation debt is now permanent and is the honest cost of federation (family P4/P7):
domain quarantine, report queue, and a blocklist are the price of inbound content. The reward is that
the catalogue's identity layer is shared — a review written anywhere about `imdb:tt0133093` is the
same object here, and the entity page becomes the local view of a global conversation.

## D27 — The prior is silent: applied, never named

**Context.** D25 required the taste term to be "always separable in the `why` string", and the v3 plan
carried a `why` line on every card plus an idea list (2026-11-10, 496 ideas) that proposed making the
instance's taste *visible* — a taste vector on the page, a "curated by Orbital" badge, a manifesto, a
public `/@orbital` profile, publishing the vector over ActivityPub, an instance-to-instance taste
comparison, and a distinguishable style for the operator's own endorsements. The owner's instruction is
the opposite and it is categorical: **nothing should expose admin taste or let users know about it; only
docs.**

**Decision.** The instance's taste is folded from its declared sources (§D30) and **applied to
ordering**. It is disclosed nowhere else: no field in any JSON, no sentence in any page, no badge, no
class name, no URL parameter, no meta tag, no feed, no sitemap, no error message, no visible label on
the operator's own reviews or endorsements. `docs/` is the only place the policy is written down.
Orbital declares the estate's `announce = "silent"` and **does not implement the `summary` mode** at
all: a mode nobody can turn on cannot be turned on by mistake.

**Rejected alternatives.**

- *"Separable in `why`" (D25).* Superseded. Naming the term is the disclosure; a `why` string may cite
  clicks, recency, the viewer's own tags, endorsements and link health — never the instance's lean.
- *A summary mode ("a lean exists, 12% strong") behind an operator flag.* The estate allows it for
  kindred (`KINDRED_ANNOUNCE_PRIOR`); orbital refuses it because it announces the existence of the
  thing the owner asked to hide, and because a startup flag is one config edit away from a public page.
- *A per-entity "how Orbital sees this" number.* It is the profile reconstructed at one row.
- *Naming it in the HTML for debugging.* Differencing recovers the weights from two responses; a debug
  attribute behind a flag is still an attribute.

**Consequences.** The prior cannot be *measured* from the outside, so it must be measured from inside:
step 20 of the roadmap is a test that serialises a ranked answer and the rendered HTML and fails if the
prior's vocabulary, source names, weights or a `prior`/`taste` field appears. `/api/v1/taste` answers a
caller's own lens only. A reader-facing dial (D30) narrows the reader's own preference and must never be
wired into the prior — the same word, the opposite disclosure rule (estate §3C.5).

## D28 — Contribution is open first; accounts and trust levels are off

**Context.** Measured 2026-11-10: **0 logins since 2026-10-29**, 2 members (one is the operator), and 5
of 1,465 visitors ever reached the personalisation warm-up. The v3 plan made reviews, long-form text and
tag creation depend on trust levels, and required a login to write. Every one of those gates stands
between a stranger and the first contribution to an empty catalogue — which is the actual problem.

**Decision.** The site starts with `contribute = "open"`: an anonymous visitor adds an entity, writes a
micro-review and casts a ballot with no account, identified by a signed visitor cookie that resolves to
a `contributors` row with no password, email or handle. Trust levels stay in the substrate and are
**unused**. Accounts buy portability and identity (a handle, history that follows you, claims), never
the right to contribute. Each capability declares its gate (`open` / `login` / `trust(n)` / `grant`) in
the config file (D29), so turning one on is a declaration, not a code change.

Abuse is bounded without identity: per-cookie and per-IP rate limits, a first-N moderation queue that
self-resolves in 72 hours, and a public modlog.

**Rejected alternatives.**

- *Login-gated writing (idea 414).* It is the friction being removed; the site has no audience to gate.
- *TL-gated review length (idea 3) and tag creation (idea 308).* They gate exactly the first act.
- *Invite codes (idea 443).* They assume an account is the unit of contribution.
- *A moderation queue that needs a human per item.* With one operator and no users, a queue that does
  not age out is a second backlog. It self-resolves unless somebody acts.

**Consequences.** An open write path is an abuse surface, and that is now the site's main risk (§6.3 of
the roadmap). The vulnerability is deliberate and has a stated exit: if the queue needs a human more
than about once a week, step 23 (capability gates via `polaris-authz`) moves up.

## D29 — Every policy is a declaration: one config file, per-surface gravity

**Context.** The owner asked for a site as "modular and configurable as possible — each admin and each
user configures it to their taste". Today the gravity weights are constants in `ranking.rs`, the shelf
set is code, and the capability story is implicit. Constants are the reason a fork exists.

**Decision.** One config file (`orbital.toml`) plus an env overlay holds every policy: the taste
sources and their weights, each surface's gravity strength and ceiling, which shelves run, capability
gates, write limits and the importer's pacing. Code holds mechanism; the file holds opinion. A
per-surface declaration means home, `/v/{slug}`, `/e/{slug}` and the ring each carry their own number,
and changing one changes only that surface. A reader's own dial (the estate's `polaris-gravity`) may
narrow what the admin declared and never widen it, clamped **on read** so a lowered ceiling applies to
values stored earlier.

**Rejected alternatives.**

- *Env variables for everything.* They cannot express nested per-surface policy, and they are invisible
  in a diff.
- *A settings UI as the primary surface.* The file is the source of truth and the UI edits the file; a UI
  that writes a database the file does not know about recreates the two-truths problem.
- *One global gravity number.* Contradicts the request and the surfaces genuinely differ: a home shelf
  and a ring hop are not the same recommendation.

**Consequences.** `orbital config check` becomes a shipped command with its own exit code, every key
needs a validated default, and an invalid declaration must be refused at boot with the field named —
otherwise "configurable" means "silently wrong". Configuration sprawl is a named risk (§6.6).

## D30 — Gravity is folded per surface from declared sources

**Context.** The v3 plan credited the instance's taste to "the operator's own signals plus the member
aggregate", with the operator's contribution as one opaque vector. The owner asked for named sources and
per-source weights: specific accounts first (`criticalnexus` 0.3, `steeltitan` 0.7), then users at or
above a trust level, then high-affinity readers, then everyone weighted by affinity. The estate already
ships this as `polaris-taste-source` — selectors (`Role`, `TrustLevel`, `Badge`, `Affinity` with an
evidence floor, `Explicit`, `Everyone`), validated declarations, decay and a fold that reports one line
per source.

**Decision.** Orbital declares its sources in the config file and adopts the crate rather than
reimplementing it. Starting declaration: `accounts = { criticalnexus = 0.3, steeltitan = 0.7 }` (both
inactive until those accounts have signals), `trust = off` until D28's gates exist, `affinity` with an
evidence floor, `everyone` weighted by affinity. A source with an empty corpus reports **absent**, never
0. The prior stays silent (D27) whatever its sources.

**Rejected alternatives.**

- *Orbital's own `trust_mult` weighting.* It is one of the three divergent copies the estate measured.
- *A single "operator vector".* It cannot express "these two people, 0.3 and 0.7", which is the request.
- *Letting readers see which source moved a result.* That is D27's refusal with extra steps.

**Consequences.** The declared weights are an editorial act that leaves no public trace, so the only
evidence they work is an offline comparison (step 21) — and the eval must be able to say `NO VERDICT`.

## D31 — Share before writing: adopt the family's arithmetic, delete ours

**Context.** The family measured 13 of 17 products carrying zero shared dependencies, then shipped
`commons-identify`, `polaris-affinity`, `polaris-taste-source`, `polaris-gravity`, `polaris-search-client`
and the `polaris-export` format — all of which orbital would otherwise write again. Orbital is
explicitly listed as a **donor** to `polaris-affinity` (`ranking.rs::cosine`, `tag_relevance`).

**Decision.** A component the family already ships is adopted **by deletion**: the crate comes in, the
local implementation goes, and the gate is that behaviour is byte-identical before and after (or that the
gate refuses what the old path accepted). Orbital writes only what is genuinely its own: the catalogue's
identity resolution adapter, the entity page grammar, the contribution rules and its own config
declaration. Duplicated arithmetic in this repo is a defect, not a design choice.

**Rejected alternatives.**

- *Keep `ranking.rs` and cite its tests.* Tests are not portability; the family's three copies already
  disagree on what "no evidence" means (`0.0` vs `None`), which is a wrong answer, not wasted lines.
- *Write a local wrapper over a new crate "for now".* A wrapper that re-implements the arithmetic is the
  duplicate with a different import.
- *Adopt `polaris-candidates` now.* It is blocked on kindred's pool-recall measurement and orbital ranks a
  few hundred rows; the family's own verdict is "adopt `polaris-affinity`, not this".

**Consequences.** `git grep` for the deleted functions is part of the gate, and `cargo tree -i <crate>`
is how the adoption is proved. Where orbital and the family disagree, the family wins unless it
contradicts D9 (one surface) or D27 (the prior is silent).
