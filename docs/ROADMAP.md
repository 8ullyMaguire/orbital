# Orbital — roadmap

State at the time of writing: **schema v9**, **211 tests**, `orbital doctor` green, seven timers, 2 visible
members, 19 links across 18 domains, capabilities document served and gate-checked. Last rewrite **2026-11-10** (v5): the order below is the owner's — **stack first, then
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
| Verticals configured | 10; **6** hold nothing — and the nav no longer offers them (D53) |
| Links | **19**, across **18 distinct domains**, 0 failing — the five `example.com` rows a smoke test wrote are deleted (D53) |
| Outbound clicks recorded | **195** |
| Visitor profiles | 126 stored; **5** ever reached the personalisation warm-up |
| Logins | **0 since 2026-10-29** |
| Vertical navigations, all time | **5**, and 0 ring hits |
| Tests | **211** (78 unit, 69 HTTP, 8 store, 5 taste, 4 eval, 4 concentration, 4 dial, 4 silence, 4 attention, 2 fame), schema **v9**, deploy gate green |
| Capabilities document | **200** at `/api/v1/capabilities`, 4 declared routes all answer (was 404) |
| Machine-readable search | `/api/v1/search?q=` answers JSON (was 404) |
| Concentration | weekly snapshots exist: profile **gini 0.39 / top-10 100 %** over 3 members, vertical **0.30 / 100 %** over 2 — N is far too small for a trend, recorded as such |
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

### Phase A — how it was judged (shipped)

Adopt, don't write. Every step here ends with less code in this repo than it started with, and the gate
is always the same shape: the ordering is byte-identical before and after, and `git grep` cannot find the
deleted function.

Attention is the score, the score buys position, and position buys attention: the loop that makes every
OnlyFans model "top 0.1%" and hides everyone else. The owner's rule (2026-11-10) is that the
already-visible are **ranked lower by default, with a configurable strength** — a fairness policy, so the
site may state it in one sentence (D27 forbids disclosing the *prior*, not a concentration policy).

**How the phase is judged.** The concentration series (step 1, shipped) is the instrument, and it is a
guardrail, not a target: a level near **0.35** on home and vertical, a band of **0.30–0.45**, a ring at
**0.20–0.30**, and the monument view exempt; companion top-10 share **30–50 %**. A step in this phase
that raises the coefficient on a discovery surface needs a recorded decision, and the *trend* — not the
absolute number — is what says whether the damping works. With three members the maximum possible Gini is
0.667, so the numbers currently recorded are structure, not fame; they are not tuned against.

Ideas:
`~/secondbrain/10-Projects/orbital-fame-headwind-2026-11-10.md`, filed on the Concord page.

The guardrail every step here shares: **damped, never hidden** — attenuation may cost position and may
never cost reachability, and no damped score may exceed its raw score.

### The open list — 13 steps, in the owner's order

**Rewritten 2026-11-10, the freeze.** The per-phase headings are gone from this list, because the order is now the
owner's freeze order and not the phase order: the housekeeping, then the contributor and the seed, then the claim,
then the work that needs no entity at all. The prose above still says *how* the phases are judged; the ten Phase D
product steps are **Parked** below and are not counted in this number. The loop's rule is the first step that is
neither Done nor Parked.

1. **[BE] Importer**: resumable, idempotent on `canonical_key`, provenance on every row, an `import_runs`
    receipt.
    *Gate:* run twice — the second inserts 0 and updates 0; a run killed mid-way resumes and the receipt
    says so.

2. **[OPS] First sources**: 5,200 adult rows (facts only) plus a non-adult seed so the catalogue is not
    99% one category; health and metadata backfill with the existing checker, 500/day made proportional.
    *Gate:* every imported row has `last_checked`/`last_status`; the dead share is quoted per source; one
    full pass fits in a stated number of days.

3. **[BE] Entity claims** through `polaris-ledger`: page proof, Bluesky token or DNS TXT, with
    merge-on-claim.
    *Gate:* a claim with no proof is refused; a claim whose proof disappears within 24h is reversed and the
    reversal is a ledger entry.

4. **[BE] Anonymous contributor identity**: a signed visitor cookie resolves to a `contributors` row with
    no password, no email, no handle; `contribute = open` is what authorises a write.
    *Gate:* two writes from one cookie jar land on one row and a second jar on a second; a request with no
    cookie is refused `400 contributor_required`, not 401.

5. **[BE] Search on the estate index** through `polaris-search-client`, site-scoped, keeping orbital's own
    filters and its `/api/v1/search` shape.
    *Gate:* a committed golden set records `nDCG@10` + `recall@50` against the local search it replaces; a
    zero-result query returns the labelled cross-site fallback; `/api/v1/search` keeps its contract.

