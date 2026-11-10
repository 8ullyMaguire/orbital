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

## D32 — Capabilities are declared through the estate's mechanism, and `taste` is declared off

**Context.** The estate's W0 contract requires every public host to answer `GET /api/v1/capabilities`,
and orbital answered **404** — measured 2026-11-10 — while the manifest's document for orbital sat
correctly installed in `/var/www/estate/capabilities/orbital.json`. The vhost proxied all of `/api/v1/`
to the app and never included the snippet, so the contract was unmet by an nginx line, not by a product
feature. The same measurement found the manifest under-declaring: `search`, `recs` and `preservation`
were `off` while the site has all three (`/search`, Top/Trending ordering, nightly dumps plus a monthly
restore test).

**Decision.** Orbital adopts the estate's mechanism and does not implement the document in Rust:
`estate.toml` is the declaration, `tools/estate.py render` produces it, `install-capabilities.sh`
publishes it, the nginx snippet's exact-match `location` serves it, and `scripts/check-capabilities.sh`
is the deploy gate (200 with the right `Host`, `site = orbital`, every declared route answering).
Because a declared capability must have a route behind it, orbital gained a real
`GET /api/v1/search?q=` that serialises the one `store::search` query the HTML page already runs.

**And `capabilities.taste` stays `off`, publicly, while an ordering prior exists.** D27 forbids any
public surface from revealing that a prior exists; this document is a public surface, so declaring a
taste capability would announce the exact thing the owner forbade. The estate's capability-truth rule is
about not over-claiming to clients; under-declaring a private ranking costs a client nothing and is the
only option that satisfies D27.

**Rejected alternatives.**

- *A Rust `/api/v1/capabilities` handler.* It would be a second implementation of the manifest, and it
  would drift the moment `estate.toml` changed — the estate has already paid for that pattern six
  times (six notification implementations).
- *Declaring `taste = "local"` because it is true.* True and disclosing. The public document is not the
  place for a true value that leaks a private one.
- *Leaving the 404 and documenting the gap.* A contract that is machine-readable or nothing: a client
  cannot distinguish "no capabilities" from "nginx forgot".
- *Declaring `search = "off"` to avoid writing a route.* That is the under-declaration this decision
  fixes; the route is 40 lines over an existing query.

**Consequences.** The deploy now fails if the contract breaks, which is the point. `capabilities` is a
disclosure surface with a rule attached (roadmap §6.2): any future capability that hints at the prior
stays `off`. The gate itself had two bugs on first contact — it dialled the origin without a `Host`, so
the estate map produced an empty document name and a false 404, and it counted Cloudflare's 403 for
Python's default agent as "the route answers". Both are fixed and both are recorded here because a gate
is a program and it fails like one.

## D33 — The home shelves are a composition, and the ordered two are frozen at lambda 1.0

**Context.** The home page carried three lists, each computed inline and truncated with its own
`.take(6)`: no record of which source proposed a card, no budget, no diversity, and no shared arithmetic
with the rest of the estate. Roadmap step 1 replaced them with three shelves composed by
`polaris-candidates` (D31).

**Decision.** A shelf is a composition: named sources, a declared per-source budget, provenance, then a
selection. `for_you` unions the viewer's tag relevance with raw clicks and runs MMR at
`for_you_lambda = 0.85`; `rising` and `new` each have one source and run at **1.0 — pure relevance**,
which makes their order identical to `rank_trending` / `rank_new`. That equality is a *promise to the
reader*: a change to those two shelves changes what the site says "rising" means, so it needs its own
gate and its own declaration, not an edit inside a selection call.

An empty shelf renders no heading (it is not backfilled), and every card's `why` is built only from facts
the reader can observe — their own tags, clicks, visitors, age. Never the prior (D27).

**Rejected alternatives.**

