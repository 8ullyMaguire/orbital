# Orbital — roadmap

State at the time of writing: **schema v5**, 94 tests (45 unit, 42 HTTP, 7 store), `orbital doctor`
green on the host, seven timers running, 2 visible members, 24 links across 19 domains. Last rewrite
**2026-11-10**, the depth-and-frictionless direction (v4).

This file is the honest list **and the ledger**: every open step carries its own gate on the same
line, and a step moves to §4 "Done" only once that gate has run against the live host. The count of
open steps is the project's remaining work.

---

## 0. Direction — 2026-11-10 (v4)

Orbital is **a taste-ranked catalogue of anything, written by the people who use it**, on a site that
starts at zero friction and adds a gate only when abuse proves it needs one. The owner's 2026-11-10
brief fixes five things that outrank any feature:

1. **Depth, not breadth.** The platform is liked as it is; the work is making what exists *thick* —
   more per entity, more per surface, better ordering, easier contribution. New products, native apps,
   annual artefacts and "verification layer" positioning are refused (§4 "Deliberately not in this
   plan"). The two exceptions are **ActivityPub** and **power-user surfaces** (CLI, TUI, scripts, API,
   exports), because those are depth for the person already using the site.
2. **Frictionless first contribution.** The site starts **open**: no account to add an entity, write a
   micro-review or cast a ballot. Trust levels exist in the substrate and stay **off** until the site
   has enough people to abuse it. Accounts, when they arrive, buy portability and identity — never the
   right to contribute (D28).
3. **Gravity is silent.** The instance's taste is **applied and never named**: no field, page,
   sentence, badge, URL parameter, feed or API response may reveal that a prior exists, let alone what
   it contains. Only `docs/` describes it. This supersedes D25's "separable in `why`" clause (D27).
4. **Gravity is per-surface and multi-source.** Each surface declares its own strength, and the prior
   is folded from **declared sources**: named accounts (starting `criticalnexus` 0.3, `steeltitan` 0.7),
   users at or above a trust level, users with high affinity to the viewer, and everyone weighted by
   affinity (D30). The arithmetic comes from `polaris-taste-source`, not from this repo.
5. **Modular and configurable.** Every policy is a declaration in one config file — sources, weights,
   caps, per-surface strength, which shelves run, which capabilities are gated. An admin shapes the
   instance without editing code, and a reader's own dial narrows whatever the admin set, never widens.

**Share before writing** is the sixth rule and it is a budget, not a virtue: the estate already runs
the arithmetic this roadmap would otherwise write (§5, D31).

The 496-idea brainstorm behind this direction is filed in Concord (`concord.polarisocial.xyz`, project
`orbital`): 447 open rows, 33 refused with their reasons, 16 already shipped. That page is the **list**;
this file is the **order**.

## 1. The measurements that set the order

| Measure | Value, 2026-11-10 |
|---|---|
| Visible members | 2 (one is the operator) |
| Verticals configured | 10; 7 hold nothing |
| Links | 24, across **19 distinct domains**, 5 failing 3+ checks |
| Outbound clicks recorded | 186 |
| Visitor profiles | 126 stored (1,465 all-time, before the 180-day prune); **5** ever reached the warm-up |
| Logins | **0 since 2026-10-29** (17 sessions, all that day) |
| Vertical navigations, all time | **5** — and 0 ring hits |
| Tests | 94 (45 unit, 42 HTTP, 7 store), schema v5, deploy green 2026-11-10 |
| Catalogue supply available | 5,200 adult site reviews (source sitemap: 112,461 URLs, 27 languages) |
| Brainstorm filed | 496 ideas: 447 open, 33 refused, 16 already shipped |

What that rules **out** for now: ranking *tuning*, endorsement weights, monetisation, page-body i18n,
digests, and any claim that browsing works. A ranker over two members is a decimal point on a coin flip.

What it rules **in**: inventory, an entity page worth landing on, contribution with no account, health
as a quality gate, claims, and the one graph that is cheap with no audience — an entity's own
neighbourhood.

## 2. What the outside critique got right, and the three places it was wrong