6. **[CFG] One config file** (`orbital.toml`) plus env overlay: sources and weights, per-surface gravity,
    capability gates, which shelves run, limits, importer pacing. No policy in code.
    *Gate:* two config files over one dataset produce two orderings and two shelf sets; `orbital config
    check` exits non-zero on an unknown key or invalid weight.

7. **[BE] Capability gates per capability** (`open` / `login` / `trust(n)` / `grant`) through
    `polaris-authz`; all four declared `open` today.
    *Gate:* a capability declared `open` accepts an anonymous write; flipped to `login` the same request
    answers 401 and an authenticated one 200, with no code change.

8. **[BE] `orbital` CLI**: `add <url>`, `review`, `ballot`, `search`, `import`, `export`, `doctor`,
    `config check`, with one exit-code table (0 ok, 2 bad input, 3 refused, 4 not found, 5 unavailable).
    *Gate:* each subcommand runs against the live host; the exit table is a test; `--help` is honest about
    which subcommands need a token.

9. **[OPS] Export bundle** in the estate's `polaris-export` format plus outbound webhooks on create.
    *Gate:* a bundle restores into a scratch database with matching per-table row counts; a corrupted byte
    is caught by the checksum; a webhook delivery is logged with its response code.

10. **[BE] Spike, timeboxed**: depend on gravity's AP layer (`activitystreams`, `webfinger`,
    `http_signature`) and publish one actor + WebFinger for the instance.
    *Gate:* `curl -H 'Accept: application/activity+json'` on the actor returns a valid document; a real
    Mastodon instance resolves the handle.

11. **[BE] Per-user actors and outbound reviews**: `Create`/`Update`/`Delete` of a `Review` delivered to
    followers, with the `Note`+schema.org fallback.
    *Gate:* a review appears in a Mastodon timeline with its rating visible, and the `ap_id` is the same
    object on re-delivery.

12. **[BE] Inbound**: `Follow`, `Create` for reviews, canonical-key resolution, unknown key creates the
    entity, unseen domain quarantined.
    *Gate:* a review from a second local instance lands on the right entity page and stays out of search
    until approved.

13. **[OPS] Domain allowlist, blocklist and a monthly moderation review** — the permanent cost of
    federation, staffed by the operator only as an exception path.
    *Gate:* an unseen domain is quarantined by default; the monthly review page renders with real counts and
    names what it cannot decide.

### Parked — Phase D, the entity product

**Parked 2026-11-10 by the owner, until the site has something to be a catalogue of** (D53). The substrate shipped
(schema v9, migration 0009) and sits empty; what is parked is everything that would build a *product* on top of it
before there is a catalogue. A parked step is not next — it is waiting, and the condition below is the only way it
comes back. The text is the roadmap's own, kept verbatim with its original numbering, so unparking is a move rather
than a rewrite.

**Unpark when either holds:** the catalogue has **≥ 20 entities contributed by strangers**, or **≥ 25 visible
members**. Below that, a page per thing is a page per nobody.

1. **[BE] Adopt `commons-identify` for the half it actually covers.** Measured against the shipped crate on
    2026-11-10 (D52): it knows `isbn`, `tmdb`, `musicbrainz`, `isrc`, `wikidata` — **no URL scheme, no `imdb:`,
    no `atproto:`**. So it cannot replace `canonical_key()`, and writing both would be the two-implementation
    failure D31 forbids: the crate validates and canonicalises an *identifier's value*; `canonical_key()` decides
    *which kind* a key is and normalises the address. The step is therefore: use the crate for a validating
    `isbn:`/`tmdb:`/`musicbrainz:` write path (refuse a bad check digit at the boundary, naming the scheme), leave
    URL normalisation in the database, and add `imdb:` and `atproto:` as what they are — a scheme word plus an
    opaque value, which needs no crate.
    *Gate:* `cargo tree -i commons-identify` names orbital; a round-trip over 20 sample identifiers through the crate
    and `canonical_key()` agreeing on the value; an ISBN with a wrong check digit refused by name at the write path;
    and `git grep -n "fn canonical_key" src/` finding nothing, because the one implementation is the migration.

3. **[BE] Entity from a URL**: paste a URL, resolve its key, create or return the entity with a fetched
    title and one tag.
    *Gate:* three pastes create three entities; a `www.`/trailing-slash variant of the first returns the
    same id and creates no row.

4. **[BE] Micro-review, no account**: one sentence plus 1–5, one per contributor per entity, editable for
    15 minutes, tombstoned after.
    *Gate:* a second submission by the same cookie updates rather than duplicates; a deleted review is
    invisible to anonymous readers and still present for its author.

5. **[BE] Ballot**: "which of these two?", one vote per pair per contributor, feeding the voter's vector
    only.
    *Gate:* 10 ballots from one cookie change that contributor's vector and no other visitor's ordering.

