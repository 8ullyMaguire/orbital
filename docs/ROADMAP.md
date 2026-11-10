# Orbital — roadmap

State at the time of writing: **schema v5**, **95 tests** (45 unit, 43 HTTP, 7 store), `orbital doctor`
green, seven timers, 2 visible members, 24 links across 19 domains, capabilities document served and
gate-checked. Last rewrite **2026-11-10** (v5): the order below is the owner's — **stack first, then
shared recommendations, then frictionless contribution, then federation**.

This file is the honest list **and the ledger**: every open step carries its gate on the same line, and
a step moves to §4 "Done" only once that gate has run against the live host. The count of open steps is
the project's remaining work.

---

## 0. Direction

Orbital is **a taste-ranked catalogue of anything, written by the people who use it**, on a site that
starts at zero friction and adds a gate only when abuse proves it needs one.

The standing priorities are **not orbital's**: they are the estate's, and they live in
`~/secondbrain/10-Projects/polaris-platform-priorities.md` (owner, 2026-11-10) — popular and useful,
quality content the admin would enjoy, taste gravity, operator sovereignty with minimal involvement,
commodity software, modularity (every feature beyond core a runtime-toggled plugin), configurability,
preservation, discovery first-class, hidden gems first-class, federation by taste similarity, power users
first, engagement loops. Every step below has to pass those, and that file wins when a product decision
disagrees with it.

Five rules bind the *how*, recorded as D27–D32:

1. **The prior is applied and never named.** No field, page, sentence, badge, class name, URL parameter,
   feed, sitemap, **capability document** or error message may reveal that a taste prior exists or what
   it holds. Only `docs/` describes it (D27). This is why `capabilities.taste` is declared `off` publicly
   while an ordering prior exists — see D32.
2. **Contribution is open.** No account, trust level or invite to add an entity, write a micro-review or
   cast a ballot. Accounts buy portability and identity, never the right to write (D28).
3. **Every policy is a declaration** in one config file, with per-surface gravity and a reader dial that
   narrows what the admin set and never widens it (D29).
4. **Gravity is folded from declared sources** — named accounts (starting `criticalnexus` 0.3,
   `steeltitan` 0.7), users at or above a trust level, high-affinity readers, everyone weighted by
   affinity (D30).
5. **Share before writing**: a component the family ships is adopted *by deletion* (D31).

### The order, and why it is this order

The owner's 2026-11-10 instruction is the sequence: **same stack as every other platform → study
kindred's recommender and share what applies → frictionless contribution → federation**, on the
reasoning that the first two make the product better at what it already does, and the last two are what
increase reach most.

That order is also the cheapest first. The stack phase is adoption and deletion — no new domain model,
no new UI — and it deletes arithmetic this repo should never have written. Starting with 5,200 imported
rows and a fresh prior would mean writing the same ordering code twice.

The 496-idea brainstorm is filed in Concord (`concord.polarisocial.xyz`, project `orbital`): 523 rows,
of which **460 open, 33 refused with reasons, 30 shipped**. That page is the **list**; this file is the
**order**.

## 1. The measurements that set the order

| Measure | Value, 2026-11-10 |
|---|---|
| Visible members | 2 (one is the operator) |
| Verticals configured | 10; 7 hold nothing |
| Links | 24, across **19 distinct domains**, 5 failing 3+ checks |
| Outbound clicks recorded | 186 |
| Visitor profiles | 126 stored; **5** ever reached the personalisation warm-up |
| Logins | **0 since 2026-10-29** |
| Vertical navigations, all time | **5**, and 0 ring hits |
| Tests | **99** (45 unit, 43 HTTP, 7 store), schema v5, deploy gate green |
| Capabilities document | **200** at `/api/v1/capabilities`, 4 declared routes all answer (was 404) |
| Machine-readable search | `/api/v1/search?q=` answers JSON (was 404) |
| Concentration on home | **2 members hold every card**; 186 clicks total; no Gini or impression share recorded yet (step 4 measures it) |
| Catalogue supply available | 5,200 adult site reviews (sitemap: 112,461 URLs, 27 languages) |
| Shared arithmetic already written | `polaris-candidates` (6 modules), `polaris-taste-source`, `polaris-gravity`, `polaris-eval` |