- *Keep the inline loops and add diversity to one of them.* That is the fourth copy of the same
  arithmetic, and it is the copy that drifts (the estate's three divergent `trust_mult`s).
- *Run MMR at 0.85 on all three shelves.* "Rising" would then rename cards in a list whose definition is
  a visitor count; a shelf named after a fact must be sorted by that fact.
- *Backfill an empty shelf with the next-best list.* A zero-filled "rising" list is a recency list
  wearing a costume, and the reader cannot tell.

**Consequences.** The shelf declaration is a temporary constant in `src/shelf.rs`; roadmap step 2 moves
it into the config file with per-surface ceilings, and step 4 gives any weight change an eval with a
decision rule. Until then the two ordered shelves are frozen by a unit test
(`the_ordered_shelves_keep_the_rankers_own_order`), which is the cheapest possible guard against a
"harmless" weight edit.

## D34 — The already-visible are ranked lower by default, and the strength is a number

**Context.** The owner's 2026-11-10 observation: on attention-ranked sites every participant appears to be
in the "top 0.1%" and nobody can find anyone outside that elite, because the elite are too well known to be
compared with. The mechanism is structural, not moral — attention is the score, the score buys position,
position buys attention. Orbital's current `Top` ranks by all-time clicks and its `Trending` by seven-day
visitors, so the first 500 rows would inherit exactly that loop. Two members and 186 clicks is the cheapest
possible moment to fix a default.

**Decision.** Orbital ships a **fame headwind**: the already-visible are ranked lower by default, the
strength is configurable (`attention = damped | raw`, exponent, half-life, quotas, ceiling), and the whole
policy obeys one guardrail — **damped, never hidden**.

1. Attenuation acts on the *signal* (log clicks, exposure-corrected clicks, a per-session cap) and on the
   *structure* (per-member slot quota, long-tail cohort share, cross-shelf dedupe, an impression ceiling,
   daily tail rotation).
2. It may cost **position** and may never cost **reachability**: profile, search, vertical page, sitemap and
   the API always include every member, and no damped score may exceed its raw score.
3. It is **measurable before it is tunable**: Gini and top-10 impression share per surface are snapshotted
   weekly first (roadmap step 4), and a ranking change may not increase concentration without a recorded
   decision.
4. It is **stated in one sentence on the site** ("we rank the already-visible lower") and exposes **no
   per-entity number**. D27 forbids disclosing the *taste prior*; a concentration policy is a different fact
   and hiding a fairness rule would be the same mistake in the other direction.
5. An `exempt` list in config (default empty) marks a "monument"; it is visible in the config file and never
   on the page.

**Rejected alternatives.**

- *Hide the famous / hard-filter the top.* A filter is a policy decision taken silently on the reader's
  behalf and it removes the thing the reader came for. A soft, capped, reported demotion keeps the page
  useful and the policy visible.
- *Only show newcomers on a separate "new" tab.* That is what the site already had (`New`), and it is why
  nobody browses it: a ghetto is not a quota.
- *Leave the default raw and let the operator opt in.* The owner asked for the opposite, and defaults are
  what the first 500 rows inherit; an opt-in fairness rule ships as never-opt-in in practice.
- *Damp search results too (idea 14).* Search relevance and visibility are different promises; mixing them
  makes a zero-result query indistinguishable from a suppressed one. Filed as refused on the Concord page.
- *Apply damping inside the prior (as a term in the taste vector).* That would make a public fairness policy
  indistinguishable from the private prior, which is the one thing D27 protects.

**Estate-wide.** This is a platform-independent policy, not an orbital opinion: it is written up as the
programme's §Wave 3D ("the fame headwind") with the arithmetic as a family component, `polaris-headwind`
(queue row 18), and orbital is its reference adoption precisely because it measured first — 2 members,
186 clicks, and a home page that was already three composed shelves.

**Consequences.** The declaration is a config surface with a measurement attached, so "how hard should we
damp" is an experiment (step 4's metric), not an instinct. The worst case of a mis-set number is a duller
page, not a missing member — that is what the monotone guardrail buys. Ideas and their dispositions:
`~/secondbrain/10-Projects/orbital-fame-headwind-2026-11-10.md` (20 ideas: 15 scheduled, 4 later, 1
refused) and the Concord page carries all 20 with reasons.

## D35 — The prior is folded from declared sources, and two of orbital's tables are not evidence

**Context.** The ordering has an instance lean and, until this step, no mechanism behind it: no crate, no
declaration, no report. `polaris-taste-source` is the estate's arithmetic (D31) and D30 already names the
sources. What was undecided is where the evidence comes from — orbital has five tables that could each be
called "a signal", and two of them would have been wrong.

**Decision.** The declaration is loaded at **boot** from `TASTE_FILE` (JSON), or from the built-in
declaration D30 fixes when the file is absent; the crate's own `validate()` decides, so an invalid rule stops
the process naming the field and the source rather than being clamped into service. Evidence is
`tag_endorsements` → `Kind::Like`, and nothing else.

- `tags` is **not** evidence: the table has no `created_at`. A prior that cannot decay is a permanent claim
  about a person who may have changed, and half-life decay is half of what the crate exists to do.
- `source_hits` is **not** evidence: a click is not a statement. The crate's eight-word vocabulary omits it
  deliberately, and mapping it to `Like` would have made "somebody scrolled past" indistinguishable from
  "somebody endorsed".
- `attuned` reports **absent** rather than admitting everybody at 0.0: orbital has no affinity engine yet, so
  `Contributor::affinity` is `None` and the selector's evidence floor refuses on absence. When an affinity
  engine exists, this selector starts working without a code change — that is the point of the floor being
  data.
- The report lives in `orbital doctor` and nowhere else (D27). Absence is a status with a reason, never a
  folded zero.

**Rejected alternatives.**

- *Infer the prior from the operator's own tags and clicks.* Inference is how a policy becomes undiscoverable:
  a declared source can be audited, an inferred one cannot, and the first thing a reader would recover by
  differencing is the inference rule.
- *Weight by trust level now.* D30 has it off; the ladder is empty on this instance, and declaring a source
  that admits nobody teaches the operator to ignore the report.
- *Fold at request time.* Ranking is on the hot path; the evidence set is small and changes hourly at most.
  Folding on demand per page view would trade a boot-time cost for a per-click one.
- *Keep the declaration in an env var.* A multi-source rule with kinds and floors is not a string, and the
  refusal message has to name a field in a file the operator can open.

**Consequences.** The declaration is a boot-time contract with a falsifiable refusal, and the report is one
line per source so an empty profile always has an explanation. The profile is not wired into ordering yet:
that waits for the eval (roadmap step 2, `polaris-eval`) and the per-surface policy (step 4), so no ordering
changed in this step — the suite's 105 tests include the old ordering assertions unchanged.

## D36 — The dial is the reader's, the prior is not, and the two must not share a word

**Context.** SPEC §1e has always said the taste term is capped per surface and that a reader may narrow it,
and the estate extracted that rule as `polaris-gravity` (D31, `polaris-estate-programme` §3C.4). Orbital had
no implementation and no storage. The trap is the one the estate already recorded for kindred: a reader-facing
dial wired into an operator prior re-exposes, one legitimate feature at a time, exactly what D27 removed.

**Decision.** Orbital adopts `polaris-gravity` as the **reader's** control over **their own** lens, and the
instance's prior stays untouched by it.

- Four declared ceilings, each equal to the share the code already applied (`home` 0.40, `vertical` 0.40,
  `entity` 0.25, `ring` 0.60). A reader with no stored value gets `1.0` of that share, so this step changes
  **no ordering**. Changing a ceiling is a ranking change and needs an eval (roadmap step 2, `polaris-eval`).
- The stored number is a share of the ceiling; the ranker multiplies its own constant by `effective/ceiling`.
  Normalising at the boundary is what makes "no choice" byte-identical to yesterday.
- `visitor_profiles.gravity` is `double precision`. A first draft used `real`, and the test caught that 0.4
  round-trips through float4 as 0.4000000059604645 — a stored choice that does not compare equal to itself.
- The markup field is **`lens`**. The existing silence guard fails on any page containing the estate's word
  for the pull, and it failed on the first run of this step — correctly: a form field name is served.

**Rejected alternatives.**

- *Expose the dial as "taste gravity" on the page.* It is the clearest name for us and the most revealing one
  for a reader: it says the site has a gravity well before the reader has any reason to know.
- *Store the dial in a cookie.* A client value can be forged upward, and the only defence is the clamp — the
  same defence that already exists for free with a row. A row also survives a browser change.
- *Let the dial apply to the instance prior's sources.* That is the dial becoming an announcement: a reader
  who narrows the pull and sees the ordering change has learned the pull exists and roughly how much of it
  was the operator's.
- *Clamp an out-of-range request instead of refusing it.* A reader who asks for 100% meant 100%; serving the
  ceiling answers a different question, silently. `polaris-gravity` refuses, and the refusal names the number
  the surface allows.

**Consequences.** The dial is observable end to end: a slider in the tuner, a row per visitor, a refusal that
names the ceiling, and clamping proven live by lowering the ceiling between two requests and restoring it —
the same row reads 25, then 10, then 25 again. Roadmap step 4 replaces these ceilings per surface with real
numbers, and step 2 exists so those numbers are chosen against a measurement rather than a preference.

## D37 — Ranking changes are decided by a paired interval over real clicks, or they do not happen

**Context.** Phases B–C of the roadmap move weights, ceilings and shelf composition. Every one of those is
a change a reasonable person can argue for and nobody can check, and the estate already extracted the
metrics and the decision rule (`polaris-eval`, `paired::compare`) precisely because two products publishing
two different nDCG values for the same ranking is a failure of the instrument, not of the ranking.

**Decision.** Orbital adopts `polaris-eval` and commits a golden set, and a ranking change ships only when
the run beats the baseline with an interval that excludes zero over at least **30** paired cases.

- **Cases** are real sessions that clicked a member card; **judgements** are the clicks (grade 2) and
  nothing else. No grade-0 judgements: the site does not record what it showed, and inventing "saw it, did
  not click" would be inventing rows — the one thing this repo never does.
- **Arms** are `rank_new` (baseline) and `rank_top` with the session's lens (candidate).
- **The rule is fixed before the data** and stored in the golden set's `rule` block: metric, floor,
  interval, and the caveat that the clicks were collected under the candidate's own ordering.
- **A seed with no row is counted as missing**, and a golden set with no seeds is refused rather than run
  empty — the failure mode is a set that shrinks while the number rises.
- The command is `orbital eval` (a power-user surface, not a route: its output names rankings).

**Rejected alternatives.**

- *Wait for 200 seeds.* The roadmap asked for 200; the database holds **130** sessions with a click. The
  honest move is to record 130 and say so. Padding the set with sessions that never clicked would be
  inventing cases, and lowering the floor to fit the data is how a decision rule becomes a decoration.
- *Judge by tag overlap instead of clicks.* Tags are the candidate's own input: the eval would score the
  ranker against the arithmetic it is built from, and every change would look good.
- *Report the mean difference without the interval.* A +0.045 nDCG over 130 cases with a straddling
  interval is not a result; `paired::compare` refuses it, and so does this repo.
- *Let the eval print a number and leave the decision to the operator.* Then the number becomes a
  suggestion and the blocking never happens. Exit 1 on anything but `better`.
- *Serve the verdict at a URL.* It names rankings; the prior's silence rule (D27) starts with "no response
  describes how the ordering was produced".

**Consequences.** The first real number is `A 0.8619 / B 0.9073, delta +0.0454 [+0.0085, +0.0823]` over 130
cases — the candidate beats a recency list, and that is now a bound rather than an opinion. Any later
change to a ceiling, a source weight or a shelf declaration re-runs the same command, and a red or
`NO VERDICT` result means the change waits for more clicks.

## D38 — The headwind is judged by a per-surface weekly Gini, and that number is never public

**Context.** D34 makes visibility damping the default, and every lever in SPEC §2c changes who is seen. A
policy that cannot be measured is a preference, and a policy measured by one aggregate number over four
surfaces is four questions averaged into none. The trap on the other side is worse: publishing "90% of
impressions go to one member" is a public statement about whose work the site favours, and it names the
mechanism D27 keeps silent.

**Decision.** Orbital measures concentration per surface, weekly, and shows it only to the operator.

- **Cases**: members with at least one impression on that surface, all time. Surfaces are read from the
  data (`SELECT DISTINCT surface FROM source_hits`); a surface nobody used is not a surface with a
  concentration of zero, so it produces no row.
- **Metric**: `gini` over the impression counts and `top_share(…, 10)`. Both are pure functions with
  hand-computable values, and the tests assert the values, not the shape ("one member with 900 impressions
  and ninety-nine with one each is Gini 0.8909 and a 90.99% top-10 share").
- **Cadence**: one row per `(surface, week)` — ISO week, so re-running inside a week corrects the row. A
  series with two rows per week is a series nobody can read a trend from.
- **Exposure**: `orbital doctor` prints it, `orbital snapshot` records it, and **no route, field, feed or
  page** carries it. The gate asserts the shape of the public responses rather than the absence of a route:
  `/api/v1/stats` must be byte-identical and the home page must contain zero occurrences of the word.
- `--as-of` exists so a week can be backfilled without hand-editing rows, which is what made the two-week
  proof possible on the live host on the same day.

**Rejected alternatives.**

- *One global Gini.* Averaging four surfaces with different pools and lifetimes produces a number that moves
  for reasons nobody can name, and the headwind is applied per surface.
- *Compute on the fly per request.* The number would change under the operator's feet while reading it, and
  it belongs with the other weekly facts, not in a hot path.
- *Publish it as a fairness badge.* It is precisely the disclosure D27 forbids: a reader who learns that
  attention is 90% concentrated has learned that something is being done about it, and roughly how much.
- *Judge damping by the roadmap's own intuition ("it feels fairer").* That is the alternative D37 exists to
  refuse for ranking; it applies with more force to a policy that demotes real members.

**Consequences.** The baseline for every Phase B step now exists and is real:
`profile gini 0.39, top-10 100.0% of 187 impressions over 3 members` and `vertical 0.30 / 100.0% / 5
impressions over 2 members`, recorded for 2026-11-02 and 2026-11-09. With three and two members the top-10
share is 100% by arithmetic, not by fame, and the printed counts make that obvious. A later step that claims
"damping reduced concentration" must show the same surfaces moving against this series.

## D39 — Concentration is a guardrail with a band, never a target

**Context.** D38 gave the headwind an instrument; the obvious next temptation is to give it a bullseye and
tune toward it. Two failure modes follow immediately: flattening the ranking until `Top` means nothing
(below ~0.25 the ordering carries no signal, so members have no reason to invest in a profile), and calling
any number acceptable because it is the one we have (with three members the maximum possible Gini is 0.667,
so the recorded 0.39 is structure, not fame).

**Decision.** The concentration series is read as a **band**, and the guardrail is a direction of travel.

- Home and vertical: aim **~0.35**, accept **0.30–0.45**. Ring: **0.20–0.30**, because it exists to
  circulate. The monument view (`Top` with no decay) is exempt and expected to run higher.
- Companion: **top-10 members take 30–50 % of impressions** on discovery surfaces; above 60 % is a red
  flag, below 25 % suggests over-flattening.
- The rule that binds a change is not the level but the **sign**: a ranking change that raises the
  coefficient on a discovery surface needs a recorded decision (this file), and a rising trend is
  investigated whatever the absolute number.
- Numbers below a meaningful sample (tens of members) are recorded and **not tuned against**: N=3 makes
  0.667 the ceiling, and 187 impressions can be one person refreshing.

**Rejected alternatives.**

- *A single target of 0.0 ("everyone equal").* That is the failure the product exists to avoid: discovery
  requires that being good at something moves you. The route to "no concentration" is a random shuffle.
- *A target of 0.6–0.7 ("80/20 is natural").* That is the 1 % problem §2c was written for; the long-tail
  cohort would be permanently overrun.
- *Judging per release instead of per week.* A single week's coefficient moves with one refresh; the
  series is what distinguishes damping from noise.
- *Folding the monument view into the same number.* It would make every damping step look like a
  regression, and the exemption would become a permanently red line rather than a deliberate one.

**Consequences.** Phase B has an acceptance band rather than an aspiration, and step 8's guardrail asserts
the direction. The recorded baseline (home n/a, profile 0.39, vertical 0.30 over 3 and 2 members) is
explicitly not evidence of fame, and the report prints the counts beside the coefficient so it cannot be
quoted as if it were.

## D40 — Silence is a gate with a vocabulary, and `/api/v1/taste` is the reader's

**Context.** D27 forbids naming the prior, and the roadmap asks for a test that fails when a ranked answer
or a page names it. Two turns of this project have already produced the failure modes: a form field called
`gravity` was served to every reader, and an inline script comment on `/` shipped the word "folded". Care
did not catch either; a test did.

**Decision.** Silence is a program with a vocabulary, and the one route that may use the word answers the
caller about themselves.

- `src/silence.rs` holds seven words and a whole-word, case-insensitive matcher. Anything in `body` — HTML,
  JSON, a key — is a leak.
- The suite walks eighteen surfaces a stranger can reach with a declaration that names a source "the
  regulars", and requires every one silent. Planted leaks prove the matcher can fail before it is trusted to
  pass (`priority`, `gravityfed` and `Reprioritise` must stay clean, or the gate is noise).
- `scripts/check-silence.sh` runs the same vocabulary against the **deployed** host inside the deploy script,
  because `/api/v1/capabilities` is served by nginx from the estate-generated file and no test of the router
  can see it.
- **One exemption, in the script, with a whitespace-tolerant pattern**: `"taste": "off"` in the capabilities
  document — the family's capability name, declaring that this instance does not offer it (D32). Everything
  else about the word fails.
- **`/api/v1/taste`** answers `{"tags": …, "dial": …}`: the reader's own weights and their own dial, with
  `private, no-store` and a weak ETag, and the handler never reads `cfg.taste`.

**Rejected alternatives.**

- *A regex over the templates at build time.* Half the words arrive from Rust (`serde` keys, a `doctor` line
  would be caught, a `Json` field would not), and the feed and sitemap are generated, not templated.
- *Keeping the 404 on `/api/v1/taste`.* It was the previous turn's guard, and the roadmap now gives the path
  to the reader's lens. Keeping it would have made the roadmap's own gate unreachable; the old assertion was
  replaced deliberately, and the new test pins the *shape* (two fields, both the caller's) rather than the
  absence of a route.
- *Putting `folded` in the vocabulary.* It is UI language for a closed panel. A gate that fires on it is a
  gate somebody disables — which is worse than a narrow gate that survives.
- *Exempting capabilities by skipping it entirely.* Then the day the document grows a `prior` field, nothing
  notices. Stripping one field and re-scanning keeps the rest of the document under the gate.

**Consequences.** The suite and the deploy both fail on a leak, and the live run is a pasteable line:
`OK silence: every public surface is silent` — 14 surfaces silent, `capabilities silent but for taste:off`,
`/api/v1/taste caller's lens only`. The gate found two real leaks the moment it ran, which is the only
evidence that a gate is worth its lines.

## D41 — A click is worth what the declaration says, and `raw` means raw

**Context.** Every discovery surface starts from attention. The raw count makes three mistakes: a fan
looking ten times is counted as ten people, a card at slot 1 is clicked because it is at slot 1, and between
5,000 and 5,010 clicks the ordering describes noise. The fame headwind (D34) is built on this number, so the
number has to be a declaration with a stated consequence, not a constant someone tuned once.

**Decision.** `attention = damped | raw` plus `alpha`, default **damped / 0.50**.

- **Damped** reads a corrected column computed in SQL: one hit per `(member, session)`, each weighted
  `1 + ln position` (inverse-propensity with `exam(p) = 1/(1 + ln p)`; an unrecorded position is slot 1, the
  least credit), then log-normalised and compressed by `alpha`.
- The consequence is declared as a **ratio**: a lead of `k×` becomes `(ln(1+k)/ln 2)^alpha ×`, which is
  1.86× at the default 10× lead and is what `orbital doctor` prints.
- **Raw** reads the raw count and *never consults the corrected column*: `clicks / max`, ties by handle.
- `alpha` outside `(0, 1]` is refused by field name — 0 flattens every member to the same score and above 1
  sharpens the lead instead of damping it.
- The corrected column is computed once, in the stats fragment, so no page or ranker can forget the cap.

**Rejected alternatives.**

- *Applying the session cap in `clicks` itself.* Then the number the pages print stops being what happened,
  and every existing ordering moves silently. The tooltip "110 clicks" would become a lie.
- *Making `raw` also read the corrected column.* This was the first implementation, and the eval caught it:
  `raw`'s numbers moved (+0.0454 → +0.0483) on live data because a session that clicked twice now counted
  once. `raw` means raw, and the eval reproducing its previous run exactly is the proof.
- *A fixed `alpha` in the crate.* The ratio is a policy, and D38's band is the thing it serves; an operator
  who wants the log alone sets `alpha` 1.0, and the file says so.
- *Recording impressions to do real CTR correction.* That is a bigger instrument than this step and it
  changes what the site stores about a visitor. The declared model uses what the rows already carry, and its
  limitation is stated in SPEC §2c-bis.

**Consequences.** The ordering on a two-member pool does not visibly change — 76 vs 110 clicks compresses
from a 0.076 gap to 0.047, which the lens term still decides — and that is reported as the result rather than
dressed up. The compression is proved on a fixture with a known answer, the session cap on real rows, and
the identity over 200 seeds plus the live eval.

## D42 — Fame is read through two windows: a 90-day half-life, and a monument that keeps the record

**Context.** `Top` was a lifetime click count. On a catalogue that is meant to keep growing, that is a
permanent incumbency: whoever arrives first holds the first row forever, and the fame headwind (D34) is doing
arithmetic on a number that cannot fall. But simply expiring fame would delete the members who built the
place, which is both unfair and dishonest about what happened.

**Decision.** Two windows over the same clicks, both computed in the stats fragment.

- **`recent`** weights each hit by `0.5^(age_days / 90)` on top of D41's session cap and position weight.
  `Top`, For You and embeds rank by it. A stale 10,000 is worth 98 at 600 days.
- **`attention`** stays the lifetime total, undamped, latest-lens off. The monument tab (`?tab=monument`)
  ranks by clicks and shows the figure in full digits.
- Both numbers are labelled on their own page (`visitors` / `clicks, all time`), and `orbital doctor` prints
  both for the leader, because a declaration that says "90 days" beside a page showing a lifetime number is
  the exact confusion this step exists to prevent.
- `rank_monument` is lensless and undamped by design and is exempt from D39's band.

**Rejected alternatives.**

- *Decay the `clicks` column itself.* Then the honest count on every page becomes a fiction, and the
  monument has nothing left to show. Two columns, one truth.
- *Half-life of 30 days.* Twelve days of latency is still 0.76 at 30 days, so a member's quiet fortnight
  would look like abandonment; 90 days is the shortest window where a week off costs nothing.
- *A separate "all time" page outside the tabs.* A monument nobody can reach from the ranking it corrects is
  not reachability, it is a URL. It sits in the tab list of the vertical it belongs to.
- *Expiring members entirely (drop at zero recent).* That is a fairness policy pretending to be arithmetic:
  it hides people rather than ranking them lower, which D34 and D39 both refuse.
- *Keeping one number and calling it both.* The 90-day number is weighted and can exceed the click count, so
  one label would have to lie about one of them.

**Consequences.** Ordering on today's live pool is unchanged — every real click is at most twelve days old,
so the decay factor is 0.91 and no gap crosses — and that is reported as the result, not dressed up. The
gate's sentence is proved on a fixture with a hand-checkable answer (10,000 at 600 days < 200 at 3 days) and
by a mutation that removes the half-life, which kills both gate tests.

## D43 — Exposure is capped per member and shared out to the tail, as a declaration

**Context.** Attention weighting (D41) and the half-life (D42) make the *number* fair. They do not stop one
member from holding every slot on every shelf — and on 2026-11-10 the live home page did exactly that: two
members, eight cards, each member four times. A page that is eight copies of two people is not a catalogue,
it is a portrait of the loudest thing the site has.

**Decision.** Three declared numbers in `EXPOSURE_FILE`: `slots_per_member` (default 1), `cohort_share`
(default 0.40), `below_percentile` (default 0.50). Applied as a composition over the order a ranker produced:
count per member, skip members already on the page view, then fill the declared share of slots from members
below the exposure percentile — replacing from the end of the shelf. `0`/`0.0` is the escape hatch and
restores the previous page byte-for-byte.

- **The page, not the shelf, is the unit of the cross-shelf rule**: every rail on `/` shares one `used` set,
  including the picks rail. A rule three sections obey and a fourth ignores is not a rule.
- **`slots_per_member: 0` turns off the cross-shelf rule too.** Anything else would make the escape hatch
  partial, and a partial escape hatch cannot be used to prove what changed.
- Exposure is the existing `recent` column. No new instrument, no new table, no impression log.

**Rejected alternatives.**

- *A hard filter on the pool* (`WHERE clicks > x`). That hides people rather than sharing slots, and it makes
  a member disappear from the catalogue entirely — D34 and D39 both refuse hiding.
- *Quota by rank position* (`everyone after #6 gets a slot`). Position is a property of one shelf's order; a
  member at #7 on Rising may be #2 on New, so the rule would be a different rule on every shelf and nobody
  could state it in one sentence.
- *Randomising the tail*. Determinism is worth more than novelty here: the same page must be reproducible for
  a postmortem, and the cohort rule is about *who*, not *which of them*.
- *Per-shelf caps only, no page rule.* This was the first shape, and the live page showed why it fails: three
  shelves each correctly capped to one slot per member still showed each member three times.
- *A separate "quota" surface for the tail.* A ghetto tab is the thing D34's rejected list already names: the
  tail belongs inside the shelves, not beside them.

**Consequences.** On the live two-member site the home page is now one shelf with two cards — reported as the
result, not smoothed over: the caps did not create the shortage, they stopped the page from hiding it. With
the declaration off, `/` is `cmp`-identical to the pre-step capture. Live: each member appears exactly once
where it appeared four times before.

## D44 — The operator authors nothing, so curation is retired from every surface

**Context.** `curation` was a table one person wrote, and it powered two public surfaces: a home rail headed
"Curator's picks — hand-picked, manually ordered. The editorial layer, not the ranking", and a "Curated" tab
on every vertical. Both are the operator authoring a list on a catalogue whose whole promise is that ranking
comes from what readers actually click. The rail also announced the hand, which is the other half of the same
invariant: the instance's taste may be applied and is never named.

The exposure caps (D43) made the violation visible: the rail repeated the same two members the shelves had
just been told to show once, so the page obeyed the rule three times and broke it once.

**Decision.** Every reader of `curation` goes: the home rail, the vertical tab and its count, the "curated"
branch of the vertical handler, the `store::curated` / `curated_any` functions, the admin write route
`POST /api/admin/curation/{slug}` and its handler, and the seed that filled one row so a tab was not empty.
`?tab=curated` now falls back to Top, so an old bookmark cannot resurrect the surface. The templates lose
every mention, including the about-page paragraph that advertised an "editorial layer".

The **table is kept** and nothing reads it: dropping a table is a migration with a rollback story, and the
gate that matters — no public route reads it — is satisfied by removing the readers. The live database still
holds two rows (dev and nsfw); the pages are byte-identical with and without them, which is the test.

**Rejected alternatives.**

- *Keep the tab, rename it "Featured".* Renaming a hand is not removing it, and "featured" is the same
  promise with a softer word.
- *Keep the rail for `trust_level >= 3`.* The operator is the only TL3, so the rule would exist to exempt its
  author.
- *Drop the table in the same change.* It would take the history with it and give the deploy a migration to
  roll back for no user-visible gain. Readers first, storage later, and the roadmap keeps a row for it.
- *Keep `set_curation` for tooling.* A writer with no reader is dead code with a permission check on it, which
  is worse than absent: it implies the feature is still on.
- *Leave the about-page sentence.* A surface may not advertise a feature it does not have; the sentence was
  the last place the word "curated" survived after the code was gone.

**Consequences.** The gate is a byte comparison: a curation row that changes any page fails the suite, proved
by mutating the handler to give curated members a score bonus (the page order changes, the test fails). Live:
two curation rows exist in the database, five public surfaces carry no hand-word, and `grep -i curated
templates/` is empty.