Still true, kept so it is not re-derived. Right, and taken: freeze the ranking; hide empty verticals
(`min_members = 5` exists on every row and is compared to nothing); make embedding the ring widget a
condition of membership; recruit personally rather than waiting; save HN/Lobsters until there are 20+
real entries; stop translating page bodies.

Wrong:

- **"Seed 10 members from your 16 sites."** Those 16 sites are all mine. That cohort is one person. The
  real supply is *entities* — verifiable, claimable, not invented.
- **"There is no self-serve onboarding."** There is. What is missing is a reason to use it, which is a
  catalogue problem, not an auth problem — and the answer is now to remove auth from the path entirely.
- **"Delete the sophistication."** The ranking code is written, tested and mutation-checked. Freezing it
  costs nothing, but it is about to be *replaced by crates the family already runs*, which is cheaper
  than either keeping or deleting it.

## 3. What exists

**The product.** Profiles, links, tags, one primary vertical and two secondaries; Top / Trending / New
/ Curated per vertical, webring, feeds, embeddable leaderboard, sitemap and robots. Server-rendered
pages (Askama), a Svelte dashboard embedded in the binary. Click attribution through `/l/{id}` with
referrer host only and distinct-session counting. Link health with an SSRF guard including redirect
hops. One surface (D9). Personalisation: a lens per visitor, passive accumulation with a five-page
warm-up, `/lens`, an off switch, adoption at signup, 24-tag cap, and a tuner that arrives folded.
Governance: proposals, three-vote quorum, merges that keep tags, TL2+ endorsements as a separate +0.2
term. API tokens and a write API; `rank-delta` from weekly snapshots. Fetched titles and favicons,
generated social cards, JSON-LD on profiles. `/status` — the estate as nginx actually serves it.
English and Spanish chrome.

**The operations.** `orbital doctor` (exit code = failures), `import`, `snapshot`, `retag`,
`fetch-titles`, `og`, `prune`, `check-links`. Seven user timers (checklinks, backup, restore-test,
prune, snapshot, metadata, uptime). Nightly dumps to `.part` then rename, 14 kept, monthly restore test
into a scratch database. nginx rate limits on the API and the unauthenticated write path, CORS on the
read API, no `Set-Cookie` on API responses. CI on both forges against a Postgres service. A smoke test
that hides its own fixture.

**Measured gaps this rewrite names.** `GET /api/v1/capabilities` answers **404** — the estate's W0
contract requires it, so step 3 exists. The smoke fixture's five `example.com` links still sit in
production under a hidden user (`links` = 24, 19 distinct domains) — step 11 deletes them. There are no
entity pages at all, so 5,200 rows have nowhere to be read.

## 4. Next, in order

Each step ships alone, is verified against the live host, and carries its gate on the same line. Tags:
**[DB] [BE] [FE] [OPS] [CFG]**. Order encodes dependency: the schema cannot follow the page that reads
it, and the prior cannot be adopted before the surfaces that honour it exist.

### Phase A — the entity substrate (shared components first)

1. **[DB] Migration 0006**: `entities`, `categories`, `entity_tags`, `entity_edges`, `entity_people`,
   `reviews`, `review_votes`, `entity_edits`, `entity_scores`, `ballots`, `import_runs`;
   `links.entity_id`; nullable `users.password_hash` + `claimed_at`. SPEC §5b.
   *Gate:* `cargo test` green; a duplicate `canonical_key` is refused (constraint error shown by a test);
   normalisation is table-driven (`www.`, trailing slash, fragment, uppercase host).
2. **[BE] Adopt `commons-identify`** for canonical keys (URL → key; `imdb:`, `isbn:`, `tmdb:`,
   `musicbrainz:`, `atproto:`) and delete any local resolver — the key is the join, so there must be
   exactly one implementation, and the estate already has it.
   *Gate:* `cargo tree -i commons-identify` names orbital; round-trip tests over 20 sample ids; `git
   grep -n "fn canonical_key" src/` finds only the adapter.

### Phase B — contribution with no account