What that rules **out** for now: ranking *tuning* without an eval, endorsement weights, monetisation,
page-body i18n, digests, embeds or vector search over 24 links, and any claim that browsing works.

What it rules **in**: adopting the family's stack, making what exists thick, an entity page worth landing
on, contribution with no account, and health as a quality gate.

## 2. What the outside critique got right, and the three places it was wrong

Kept so it is not re-derived. Right, and taken: freeze the ranking until an eval exists; hide empty
verticals (`min_members = 5` exists on every row and is compared to nothing); make embedding the ring
widget a condition of membership; recruit personally rather than waiting; save HN/Lobsters until there
are 20+ real entries; stop translating page bodies.

Wrong:

- **"Seed 10 members from your 16 sites."** Those 16 sites are all mine. That cohort is one person. The
  real supply is *entities* — verifiable, claimable, not invented.
- **"There is no self-serve onboarding."** There is. What is missing is a reason to use it — and the
  answer is now to remove auth from the path entirely.
- **"Delete the sophistication."** The ranking code is written and tested, but it is about to be
  *replaced by crates the family already runs*, which is cheaper than either keeping or deleting it.

## 3. What exists

**The product.** Profiles, links, tags, one primary vertical and two secondaries; Top / Trending / New /
Curated per vertical, webring, feeds, embeddable leaderboard, sitemap and robots. Server-rendered pages
(Askama), a Svelte dashboard embedded in the binary. Click attribution through `/l/{id}` with referrer
host only and distinct-session counting. Link health with an SSRF guard including redirect hops.
Personalisation: a lens per visitor, passive accumulation with a five-page warm-up, `/lens`, an off
switch, adoption at signup, 24-tag cap, and a tuner that arrives folded. Governance: proposals,
three-vote quorum, merges that keep tags, TL2+ endorsements as a separate +0.2 term. API tokens and a
write API; `rank-delta` from weekly snapshots. Fetched titles and favicons, generated social cards,
JSON-LD on profiles. `/status` — the estate as nginx actually serves it. English and Spanish chrome.

**The stack, as of 2026-11-10.** `/api/v1/capabilities` is served by the estate's own mechanism
(`estate.toml` → `tools/estate.py render` → `/var/www/estate/capabilities/<site>.json` → nginx snippet),
not by a second implementation in Rust, and `scripts/check-capabilities.sh` is the deploy gate: 200
through nginx with the right Host, `site = orbital`, every declared route answering. `/api/v1/search`
serialises the one `store::search` query the HTML page already runs, so the declared capability has a
route behind it. `commons-feed` is adopted and live.

**The operations.** `orbital doctor` (exit code = failures), `import`, `snapshot`, `retag`,
`fetch-titles`, `og`, `prune`, `check-links`. Seven user timers (checklinks, backup, restore-test, prune,
snapshot, metadata, uptime). Nightly dumps to `.part` then rename, 14 kept, monthly restore test into a
scratch database. nginx rate limits on the API and the unauthenticated write path, CORS on the read API,
no `Set-Cookie` on API responses. CI on both forges. A smoke test that hides its own fixture, and a
status-page test that no longer assumes a clean tree.

**Measured gaps this rewrite names.** No entity pages at all, so 5,200 available rows have nowhere to be
read. The smoke fixture's five `example.com` links still sit in production under a hidden user. The
ranking arithmetic in `src/ranking.rs` duplicates `polaris-candidates`. The capabilities document
declares `taste = off` while a prior exists — deliberate (D27/D32), and the reason the silence gate is a
test rather than a convention.

## 4. Next, in order

Each step ships alone, is verified against the live host, and carries its gate on the same line. Tags:
**[DB] [BE] [FE] [OPS] [CFG]**. Order encodes dependency *and* the owner's priority order.

### Phase A — the same stack, the same arithmetic (6 steps)

Adopt, don't write. Every step here ends with less code in this repo than it started with, and the gate
is always the same shape: the ordering is byte-identical before and after, and `git grep` cannot find the
deleted function.

1. **[BE] Adopt `polaris-taste-source`** with the declared sources (D30): `criticalnexus` 0.3,
   `steeltitan` 0.7 (inactive until they have signals), trust selector off, affinity selector with an
   evidence floor, everyone-by-affinity.
   *Gate:* an invalid declaration is refused at boot with the field named; a source with an empty corpus
   reports `absent`, never folded as 0; the per-source report appears in `orbital doctor` and nowhere
   public.