6. **[BE] Abuse bounds without accounts**: per-cookie and per-IP limits, a first-N moderation queue, a 429
    with `Retry-After`.
    *Gate:* the 11th write inside a minute answers 429; a queued write is absent from anonymous HTML and
    present for the operator; the queue self-resolves after 72h.

10. **[FE] `/e/{slug}`**: kind, title, description, image, category, tags, health, people, similar
    entities, rating summary, reviews ordered by helpfulness.
    *Gate:* a dead site says so; a film and a site render from the same template with no `if kind ==`; the
    page carries JSON-LD `Review`/`AggregateRating` and a canonical URL.

12. **[FE] Home shelves over entities**, not members — the same three shelves, a bigger catalogue.
    *Gate:* the shelves fill from imported entities and an entity-free vertical says why it is empty.

15. **[FE] Admin settings page** that edits the config and cannot display the prior.
    *Gate:* the page's HTML and its API contain no prior values (asserted); saving a weight changes the
    live ordering after restart.

16. **[BE] User-written metadata edits** with diff and revert, plus the report/quarantine queue with
    structured reasons, a 72-hour self-resolution and a public modlog.
    *Gate:* a revert restores the previous text byte for byte; a quarantined review is 404 to anonymous and
    200 to its author; an unreported item self-resolves inside its window with the action logged.

18. **[FE] TUI** (`ratatui`): verticals → entities → review, keyboard-first.
    *Gate:* a scripted session renders three screens to a snapshot and completes a review end to end.

### Done

Shipped steps move here, newest last, with the commit and the one line of gate output that proved them. A
step is not done because it merged; it is done because its gate ran against the live host.