3. **[BE] Serve the capabilities document**: `GET /api/v1/capabilities` with `contribute = "open"`,
   `accounts = "off"`, `federation = []`, `feeds = "commons-feed"`, `search = "polaris-search"` and the
   declared routes (estate W0 contract; currently a 404).
   *Gate:* `curl -s /api/v1/capabilities` returns the document, and the estate's
   `check-capability-truth` finds **no served lie** for orbital (it currently flags `feeds` as
   undeclared while `/feed.xml` answers 200).
4. **[BE] Anonymous contributor identity**: a signed visitor cookie resolves to a `contributors` row
   with no password, no email, no handle; `capability: contribute = open` is what authorises a write.
   *Gate:* two writes from one cookie jar land on one contributor row and a second jar on a second; a
   request with no cookie is refused `400 contributor_required`, not 401.
5. **[BE] Entity from a URL**: paste a URL, resolve its key through `commons-identify`, create or return
   the entity with a fetched title and one tag.
   *Gate:* three pastes create three entities; a fourth (`www.`/trailing-slash variant of the first)
   returns the same entity id and creates no row.
6. **[BE] Micro-review, no account**: one sentence plus 1–5, one per contributor per entity, editable
   for 15 minutes, tombstoned after. A full review is the same object with a longer body.
   *Gate:* a second submission by the same cookie updates rather than duplicates; a deleted review is
   invisible to anonymous readers and still present for its author.
7. **[BE] Ballot**: "which of these two?", one vote per pair per contributor, feeding the voter's vector
   only.
   *Gate:* 10 ballots from one cookie change that contributor's vector and no other visitor's ordering
   (a second anonymous jar renders the order it rendered before).
8. **[BE] Abuse bounds without accounts**: per-cookie and per-IP rate limits, a first-N moderation queue
   for new contributors, and a 429 carrying `Retry-After`.
   *Gate:* the 11th write inside a minute answers 429; a queued write is absent from anonymous HTML and
   present for the operator; the queue self-resolves after 72h with no action.

### Phase C — inventory

9. **[BE] Importer**: resumable, idempotent on `canonical_key`, provenance on every row, an
   `import_runs` receipt (the estate's darkhold/AO3 pattern).
   *Gate:* run twice — the second run inserts 0 and updates 0; a run killed mid-way resumes where it
   stopped and the receipt says so.
10. **[OPS] First sources**: ThePornDude's 5,200 adult sites (facts only: name, domain, category) plus a
    non-adult seed (OpenLibrary works, an awesome-list) so the catalogue is not 99% one category.
    Health and metadata backfill with the existing checker and `fetch-titles`, with the 500/day limit
    made proportional to the row count.
    *Gate:* every imported row has `last_checked`/`last_status`; the dead share is quoted per source; one
    full pass fits in a stated number of days.
11. **[OPS] Delete the smoke fixture's rows** and assert the hidden user owns nothing.
    *Gate:* `links` drops by 5 (24 → 19) and `/` and `/v/dev` still render 200 with content.

### Phase D — the entity page and reading depth

12. **[FE] `/e/{slug}`**: kind, title, description, image, category, tags, health, people, similar
    entities, rating summary, and reviews ordered by helpfulness.
    *Gate:* a dead site says so; a film and a site render from the same template with no `if kind ==`;
    the page carries JSON-LD `Review`/`AggregateRating` and a canonical URL.
13. **[BE] Search on the estate index** through `polaris-search-client`, site-scoped, keeping orbital's
    own filters.
    *Gate:* a committed golden set runs and records `nDCG@10` + `recall@50` with `NO VERDICT` legal; a
    zero-result query returns the labelled cross-site fallback.
14. **[BE] Similar entities** via `polaris-affinity` (cosine over tag sets and reviewer overlap).
    *Gate:* neighbours for 20 seed entities are stable across a rebuild and byte-identical before and
    after the arithmetic is extracted.
15. **[FE] Home from gravity**: three shelves (for you, new, rising), each card carrying a `why` line
    built only from observable facts — clicks, recency, tags the *viewer* set, endorsements, health.
    *Gate:* three shelves render for an anonymous visitor and for a tuned one, and the rendered home
    contains no prior vocabulary (the step-20 test covers it).

### Phase E — gravity, silent and per surface

16. **[BE] Adopt `polaris-affinity`** and delete `ranking.rs`'s arithmetic (donors: `cosine`,
    `tag_relevance`, `relevance`).
    *Gate:* top-10 byte-identical for 200 seeds before and after; `git grep -n "fn cosine\|fn
    tag_relevance"` finds only the adapter module.