2. **[CFG] Adopt `polaris-gravity`** — the reader's dial — with orbital declaring a ceiling per surface.
   *Gate:* a reader value narrows the instance value and can never widen it; a stored value above a newly
   lowered ceiling is clamped on read (two requests, the second after the config change).
3. **[BE] Adopt `polaris-eval`** and commit a golden set: 200 seeds, a recency baseline, a decision rule
   fixed before the data arrives.
   *Gate:* a run that cannot beat the baseline prints `NO VERDICT` and blocks the change; a run that can
   prints the interval.

4. **[BE] Concentration baseline, before any damping**: Gini and top-10 share of impressions per surface,
   snapshotted weekly beside `rank_snapshots`, and exposed by `orbital doctor` — never by a public route.
   *Gate:* a synthetic pool where one member takes 90% of impressions reports Gini > 0.8 with a top-10
   share of ~90%; two weeks produce two snapshot rows; no `/api/v1` response changes because of it.
5. **[CFG] Per-surface gravity policy** in the config file: home, `/v/{slug}`, `/e/{slug}` and the ring
   each declare their own weight and cap, and no surface reads another's.
   *Gate:* changing one surface's weight changes that surface's ordering and leaves the other three
   byte-identical over the same dataset.
6. **[BE] The silence gate**: no surface may name the prior.
   *Gate:* a test serialises a ranked answer and the rendered HTML and fails if any of the prior's
   vocabulary, source names, weights or a `prior`/`taste` field appears; `/api/v1/taste` answers only the
   caller's own lens.

### Phase B — the fame headwind (6 steps)

Attention is the score, the score buys position, and position buys attention: the loop that makes every
OnlyFans model "top 0.1%" and hides everyone else. The owner's rule (2026-11-10) is that the
already-visible are **ranked lower by default, with a configurable strength** — a fairness policy, so the
site may state it in one sentence (D27 forbids disclosing the *prior*, not a concentration policy). Ideas:
`~/secondbrain/10-Projects/orbital-fame-headwind-2026-11-10.md`, filed on the Concord page.

The guardrail every step here shares: **damped, never hidden** — attenuation may cost position and may
never cost reachability, and no damped score may exceed its raw score.

7. **[BE] Attention weighting**: `log1p(clicks)` instead of `clicks / max`, exposure-corrected clicks (a
   position-bias model, so slot 1 stops winning by having been in slot 1), and a per-session cap so one fan
   cannot manufacture fame. Declared as `attention = damped | raw` plus `alpha`, default **damped**.
   *Gate:* `raw` reproduces the pre-change order byte-identically over 200 seeds; `damped` shrinks a 10×
   lead to the declared ratio on a fixture; a second click from the same session does not move a score.
8. **[BE] Amortised fame**: `Top` ranks by clicks with a 90-day half-life; a "monument" tab keeps the
   lifetime total reachable.
   *Gate:* 10,000 lifetime clicks with none in 90 days ranks below 200 recent; the monument tab still shows
   10,000; both numbers are on the page that claims them.
9. **[CFG] Exposure caps**: at most `k` slots per member per shelf (default 1), no member on two shelves in
   one page view, and a declared long-tail cohort share (default 40% of slots below median exposure).
   *Gate:* with one member 100× ahead they hold exactly 1 home slot; every shelf meets the cohort share; a
   quota of 0 restores yesterday's page byte-for-byte.
10. **[BE] Impression ceiling and tail rotation**: a rolling 24h share cap per surface, and the last `N`
   slots rotate daily by a hash of (date, member).
   *Gate:* a member above the ceiling is capped for that day and uncapped the next; two consecutive days
   differ in the tail and are identical in the top; the rotation is reproducible from the date alone.
11. **[FE] The "undiscovered" shelf**: only members below the 60th percentile of exposure, ranked by
   momentum (visitors now against their own trailing average).
   *Gate:* the shelf renders with a real long tail and renders nothing when every member is above the
   percentile; momentum outranks mass on a fixture where a famous member is flat.