- **2026-11-10** `3938b0b`…`69684a7` — **step 1, the fixture rows (and the freeze's two housekeeping items)**: the
  smoke test that wrote to the live database left two hidden members and five `example.com` links, which the vertical
  counters counted and the link checker checked. `orbital cleanup [--apply]` reports and removes exactly
  *hidden + test-shaped* handles (`^(test|smoke|e2e|dummy|fixture)`), `orbital doctor` FAILs if one ever comes back, and
  a vertical with nobody in it is no longer offered by the front page or the nav (its page still answers 200).
  **Gate**: cleanup exact and idempotent — 2 members and 2 links removed in the test, second run `(0, 0)`, a real
  member's link left alone; front page shows `/v/dev` and not `/v/gaming`, has no "still growing" line, `/v/gaming` is
  still 200; live `links` 24 → 19, `doctor` `PASS fixtures — none`. 211 tests (`cargo test`, 16 binaries). D53 records the parking itself.
- **2026-11-10** `bc94645`…`5fc8000` — **step 1, migration 0009 (the ledger's "0006")**: the entity substrate —
  `entities`, `categories`, `entity_tags`, `entity_edges`, `entity_people`, `reviews`, `review_votes`,
  `entity_edits`, `entity_scores`, `ballots`, `import_runs`; `links.entity_id`; nullable `users.password_hash` and a
  `users.claimed_at`. The join key is normalised **by the database at write time**: `canonical_key(text)` plus a
  `BEFORE INSERT OR UPDATE` trigger, so no caller can write an unnormalised key.
  **Gate**: `cargo test` green (**209**); a duplicate key refused as `entities_canonical_key_key`, including four
  variants of one page; normalisation driven from a five-row table (`www.`, trailing slash, fragment, uppercase
  host), with a paginated view and a neighbouring path proved to stay distinct. Four mutations bite (unique index
  dropped · fragment kept · host not lowercased · self-merge allowed). Live: schema **9**, **11** tables present and
  **empty**, `canonical_key('HTTPS://WWW.Example.com/x/#frag')` = `url:https://example.com/x` =
  `canonical_key('https://example.com/x')`, every public surface 200, the word *entity* nowhere on the home page and
  `/entities` 404 — a substrate step changes no surface, and says so.
  **Two corrections found by implementing it**: the gate's key keeps its scheme (`url:https://…`), and
  `reviews.user_id` must cascade — with `SET NULL` the "an author exists" check fails *inside the user's own deletion*,
  which the test found by deleting a contributor. **And the next step's premise is false**: `commons-identify` covers
  isbn/tmdb/musicbrainz/isrc/wikidata only — no URL, no `imdb:`, no `atproto:` — so step 1 of this list is rewritten
  rather than left to fail.
- **2026-11-10** `e8146c2`…`f3ab5a1` — **step 1, floors**: a tag or a category a reader (or the instance) ranks
  *away* — `?not=rust,cli:0.4,v:music`, with `FLOORS_FILE` for the instance. The arithmetic is
  `polaris_candidates::floor`: multiplicative, 0.50 per rule, 0.60 together, so a floor is a position and never a
  filter. Every discovery list takes it (`rank_top_floored`, `rank_trending_floored`, `rank_new_floored`).
  **Gate**: a floored member stays on the page and moves down by the declared amount; a floor that matched nothing is
  named. Live: `/` → `criticalnexus, steeltitan`; `/?not=rust:0.5` → `steeltitan, criticalnexus` with
  `ranked away: 1 profile lower (@criticalnexus)`; `/?not=typo_here` → `matches nothing here — check the spelling`;
  `/?not=rust:0.9` → `has weight 0.9; the cap is 0.5`; `/@criticalnexus`, `/v/dev`, the similar API all 200;
  `orbital doctor` → `PASS floors none declared (FLOORS_FILE absent)`.
  **The finding that took a live measurement**: the first version applied the floor inside the ranker only, and on
  live a 0.5 floor moved nothing — the exposure caps' cohort promote lifted the floored member straight back, because
  the member a reader ranks away is usually the one with less exposure. The floor now runs last on every shelf, after
  the budget, the diversity pass, the caps and the promote: a stated preference outranks an automatic quota.
  Three mutations bite (floor removed from the ranker, an unapplied floor made silent, `checked` swapped for `new`).
  **206 tests**.
- **2026-11-10** `c392a32`…`81c565e` — **step 1, the "hidden gems" shelf**: members the click count has
  overlooked and who convert the readers they do get — eligible at `visitors >= 5`, candidates strictly below the
  median clicks of the eligible set, ordered by `clicks / visitors`, capped, with the novelty slot filling what the
  caps leave empty. It sits **third**, ahead of `Undiscovered`: a claim about conversion is more specific than a claim
  about momentum, and the cross-shelf rule gives the specific claim first pick.
  **Gate**: `orbital impressions --days 7` prints the long-tail share and refuses under either floor — 200
  impressions, 8 members. Live: `home: 224 impression(s) in the last 168h, 2 member(s)` then
  `NO VERDICT — 2 member(s) in the pool; a tail needs at least 8 to mean anything`. **The shelf itself cannot render
  on live**: two members, both already on `For you`, and each member gets one slot per page — so this step ships with
  its gate met by refusal and its shelf proven on fixtures, named rather than dressed up.
  Three mutations bite: removing `clicks < median` puts the loud members on the shelf (`["quiet_gem", "loud00", …]`),
  removing the audience floor makes a four-visitor member a gem, removing the verdict floor prints
  `long tail: 9.1% of 110 impression(s)`. Two findings came out of writing the gates: `<= median` made *every* member
  eligible on a flat pool (strict `<` fixes it), and the impression floor alone let a two-member pool report a
  "50.0% tail" that is one member out of two — hence the second floor. **199 tests**.
- **2026-11-10** `93096d5`…`264e11d` — **step 1, the neighbourhood**: `src/neighbours.rs` answers "people like
  you" from IDF tag overlap — live on this surface, which is where D48 said the weighting would become live, because
  a new list has no previous order to regress. Four reasons instead of one empty box (`shared`, `no_tags`,
  `nothing_shared`, `alone`), each with a sentence the sidebar renders in place of the list; `ranking::similarity`
  deleted with its single caller. The same function serves `/api/v1/members/{handle}/similar`, so the API cannot
  drift from the page, and the coefficient is not published.
  **Gate**: a member with no tags answers `[]` with `reason: "no_tags"` and a sentence (mutation that hands a tagless
  member somebody else's list: `left: NothingShared` fails); twenty samples computed twice from a pool rebuilt in
  reverse order are identical, with ties asserted alphabetical by handle (mutation reversing the tiebreak:
  `["zulu", "mike", "alpha"]` fails); the rare-tag fixture fails without the weighting (`the rare tag is the reason:
  [("c0", 0.707…), …]`). **Two gates were strengthened after they passed when they should not have**: determinism
  compared two runs of the same code (any tiebreak passes), and the rare-tag case had matching orders weighted and
  unweighted, so the fixture now gives the rare sharer a third tag where unweighted `0.5` loses to weighted `0.508`.
  **Live**: `/api/v1/members/criticalnexus/similar` → `reason: shared`, `shared_tags: ["bluesky", "self-hosting"]`,
  `etag: W/"6e69584104ab96e5"`, `304` on `If-None-Match`, `404` for an unknown handle; the sidebar prints
  `shares bluesky, self-hosting`. **Not exercised live**: the three empty reasons — every visible member here has
  tags, and inventing an untagged member to show one would be the fabricated row the invariant forbids.
  **Correction (same day)**: "`ranking::similarity` deleted with its single caller" was true of the plan and false of
  the tree — the step removed the caller and left the function. It is deleted now, with `render::similarity_reason`
  and the `weights`/`hub_tags` reads it alone needed; the suite was green both ways, which is why a deletion claim
  has to be grepped, not assumed. **195 tests**.
- **2026-11-10** `ad08c8b`…`b1635bc` — **step 1, the overlap adapter**: `src/overlap.rs` now holds the only
  `cosine`/`tag_relevance`, the IDF weighting the old comment claimed (`idf = ln(1 + N/df)`, normalised so the
  rarest tag weighs `1.0`), and a `TagSignal` implementing `polaris_candidates::Signal` where absence is `None`.
  **Gate**: `git grep -n "fn cosine\|fn tag_relevance" src/` → `src/overlap.rs:155`, `src/overlap.rs:164`, and
  nothing else — after renaming a test called `cosine_is_set_overlap`, which was the third match and would have
  satisfied the grep while defeating it. Top-10 unchanged: 10,000+ pairs over 200 seeded pools are **bit-identical**
  to the deleted formula (kept as an oracle in the suite), and the live home order is
  `(['criticalnexus','steeltitan'], ['for_you'])` before and after the deploy.
  **Named, not hidden**: pool-level IDF is *not* wired into any surface — it changes an order on uneven pools and
  order changes wait for `orbital eval`. `the_page_ranks_with_the_uniform_entry_point_and_nothing_else` fails if
  it ever reaches `ranking.rs` without that gate. Three mutations prove the gate bites: a one-ulp drift, `Some(0.0)`
  for an empty side, a second definition in `ranking.rs`. **187 tests**.
- **2026-11-10** `abfb59d`…`ca4b9bb` — **step 1, the guardrail**: damping held to five properties over 200
  seeded pools (bounded, leader untouched, monotone, compressing, `lead_after(k) ≤ k`); an `exempt` list that
  scores a member exactly as `raw` on a second normalisation so it cannot move anyone else; one public sentence
  on `/`; and no per-member score anywhere, checked structurally (no card attribute, no decimal, no score-like
  JSON key) because prose cannot be grepped for it.
  **The gate cannot be met as written, and this is named rather than met**: "for every member the damped score
  never exceeds the raw score" is arithmetically false — a one-click member of a thousand goes from `0.001` to
  `0.3167` at `alpha 0.5`, which is the mechanism. `the_clause_as_written_is_false_and_the_counterexample_is_
  arithmetic` asserts the refutation, and the step is done on the five properties, the sentence, the exemption
  and the no-score check — never on that clause. The false wording is left in §3 for the owner to amend.
  **Live**: `PASS attention  damped: a lead of 10 times becomes 1.86 times (alpha 0.50)`; the sentence appears
  exactly once on `/`; cards carry no score attribute and no decimal; with a declaration, `…; 1 exempt:
  steeltitan`; a misspelt key is refused at boot with `unknown field `exmpt``. **181 tests** (78 unit, 68 HTTP,
  7 store, 5 taste, 5 ceiling, 4 eval, 4 concentration, 4 dial, 4 silence, 4 attention, 3 guardrail, 2 fame).
- **2026-11-10** (same step, found by its own gate) — **the tuner's flicker**: `the_same_vertical_twice_is_the_
  same_bytes` failed once in three runs on `in dev weight` (`value="1"` then `value="0"`). The passive signal is
  0.05 a page view, decayed on read, ×10 and rounded — a hair off the 0.5 boundary, so microseconds decided the
  step. A weight below three quarters of a step is now not a step, which is also truer: one page read is not a
  preference the visitor expressed (`0.3` still shows as `3`, despite `0.3 * 10 == 2.9999999999999996`).
- **2026-11-10** `2c54797`…`ae99e8b` — **step 1, the "undiscovered" shelf**: a fourth home shelf of the members
  below the 60th percentile of exposure, ordered by momentum — this week's visitors against the member's own
  trailing three-week baseline, `(w+1)/(prev/3+1)`, with `visitors_prev` added to the stats fragment rather
  than a new table.
  **Gate**: the shelf renders on a pool bigger than the page and leads with the fastest-growing member
  (mutation sorting by exposure instead: `["small_silent", "small_flat", "small_growing"]` and the test fails);
  removing the percentile filter puts the famous flat member on it and fails; an all-flat pool renders no
  shelf at all. Two structural findings came out of the gate as written: the shelf needs a pool bigger than the
  page (the D43 dedupe starves a fourth shelf, so it moved ahead of `New`), and the D43 cohort share cannot be
  met by every shelf once a page has four — it is now asserted as a **page-level** promise with the first shelf
  held to the exact share.
  **Live**: `PASS undiscovered  cut 110.0 of exposure (60th percentile), 1 of 2 member(s) below it`; the shelf
  does not render, because the one member below the cut is already on `For you` — the honest state of a
  two-member catalogue, named rather than padded. **169 tests** (78 unit, 68 HTTP, 7 store, 5 taste, 5 ceiling,
  4 eval, 4 concentration, 4 dial, 4 silence, 4 attention, 2 fame).
- **2026-11-10** `54a3240`…`679e442` — **step 1, the impression ceiling and the daily rotation**: every render
  records one impression per member per hour (no visitor identity, one bounded row), a member over 50 % of a
  surface's 24-hour impressions is left out of that surface, and the last two slots of a shelf are refilled
  daily by `hash64(day, member)` (FNV-1a, hand-written because `DefaultHasher` is not stable across releases).
  **Gate**: a member with 100 of 102 impressions is not shown and is shown again once the buckets fall out of
  the window (mutation that disables the pool filter: `100 of 102 impressions is over a 50% ceiling, so the
  member is not shown` fails); two consecutive days differ in the tail and agree on the top (mutation that
  drops the day from the hash: `two consecutive days must not show the same tail: [0,1,2,3,15,18] vs
  [0,1,2,3,15,18]`); and the hash is pinned to a golden value so a refactor cannot silently reseed it.
  **Live**: `PASS ceiling  a member may hold at most 50% of a surface's 24-hour impressions (after 8), and the
  last 2 slot(s) rotate daily — home has 6 impression(s) in 24h from 2 member(s), 0 over their share`;
  `orbital impressions` prints `tail for 2026-11-10: @criticalnexus @steeltitan` and `tail for 2026-11-11:
  @steeltitan @criticalnexus`, byte-identical across two runs; a real fetch moved `home` from 6 to 8.
  **Not exercised on live**: the ceiling itself — two members at exactly 50 % sit *at* the share, not over it,
  and I will not inject impressions to make a fixture out of production. **164 tests** (78 unit, 68 HTTP,
  7 store, 5 taste, 5 ceiling, 4 eval, 4 concentration, 4 dial, 4 silence, 4 attention, 2 fame).
- **2026-11-10** `5eac9e5`…`3485067` — **step 1, curation retired**: the home rail, the vertical tab and its
  count, the `curated` branch, `store::curated`/`curated_any`, the admin write route and the seed that fed one
  row all go; `?tab=curated` falls back to Top; the table stays, unread, until a migration can drop it.
  **Gate**: a curation row naming the member the ranking would *not* promote changes no page (mutation that
  gives curated members a score bonus: the vertical's row order changes and the test fails on the first
  differing line); no template carries the word (`templates still carry a hand: ...` was the RED output, and
  it caught three real ones — the about page's "editorial layer", the report form's "curator", and the home
  rail's subtitle).
  **Live**: the database still holds two curation rows (dev, nsfw), five public surfaces (`/`, `/v/dev`,
  `/v/nsfw`, `/about`, `/report`) carry no hand-word, `?tab=curated` renders no Curated text, and
  `grep -ri curated templates/` is empty. **150 tests** (78 unit, 68 HTTP, 7 store, 5 taste, 4 eval,
  4 concentration, 4 dial, 4 silence, 4 attention, 2 fame).
  **Note**: the shared thinkcentre checkout was being edited by another session (`Cargo.toml`, `src/lib.rs`
  adding `polaris-healthz`), so this step was built and tested in a `git worktree` at HEAD under a separate
  target dir, and the deploy's git-sync step was skipped rather than discarding that work.
- **2026-11-10** `7fe31c5`…`b76a764` — **step 1, exposure caps**: `EXPOSURE_FILE` declares `slots_per_member`
  (1), `cohort_share` (0.40) and `below_percentile` (0.50); applied as a composition over the order a ranker
  produced, shared across every rail of one page view.
  **Gate**: with one member 100× ahead the page shows them once (mutation that turns the cap off: 3
  appearances); a 20-member pool against three shelves of six meets 3-of-6 tail cards per shelf (mutation
  that turns the share off: `0 card(s) below the median exposure (52.5), needs 3 of 6`); and with `0`/`0.0`
  declared, `/` is **`cmp`-identical** to the capture taken before the step ran.
  **Live**: `/@criticalnexus` went 4 cards → 1, `/@steeltitan` 4 → 1, `PASS exposure  1 slot per shelf, once
  per page` — and the honest consequence, the two-member home page is now one shelf with two cards.
  **Found and named**: the page's fourth rail ("Curator's picks") repeated the same members outside the
  caps, which made the gate false for the page as a whole; it now obeys the same rule, and the rail's
  operator authorship is the next step, because "the operator authors nothing" is an invariant and a rail
  named after a hand is that violation wearing a label. **148 tests** (65 unit, 54 HTTP, 7 store, 5 taste,
  4 eval, 4 concentration, 4 dial, 4 silence, 4 attention, 2 fame).
- **2026-11-10** `8ea3066`…`d44cdfb` — **step 1, amortised fame**: `Top` reads a 90-day half-life; the monument tab
  keeps the lifetime record. Two columns in the same stats fragment — `recent` (attention × `0.5^(age/90)`) and
  `attention` (lifetime) — so a surface cannot read the wrong one by accident; the monument is lensless,
  undamped and D39-exempt, and prints full digits.
  **Gate**: a fixture of 10,000 clicks 600 days old against 200 three days old — `Top` orders the recent one
  first, the monument the old one, and `recent` = 98.4 (600/90 half-lives) as hand-computed. The mutation
  that removes the half-life kills both gate tests (`the half-life must have done its work: 10000`).
  **Live**: `PASS fame  Top reads a 90-day half-life; the monument keeps the lifetime total — leader
  @criticalnexus 131.8 weighted over 90 days / 76 clicks all time`; `/v/dev?tab=top` says `74 visitors`,
  `/v/dev?tab=monument` says `76 clicks, all time`, doctor 0 failures. The ordering itself is unchanged
  (every real click is ≤12 days old, decay 0.91) — the decay is proved on the fixture, not claimed on the
  live pool. **144 tests** (59 unit, 51 HTTP, 7 store, 5 taste, 4 eval, 4 concentration, 4 dial, 4 silence,
  4 attention, 2 fame).
- **2026-11-10** `e37cfa1`…`da085fb` — **step 1, attention weighting**: a click is worth what a declaration says. `raw` reads
  the raw count (`clicks / max` — the arithmetic of every version before this one), `damped` reads the
  corrected column: one hit per session (a fan looking ten times is one person), each weighted `1 + ln p` by
  the slot it was seen in, log-normalised, then compressed by `alpha` (default 0.50). `ATTENTION_FILE`
  declares the mode and alpha; `orbital doctor` prints the promise, not the constant.
  **Gate**: the 200-seed identity test fails when `raw` is mutated to damped (`seed 0: raw must not
  re-rank anything`); a 10× lead lands on `(ln 11 / ln 2)^0.50 = 1.86×` against a declared 1.86; and on real
  rows *a second click from the same session leaves attention at 1.000 while the raw count goes 1 → 2*.
  **Live**: `doctor` reads `damped: a lead of 10 times becomes 1.86 times (alpha 0.50)`, switching to
  `raw` reports `a lead of k times stays k times`, and **the eval reproduces the pre-change numbers
  byte-identically** — `A 0.8619 / B 0.9073, delta +0.0454 [+0.0085, +0.0823]`, the same run as before this
  step — which is the strongest available proof that `raw` is the old arithmetic and not a lookalike.
  `damped` and `raw` return the same For You order on today's pool (76 vs 110 clicks compresses 0.076 to
  0.047 without crossing the lens term); the compression is proved on the fixture, the identity on the live
  data. **141 tests** (59 unit, 50 HTTP, 7 store, 5 taste, 4 eval, 4 concentration, 4 dial, 4 silence,
  4 attention).
- **2026-11-10** `7d5c306`…`9936f0c` — **step 1, the silence gate**: `src/silence.rs` holds the prior's vocabulary and a
  whole-word matcher, and `tests/silence.rs` proves the matcher **can fail** on four planted leaks (and stays
  quiet on `priority`, `gravityfed`, `Reprioritise`). `tests/http.rs` walks eighteen surfaces a stranger can
  reach and requires all of them silent with a declaration naming a source "the regulars". `scripts/check-silence.sh`
  runs the same gate against the deployed host inside the deploy script, where `/api/v1/capabilities` lives.
  **`/api/v1/taste`** now answers the caller's own lens: `{"tags":{"rust":1.0},"dial":{...}}` with
  `cache-control: private, no-store` and a weak ETag. **Gate, live**: `OK silence: every public surface is
  silent` — 14 pages and API answers silent, `/api/v1/capabilities silent but for taste:off`,
  `/api/v1/taste caller's lens only`. **133 tests** (55 unit, 50 HTTP, 7 store, 5 taste, 4 eval, 4
  concentration, 4 dial, 4 silence). Two leaks the gate found on its first honest run: `/` carried the word
  "folded" in an inline script comment, and a guard from the previous turn required `/api/v1/taste` to 404.
- **2026-11-10** `047f6ef`…`25bf1a0` — **step 1, the per-surface gravity policy**: each surface declares
  its own `weight` (relevance-led order), `ordered` (velocity-led: trending, new, rising) and `ceiling`, in
  one JSON file (`GRAVITY_FILE`); a declared weight is applied **against the share the ranker already uses**,
  so the built-in declaration is a multiplication by 1.0 and changes nothing. `orbital doctor` prints four
  lines, one per surface. **Gate, in the suite**: one dataset, one number changed at a time — cutting the
  vertical to a quarter hands `/v/{slug}` back to the traffic (`["robi","zara"] → ["zara","robi"]`), the
  home page and the ring stay byte-identical, and changing the home's weight does not reach the vertical;
  the same test run against the previous `src/` fails on the vertical's order. **Gate, live**: `doctor`
  reads `dial/vertical weight 0.10` while the other three keep their numbers, and cutting **home** from
  0.40 to 0.10 flipped the For You shelf (`criticalnexus, steeltitan` → `steeltitan, criticalnexus`) while
  `/v/dev`'s order and the ring's `Location` stayed identical; the declaration was then removed and the
  four lines read `0.40 / 0.30 / 0.40` again with For You back to `criticalnexus` first, `0 failure(s)`.
  **127 tests** (55 unit, 48 HTTP, 7 store, 5 taste, 4 eval, 4 concentration, 4 dial).
- **2026-11-10** `461be69`…`ddf9b1b` — **step 1, the concentration baseline**: `concentration_snapshots`
  (one row per surface per ISO week), `gini` and `top_share` reproducing hand-computed values
  (**0.8909** for one member holding nine tenths of 999 impressions; **0.9099** for the top-10 share),
  recorded by `orbital snapshot` (now with `--as-of` to backfill a week) and printed by `orbital doctor`
  and nowhere else. Live on thinkcentre: `snapshot --as-of 2026-11-03` and `--as-of 2026-11-10` produced
  **2026-11-02** and **2026-11-09** (`profile gini 0.39, top-10 100.0% of 187 impressions over 3 members`;
  `vertical gini 0.30, top-10 100.0% of 5 impressions over 2 members`), `doctor` ends `0 failure(s)`, and
  the public surface did not move: `/api/v1/concentration` **404**, `/api/v1/stats` byte-identical
  (`{"clicks":186,…}`), the home page **0** occurrences of `gini`/`concentration`. **123 tests** (55 unit,
  47 HTTP, 7 store, 5 taste, 4 eval, 4 concentration, 1 dial).
- **2026-11-10** `4f4c2e2`…`474ff7b` — **step 1, `polaris-eval` adopted** with a committed golden set:
  **130 real sessions** that clicked a member card (the roadmap asked for 200; 130 is what the database
  holds, and the file records that instead of inventing 70), the rule and the floor of **30** beside the
  seeds. Live on thinkcentre, both branches of the gate:
  `VERDICT: better — 130 cases, 0 seeds missing, A 0.8619 / B 0.9073, delta +0.0454 [+0.0085, +0.0823]`
  (exit **0**), and the same code over a five-seed set prints
  `NO VERDICT — 5 cases, 0 seeds missing, A 1.0000 / B 1.0000 … only 5 paired readers; the floor is 30`
  (exit **1**, which blocks the change). **118 tests** (55 unit, 46 HTTP, 7 store, 5 taste, 4 eval,
  1 dial).
- **2026-11-10** `2d028ff` + `6fc818c` — **step 1, `polaris-gravity` adopted**: four declared ceilings (home 0.40,
  vertical 0.40, entity 0.25, ring 0.60 — the shares the rankers already applied, so a full lens is a
  multiplication by 1.0 and the ordering is unchanged), one stored column, one slider in the tuner.
  Live, two requests with a config change between them:
  `max="40" … value="25"` → *restart with the home ceiling at 0.10* → `max="10" … value="10"` → *restart
  back at 0.40* → `value="25"` again, so the row was clamped **on read** and never rewritten; `POST
  /prefs lens=41` → **400** `This list personalises at most 40%.`; fresh visitor sees 40%; the page
  contains 0 occurrences of `gravity`/`prior`/`attuned`. **114 tests** (55 unit, 46 HTTP, 7 store,
  5 taste, 1 dial).
- **2026-11-10** `b5b9f10` — **step 1, `polaris-taste-source` adopted** with the sources D30 declares
  (`criticalnexus` 0.3, `steeltitan` 0.7, `attuned` with a 20-row evidence floor, `everyone` last; no
  trust selector). Live `orbital doctor`:
  `PASS taste/criticalnexus absent: admitted 1, contributed 0 — admitted contributors left no evidence`
  (and the same shape for the other three), `PASS taste profile empty (no declared source contributed
  evidence of a kind it may carry)`; `/api/v1/taste` → **404**; the home page contains 0 occurrences of
  `taste`/`prior`/`attuned`. Boot refusal proven at process level: `TASTE_FILE` with `weight: 30` exits
  **1**, naming the field and the source (weight of criticalnexus, 30, outside 0.0..=1.0), before
  the DB is touched. **105 tests** (48 unit, 45 HTTP, 7 store, 5 taste).
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