17. **[BE] Adopt `polaris-taste-source`** with declared sources: `criticalnexus` 0.3, `steeltitan` 0.7
    (starting values, config-owned), a trust-level selector (off until step 23), an affinity selector
    with an evidence floor, and everyone-by-affinity.
    *Gate:* an invalid declaration (weights that do not validate, unknown evidence kind) is refused at
    boot with the field named; a source with an empty corpus is reported `absent`, never folded as 0.
18. **[CFG] Adopt `polaris-gravity`** — the reader's dial — with orbital declaring a ceiling per surface.
    *Gate:* a reader value narrows the instance value and can never widen it; lowering the ceiling clamps
    an existing pinned value **on read**, proven with a stored value above the new ceiling.
19. **[CFG] Per-surface gravity policy**: home, `/v/{slug}`, `/e/{slug}` and the ring each declare their
    own weight and cap in the config file; no surface reads another's.
    *Gate:* changing one surface's declared weight changes that surface's ordering and leaves the other
    three byte-identical over the same dataset.
20. **[BE] The silence gate**: no surface, endpoint, field, URL, feed, sitemap or error message names or
    hints at the prior.
    *Gate:* a test serialises a ranked answer and the rendered HTML and fails if any of the prior's
    vocabulary, source names, weights or a `prior`/`taste` field appears; `/api/v1/taste` answers only
    the caller's own lens and 401s otherwise.
21. **[BE] Offline eval** for every ranker change: paired bootstrap over a committed golden set, decision
    rule fixed before the data, `NO VERDICT` below the sample floor.
    *Gate:* a run that cannot beat the recency baseline prints `NO VERDICT` and blocks the change; a run
    that can prints the interval.

### Phase F — modularity and configuration

22. **[CFG] One config file** (`orbital.toml`) plus env overlay: sources and their weights, per-surface
    gravity, capability gates, which shelves run, limits, importer pacing. No policy in code.
    *Gate:* two config files over one dataset produce two different orderings and two different shelf
    sets; `orbital config check` exits non-zero on an unknown key or an invalid weight.
23. **[BE] Capability gates per capability** (`open` / `login` / `trust(n)` / `grant`) through
    `polaris-authz` once accounts exist; all four declared `open` today.
    *Gate:* a capability declared `open` accepts an anonymous write; the same capability flipped to
    `login` answers 401 to that request and 200 to an authenticated one, with no code change.
24. **[FE] Admin settings page** that edits the config and cannot display the prior: sources, weights,
    caps, shelves, limits — never the folded vector, never a per-entity contribution.
    *Gate:* the page's HTML and its API contain no prior values (asserted), and saving a changed weight
    changes the live ordering after restart.
25. **[BE] User-written metadata edits** with diff and revert; TL0–TL1 queue, TL2+ apply immediately (the
    queue is off while contribution is open).
    *Gate:* an edit appears in the history with both texts; a revert restores the previous text exactly,
    byte for byte.
26. **[BE] Report and quarantine queue** with structured reasons, a 72-hour self-resolution and a public
    modlog.
    *Gate:* a quarantined review is 404 to anonymous and 200 to its author; an unreported queue item
    self-resolves inside its window with the action logged.

### Phase G — power-user surfaces

27. **[BE] `orbital` CLI**: `add <url>`, `review`, `ballot`, `search`, `import`, `export`, `doctor`,
    `config check`, with one exit-code table (0 ok, 2 bad input, 3 refused, 4 not found, 5 unavailable).
    *Gate:* each subcommand runs against the live host; the exit table is a test; `--help` is honest
    about which subcommands need a token.
28. **[FE] TUI** (`ratatui`): verticals → entities → review, keyboard-first.
    *Gate:* a scripted session renders three screens to a snapshot and completes a review end to end.