12. **[CFG] Guardrail, one public sentence, exemption list**: monotone damping asserted as a property, the
   policy sentence rendered once on the home page, and an `exempt` list in config (default empty) that
   ranks a member exactly as `raw`.
   *Gate:* for every member in a 200-seed pool the damped score never exceeds the raw score and every
   profile, search hit, vertical page and sitemap entry still answers 200; the sentence appears on `/`;
   an exempt member's order equals their raw order; no per-entity damping number appears in any response.

### Phase C — shared recommendation depth (4 steps)

13. **[BE] `polaris-candidates::signal` + IDF tag overlap** replacing `ranking.rs::cosine` and
   `tag_relevance`; the deleted functions stay deleted.
   *Gate:* `git grep -n "fn cosine\|fn tag_relevance" src/` finds only the adapter module; top-10
   byte-identical for 200 seeds.
14. **[BE] Neighbourhood**: "members like this one", from tag overlap with the `Option` contract, on the
   profile page and as `/api/v1/members/{handle}/similar`.
   *Gate:* a member with no tags answers `[]` with a reason, not a fabricated list; 20 samples stable
   across a rebuild.
15. **[FE] Hidden-gem shelf**: candidates ranked *against* clicks, capped, filled by the novelty slot.
   *Gate:* the shelf's long-tail share is printed for 7 days of impressions and the step claims nothing
   below the sample floor.
16. **[BE] Floors**: a reader (or the instance) can rank a tag or category *away* — multiplicative,
    capped, reported when it matches nothing.
    *Gate:* a floored tag's entities stay reachable and move down by the declared amount; a floor that
    matches nothing is named in the response.

### Phase D — the entity substrate and frictionless contribution (10 steps)

17. **[DB] Migration 0006**: `entities`, `categories`, `entity_tags`, `entity_edges`, `entity_people`,
    `reviews`, `review_votes`, `entity_edits`, `entity_scores`, `ballots`, `contributors`, `import_runs`;
    `links.entity_id`; nullable `users.password_hash` + `claimed_at`. SPEC §5b.
    *Gate:* `cargo test` green; a duplicate `canonical_key` is refused (constraint error shown by a test);
    normalisation is table-driven (`www.`, trailing slash, fragment, uppercase host).
18. **[BE] Adopt `commons-identify`** for canonical keys (URL → key; `imdb:`, `isbn:`, `tmdb:`,
    `musicbrainz:`, `atproto:`) and delete any local resolver — the key is the join, so there must be
    exactly one implementation.
    *Gate:* `cargo tree -i commons-identify` names orbital; round-trip over 20 sample ids; `git grep -n
    "fn canonical_key" src/` finds only the adapter.
19. **[BE] Anonymous contributor identity**: a signed visitor cookie resolves to a `contributors` row with
    no password, no email, no handle; `contribute = open` is what authorises a write.
    *Gate:* two writes from one cookie jar land on one row and a second jar on a second; a request with no
    cookie is refused `400 contributor_required`, not 401.
20. **[BE] Entity from a URL**: paste a URL, resolve its key, create or return the entity with a fetched
    title and one tag.
    *Gate:* three pastes create three entities; a `www.`/trailing-slash variant of the first returns the
    same id and creates no row.
21. **[BE] Micro-review, no account**: one sentence plus 1–5, one per contributor per entity, editable for
    15 minutes, tombstoned after.
    *Gate:* a second submission by the same cookie updates rather than duplicates; a deleted review is
    invisible to anonymous readers and still present for its author.
22. **[BE] Ballot**: "which of these two?", one vote per pair per contributor, feeding the voter's vector
    only.
    *Gate:* 10 ballots from one cookie change that contributor's vector and no other visitor's ordering.
23. **[BE] Abuse bounds without accounts**: per-cookie and per-IP limits, a first-N moderation queue, a 429
    with `Retry-After`.
    *Gate:* the 11th write inside a minute answers 429; a queued write is absent from anonymous HTML and
    present for the operator; the queue self-resolves after 72h.
24. **[BE] Importer**: resumable, idempotent on `canonical_key`, provenance on every row, an `import_runs`
    receipt.
    *Gate:* run twice — the second inserts 0 and updates 0; a run killed mid-way resumes and the receipt
    says so.
25. **[OPS] First sources**: 5,200 adult rows (facts only) plus a non-adult seed so the catalogue is not
    99% one category; health and metadata backfill with the existing checker, 500/day made proportional.
    *Gate:* every imported row has `last_checked`/`last_status`; the dead share is quoted per source; one
    full pass fits in a stated number of days.