29. **[OPS] Export bundle** in the estate's `polaris-export` format plus outbound webhooks on create.
    *Gate:* a bundle restores into a scratch database with matching per-table row counts; a corrupted
    byte is caught by the checksum; a webhook delivery is logged with its response code.

### Phase H — claims

30. **[BE] Entity claims** (a person claims their own entity, a site owner their site) through
    `polaris-ledger`: page proof, Bluesky token or DNS TXT, with merge-on-claim.
    *Gate:* a claim with no proof is refused; a claim whose proof disappears within 24h is reversed and
    the reversal is a ledger entry.

### Phase I — federation

31. **[BE] Spike, timeboxed**: depend on gravity's AP layer (`activitystreams`, `webfinger`,
    `http_signature`) and publish one actor + WebFinger for the instance.
    *Gate:* `curl -H 'Accept: application/activity+json'` on the actor returns a valid document; a real
    Mastodon instance resolves the handle.
32. **[BE] Per-user actors and outbound reviews**: `Create`/`Update`/`Delete` of a `Review` delivered to
    followers, with the `Note`+schema.org fallback for peers that do not know the type.
    *Gate:* a review appears in a Mastodon timeline with its rating visible, and the `ap_id` is the same
    object on re-delivery.
33. **[BE] Inbound**: `Follow`, `Create` for reviews, canonical-key resolution, unknown key creates the
    entity, unseen domain quarantined.
    *Gate:* a review from a second local instance lands on the right entity page and stays out of search
    until approved.
34. **[OPS] Domain allowlist, blocklist and a monthly moderation review** — the permanent cost of
    federation, staffed by the operator only as an exception path.
    *Gate:* an unseen domain is quarantined by default; the monthly review page renders with real counts
    and names what it cannot decide.

### Done

Shipped steps move here, newest last, with the commit and the one line of gate output that proved them.
A step is not done because it merged; it is done because its gate ran against the live host.

- *(empty — 34 steps open)*

### Deliberately not in this plan

- **Disclosing gravity.** Refused as a rule, not per idea: no visible vector, no "curated by" badge, no
  operator-styled endorsement, no instance or per-entity score, no taste manifesto, no federation of the
  vector, no instance-to-instance comparison, no public instance profile. The Concord page carries the
  17 refused ideas with their reasons (D27).
- **Breadth.** No new product surface, no native app, no private-team instances, no printed or annual
  artefacts, no review translation, no voice reviews, no third-party recommendation API, no
  "verification layer" positioning. The Concord page carries the 12 refused breadth ideas.
- **Accounts first.** Login-gated writing, TL-gated review length, TL-gated tag creation and invite codes
  are refused *now* and reconsidered only when abuse data exists (D28).
- **Multi-dimension ratings.** The estate's own verdict on `polaris-dimensions` refuses orbital: "a claim
  is true/false — a 40/100 endorsement is nonsense".
- **The mind map.** Dropped on the owner's call; the `entity_edges` data it would need is collected
  anyway, so it can return as a view without a migration.
- **Seeded members, invented reviews, or operator-written copy.**
- **A second comment tree, notification table, search index or ActivityPub stack.**

## 5. Shared with the family — adopt, do not rebuild

`~/secondbrain/10-Projects/polaris-estate-programme.md` is the programme index; its ranked queue and
`polaris-estate-affinity-kernel` are the sources for this table. Rule of two: never extract on one
caller, never fork a family component to save a day.