26. **[OPS] Delete the smoke fixture's rows** and assert the hidden user owns nothing.
    *Gate:* `links` drops by 5 (24 → 19) and `/` and `/v/dev` still render 200 with content.

### Phase E — the entity page and reading depth (3 steps)

27. **[FE] `/e/{slug}`**: kind, title, description, image, category, tags, health, people, similar
    entities, rating summary, reviews ordered by helpfulness.
    *Gate:* a dead site says so; a film and a site render from the same template with no `if kind ==`; the
    page carries JSON-LD `Review`/`AggregateRating` and a canonical URL.
28. **[BE] Search on the estate index** through `polaris-search-client`, site-scoped, keeping orbital's own
    filters and its `/api/v1/search` shape.
    *Gate:* a committed golden set records `nDCG@10` + `recall@50` against the local search it replaces; a
    zero-result query returns the labelled cross-site fallback; `/api/v1/search` keeps its contract.
29. **[FE] Home shelves over entities**, not members — the same three shelves, a bigger catalogue.
    *Gate:* the shelves fill from imported entities and an entity-free vertical says why it is empty.

### Phase F — configuration and modularity (4 steps)

30. **[CFG] One config file** (`orbital.toml`) plus env overlay: sources and weights, per-surface gravity,
    capability gates, which shelves run, limits, importer pacing. No policy in code.
    *Gate:* two config files over one dataset produce two orderings and two shelf sets; `orbital config
    check` exits non-zero on an unknown key or invalid weight.
31. **[BE] Capability gates per capability** (`open` / `login` / `trust(n)` / `grant`) through
    `polaris-authz`; all four declared `open` today.
    *Gate:* a capability declared `open` accepts an anonymous write; flipped to `login` the same request
    answers 401 and an authenticated one 200, with no code change.
32. **[FE] Admin settings page** that edits the config and cannot display the prior.
    *Gate:* the page's HTML and its API contain no prior values (asserted); saving a weight changes the
    live ordering after restart.
33. **[BE] User-written metadata edits** with diff and revert, plus the report/quarantine queue with
    structured reasons, a 72-hour self-resolution and a public modlog.
    *Gate:* a revert restores the previous text byte for byte; a quarantined review is 404 to anonymous and
    200 to its author; an unreported item self-resolves inside its window with the action logged.

### Phase G — power users (3 steps)

34. **[BE] `orbital` CLI**: `add <url>`, `review`, `ballot`, `search`, `import`, `export`, `doctor`,
    `config check`, with one exit-code table (0 ok, 2 bad input, 3 refused, 4 not found, 5 unavailable).
    *Gate:* each subcommand runs against the live host; the exit table is a test; `--help` is honest about
    which subcommands need a token.
35. **[FE] TUI** (`ratatui`): verticals → entities → review, keyboard-first.
    *Gate:* a scripted session renders three screens to a snapshot and completes a review end to end.
36. **[OPS] Export bundle** in the estate's `polaris-export` format plus outbound webhooks on create.
    *Gate:* a bundle restores into a scratch database with matching per-table row counts; a corrupted byte
    is caught by the checksum; a webhook delivery is logged with its response code.

### Phase H — claims (1 step)

37. **[BE] Entity claims** through `polaris-ledger`: page proof, Bluesky token or DNS TXT, with
    merge-on-claim.
    *Gate:* a claim with no proof is refused; a claim whose proof disappears within 24h is reversed and the
    reversal is a ledger entry.

### Phase I — federation (4 steps)

38. **[BE] Spike, timeboxed**: depend on gravity's AP layer (`activitystreams`, `webfinger`,
    `http_signature`) and publish one actor + WebFinger for the instance.
    *Gate:* `curl -H 'Accept: application/activity+json'` on the actor returns a valid document; a real
    Mastodon instance resolves the handle.
39. **[BE] Per-user actors and outbound reviews**: `Create`/`Update`/`Delete` of a `Review` delivered to
    followers, with the `Note`+schema.org fallback.
    *Gate:* a review appears in a Mastodon timeline with its rating visible, and the `ap_id` is the same
    object on re-delivery.
40. **[BE] Inbound**: `Follow`, `Create` for reviews, canonical-key resolution, unknown key creates the
    entity, unseen domain quarantined.
    *Gate:* a review from a second local instance lands on the right entity page and stays out of search
    until approved.
41. **[OPS] Domain allowlist, blocklist and a monthly moderation review** — the permanent cost of
    federation, staffed by the operator only as an exception path.
    *Gate:* an unseen domain is quarantined by default; the monthly review page renders with real counts and
    names what it cannot decide.

### Done

Shipped steps move here, newest last, with the commit and the one line of gate output that proved them. A
step is not done because it merged; it is done because its gate ran against the live host.

- **2026-11-10** `fdb3bbd` — the home page is three shelves composed by `polaris-candidates`
  (`compose` with a declared per-source budget, then MMR). Live: `data-shelf` renders once each for
  `for_you`, `rising`, `new` (it was 0 before), every card carries an observable `why` line
  ("76 clicks"), and the page contains zero prior vocabulary —
  `grep -ciE 'taste|prior|gravity' /` → **0**. The two ordered shelves are byte-identical to the
  rankers they replaced at lambda 1.0 (unit test `the_ordered_shelves_keep_the_rankers_own_order`);
  **99 tests** green (48 unit, 44 HTTP, 7 store).
- **2026-11-10** `340a45c` — the tuner arrives folded for everyone: the live page ships no `open`
  attribute, a visitor's own state persists in `localStorage['orbital:tuner']`, and the HTTP test covers
  the cold page and the tuned one.
- **2026-11-10** `7fd9b25` — this ledger exists, so a turn-based loop can resume without re-deriving
  "next".
- **2026-11-10** `7f11610` — `/api/v1/capabilities` served by the estate's own mechanism and gated:
  `OK capabilities: site=orbital, 4 routes answer, identity=local search=local taste=off recs=local
  preservation=local feeds=True`, from deploy step 7 (dialling the origin with
  `Host: orbital.polarisocial.xyz`).
- **2026-11-10** `7f11610` — `/api/v1/search?q=` answers JSON from the one `store::search` query; before
  the commit the live route answered 404.
- **2026-11-10** `f8cfc72` — the status-page test holds on a deployed host (it asserted only the empty
  state and failed on thinkcentre, where the deploy writes `static/estate.txt`): 95 tests green.