| need | component | state | what orbital does |
|---|---|---|---|
| canonical keys | `commons-identify` | shipped 2026-11-05; darkhold adopted | **step 2**: adopt, delete any local resolver |
| profile→score arithmetic | `polaris-affinity` | specified (M73-01..04); orbital is a **donor** | **step 16**: adopt; `ranking.rs`'s cosine/tag_relevance are deleted |
| where taste comes from | `polaris-taste-source` | shipped 2026-11-09; darkhold adopted | **step 17**: declare sources (accounts, trust, affinity, everyone) |
| the reader's dial | `polaris-gravity` | shipped 2026-11-09; darkhold adopted | **step 18**: adopt; the per-surface ceiling is orbital's declaration |
| search | `polaris-search` + `polaris-search-client` | live, 104,630 documents | **step 13**: client, site-scoped, with a golden set |
| eval harness | `polaris-eval` | live (darkhold sets) | **step 21**: same harness, orbital's golden set |
| feeds | `commons-feed` | adopted 2026-11-09; live `/feed.xml` | keep; per-vertical feeds are a later row |
| capabilities + cursors | `polaris-core` + the W0 contract | live on 20 hosts; orbital **404s** | **step 3** |
| trust levels | `polaris-authz` | crate 0.1 shipped | **step 23**, when accounts arrive |
| claims, moderation | `polaris-ledger` | crate 0.4 shipped | **steps 26 and 30** |
| accounts / SSO | `polaris-id` | live; 2 sites wired | later — orbital starts `accounts = off` |
| inbox / loops | `polaris-inbox`, `polaris-loops` | live | later; no XP before there are people |
| export | `polaris-export` bundle format | live | **step 29** |
| comments | `polaris-comments` | shipped; suki + whitebois | only if entity pages grow a comment tree |
| media | `polaris-media` | live service | entity images, when imported |
| candidate generation | `polaris-candidates` | proposed, blocked on kindred's pool recall | **not yet** — orbital ranks a few hundred rows |
| chrome, tokens | `@polaris/ui`, nginx topbar | live on 16 hosts | adopted |
| link health | orbital's own | live; the catalogue makes it load-bearing | keep, and scale it (step 10) |
| ActivityPub | gravity's AP layer | live | the stack orbital federates with, never a third implementation (D26) |

Where orbital and the family disagree, the family decision wins unless it contradicts D9 (one surface)
or D27 (the prior is silent).

## 6. Known risks

1. **The prior leaks by differencing.** Two responses that differ by one candidate recover the profile.
   The silence rule is a test per surface (step 20), not a naming convention — and a reader-facing dial
   wired into the prior re-exposes it one legitimate feature at a time.
2. **The adult filter is a security boundary.** 5,200 adult rows on the SFW origin, eligible for
   opted-in viewers on every surface: a cookie, cache key, feed or sitemap that ignores the layer leaks
   it. One test per surface, not one middleware rule.
3. **An open write path is an abuse surface.** Rate limits plus an auto-resolving queue are the *whole*
   defence until accounts exist; if the queue needs a human more than once a week, step 23 moves up.
4. **The empty room.** 5,200 imported rows without reviews is the same emptiness one layer deeper; the
   ballot and the micro-review are what fill it, and pageviews are the judge.
5. **The link checker is the quality gate** — 5,200 rows at 500/day is an eleven-day pass, so the limit
   moves with the importer (step 10) or the backlog is structural.
6. **Configuration sprawl.** A config file that can express anything becomes a second program; every key
   needs a validated default and `orbital config check` (step 22).
7. **Entity identity will collide** (`imdb:` vs `tmdb:`, a site moving domains). `same_as` plus
   `merged_into`, moved in one transaction and tested with a rollback — merge bugs are the ones that
   lose reviews.
8. **User reviews are user liability** the moment an anonymous visitor can publish one; the report
   queue, quarantine and a takedown address are step 26, not an afterthought.
9. **Federation is a permanent moderation cost**, and its domain allowlist is a blocklist in disguise
   once it is long enough.
10. **Cloudflare in front of everything**: every rate limit keys on `CF-Connecting-IP`.

## 7. Where the truth is

`docs/SPEC.md` (§1b entity, §1d reviews, §1e gravity, §5b tables, §6b federation), `docs/DECISIONS.md`
(D9 one surface; D16–D20 tokens/quorum/endorsement; D21–D26 catalogue/claims/gravity/federation;
**D27–D31** the 2026-11-10 direction), `docs/monetization.md` (measured traffic), the vault log
`~/secondbrain/10-Projects/2026-10-29-orbital-directory-live.md`, and the Concord project page — 496
rows in the brainstorm's own words. When a route or command is added, all of these move in the same
pass, and every route a doc lists is proved by `curl -s -o /dev/null -w '%{http_code}'`: 200/401/405
mean it exists, 404 means the doc drifted.