- **2026-11-10** `9a9d3a0` — the capabilities gate itself: a Host header for the estate map, and a 403
  (Cloudflare refusing Python's agent) counted as a failure rather than an answer.

### Deliberately not in this plan

- **Disclosing gravity.** Refused as a rule, not per idea: no visible vector, no "curated by" badge, no
  operator-styled endorsement, no instance or per-entity score, no taste manifesto, no federation of the
  vector, no instance-to-instance comparison, no public instance profile, **and no `taste` capability
  declared in the public capabilities document** (D27, D32).
- **Breadth.** No new product surface, no native app, no private-team instances, no printed or annual
  artefacts, no review translation, no voice reviews, no third-party recommendation API, no
  "verification layer" positioning.
- **Accounts first.** Login-gated writing, TL-gated review length, TL-gated tag creation and invite codes
  are refused *now* and reconsidered only when abuse data exists (D28).
- **Copied scale.** kindred's tag graph, its 384-dim embedding sidecar, Glicko ratings and arena replay are
  refused at 24 links and two members — the measurements that justify them are in
  `docs/SHARED-RECOMMENDATION.md`.
- **Multi-dimension ratings.** The estate's own verdict on `polaris-dimensions` refuses orbital.
- **The mind map.** Dropped on the owner's call; `entity_edges` is collected anyway, so it can return as a
  view without a migration.
- **Seeded members, invented reviews, or operator-written copy.**
- **A second comment tree, notification table, search index, capabilities handler or ActivityPub stack.**

## 5. Shared with the family — adopt, do not rebuild

`~/secondbrain/10-Projects/polaris-estate-programme.md` is the programme index; the estate's standing
priorities are `~/secondbrain/10-Projects/polaris-platform-priorities.md`; the recommender mapping is
`docs/SHARED-RECOMMENDATION.md`, written after reading both trees — this table used to name a crate that
does not exist. Rule of two: never extract on one caller, never fork a family component to save a day.

| need | component | state | what orbital does |
|---|---|---|---|
| candidate sources, shelves, diversity, floors | `polaris-candidates` (`compose`, `blend`, `mmr`, `floor`, `signal`, `profile`) | shipped; callers: kindred, darkhold | **steps 1, 7, 10**: adopt, delete the local loop |
| where taste comes from | `polaris-taste-source` | shipped 2026-11-09; darkhold adopted | **step 2**: declare sources |
| the reader's dial | `polaris-gravity` | shipped 2026-11-09; darkhold adopted | **step 3**: adopt; per-surface ceiling is orbital's declaration |
| profile→score arithmetic | ~~`polaris-affinity`~~ | **does not exist**; the arithmetic lives in `polaris-candidates` | corrected 2026-11-10 — orbital is a *donor*, not an adopter |
| eval harness | `polaris-eval` | live (darkhold + kindred sets) | **step 4**: same harness, orbital's golden set |
| canonical keys | `commons-identify` | shipped 2026-11-05; darkhold adopted | **step 12**: adopt, delete any local resolver |
| capabilities + cursors | `polaris-core` + the W0 contract | live on 20 hosts | **done**: adopted through `estate.toml` and the nginx snippet |
| search | `polaris-search` + `polaris-search-client` | live, 104,630 documents | **step 22**: client, site-scoped, against a golden set |
| feeds | `commons-feed` | adopted 2026-11-09; live `/feed.xml` | keep |
| trust levels | `polaris-authz` | crate 0.1 shipped | **step 25**, when accounts arrive |
| claims, moderation | `polaris-ledger` | crate 0.4 shipped | **steps 27, 31** |
| accounts / SSO | `polaris-id` | live; 2 sites wired | later — orbital starts `accounts = off` |
| export | `polaris-export` bundle format | live | **step 30** |
| link health | orbital's own | live; the catalogue makes it load-bearing | keep, and scale it (step 19) |
| ActivityPub | gravity's AP layer | live | the stack orbital federates with, never a third implementation |

## 6. Known risks

1. **The prior leaks by differencing.** Two responses that differ by one candidate recover the profile.
   The silence rule is a test per surface (step 6), not a naming convention — and a reader-facing dial
   wired into the prior re-exposes it one legitimate feature at a time.
2. **The capabilities document is a disclosure surface too.** A client reading one JSON file must not learn
   that an ordering prior exists. `taste = off` is a rule (D32), and a future capability that hints at it
   stays off.
3. **The adult filter is a security boundary** on the SFW origin: a cookie, cache key, feed, sitemap or API
   response that ignores it leaks the layer. One test per surface.
4. **An open write path is an abuse surface.** Rate limits plus an auto-resolving queue are the *whole*
   defence until accounts exist; if the queue needs a human more than once a week, step 25 moves up.
5. **The empty room.** 5,200 imported rows without reviews is the same emptiness one layer deeper.
6. **The link checker is the quality gate** — 5,200 rows at 500/day is an eleven-day pass.
7. **Configuration sprawl** becomes a second program without `orbital config check` (step 24).
8. **Entity identity will collide** (`imdb:` vs `tmdb:`, a site moving domains); merge bugs are the ones
   that lose reviews.
9. **Federation is a permanent moderation cost**, and its domain allowlist is a blocklist in disguise.
10. **Cloudflare in front of everything**: every rate limit keys on `CF-Connecting-IP`, and a gate that
    forgets it measures Cloudflare instead of the origin (measured 2026-11-10).

## 7. Where the truth is

`docs/SPEC.md` (§1b entity, §1d reviews, §1e gravity, §5b tables, §6b federation); `docs/DECISIONS.md`
(D9 one surface; D16–D20 tokens/quorum/endorsement; D21–D26 catalogue/claims/gravity/federation;
**D27–D32** the 2026-11-10 direction, including the capabilities decision); `docs/SHARED-RECOMMENDATION.md`
(what kindred and the estate crates actually contain); `docs/monetization.md` (measured traffic);
`~/secondbrain/10-Projects/polaris-platform-priorities.md` (the estate's standing priorities); the vault
log `~/secondbrain/10-Projects/2026-10-29-orbital-directory-live.md`; and the Concord project page — 523
rows, the brainstorm in its own words. When a route or command is added, all of these move in the same
pass, and every route a doc lists is proved by `curl -s -o /dev/null -w '%{http_code}'`: 200/405 mean it
exists, 404 means the doc drifted.
