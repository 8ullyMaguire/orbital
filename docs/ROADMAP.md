# Orbital — roadmap

State at the time of writing: **schema v5**, 91 tests (43 unit, 41 HTTP, 7 store), `orbital doctor`
green on the host, `scripts/smoke.sh` green, seven timers running. Last rewrite **2026-11-05**, the
catalogue direction (D21, D22).

This file is the honest list. "Done" means deployed and verified against the live site, not merged.

---

## 0. Direction change — 2026-11-05 (v3)

Orbital is **a taste-ranked catalogue of anything, written by its users, federated at the object
level**. Not "a directory of members", and not "a directory of sites" either: an entity is a person, a
website, a film, a book, a product, a fanwork. Members review entities, ballots between entities train
the taste vector, and every surface pulls toward the instance's taste — including adult, behind a
per-viewer opt-in.

Decisions, recorded as D23–D26 (D21 is superseded in scope, D9 still holds):

1. **Anything is an entity** with one identity: `canonical_key` = a normalised URL or an external
   scheme (`imdb:`, `isbn:`, `musicbrainz:`, `atproto:`). `same_as` merges duplicates without deleting
   anything.
2. **The operator writes nothing.** Metadata is fetched or user-written; edits are user content with a
   diff and a revert; the operator's power is override, not authorship.
3. **Gravity applies to everything, adult included**, gated per viewer: anonymous first-time is safe,
   opt-in unlocks adult in ranking and search, the taste term stays ≤25% with a novelty slot.
4. **Federation is gravity's ActivityPub stack**, per-user actors, reviews joined on canonical keys,
   inbound quarantined by default.
5. **The mind map is dropped** (owner's call): it was a nice object and not a reason to visit.

What that does to the earlier plan: the `sites` table becomes `entities`; the TPD import becomes one
source among several; the "top-200 hand-written one-liners" is gone; the adult surface exclusion is
replaced by the per-viewer filter; phases reorder around reviews and ballots, with federation as the
last phase rather than a parked idea.

## 1. The measurements that set the order

| Measure | Value, 2026-11-05 |
|---|---|
| Visible members | 2 (one is the operator) |
| Links | 24, across **19 distinct domains**, 5 failing 3+ checks |
| Logins | **0 since 2026-10-29** (17 sessions, all that day) |
| Profile views | 469 (356 of them the operator's own profile) |
| Vertical navigations, all time | **5** — and 0 ring hits |
| Visitor profiles | 1,465; **5** reached the personalisation warm-up |
| ThePornDude inventory | **5,200** distinct site reviews (112,461 sitemap URLs, 27 languages) |

What that rules **out** for now: ranking and taste tuning, endorsement weights, monetisation, page-body
i18n, digests, and any claim that browsing works. A ranker over two members is a decimal point on a
coin flip.

What it rules **in**: inventory, a site page worth landing on, health as a quality gate, claims, and
the one graph that is cheap with no audience — a site's own neighbourhood.

## 2. What the outside critique got right, and the three places it was wrong

Right, and taken: freeze the ranking (below); hide empty verticals (`min_members = 5` already exists
on every row, is compared to nothing, and the code hides *empty* verticals only); make embedding the
ring widget a condition of membership; recruit personally rather than waiting; post weekly from the
transport that already works; save HN/Lobsters until there are 20+ real entries; stop translating page
bodies.

Wrong — written down so it is not re-derived:

- **"Seed 10 members from your 16 sites."** Those 16 sites are all mine. That cohort is one person.
  The real supply is *sites* — verifiable, claimable, not invented — and other people's sites.
- **"There is no self-serve onboarding."** There is: handle + password + primary vertical, live since
  day one. What is missing is a reason to use it, which is a catalogue problem, not an auth problem.
- **"Delete the sophistication."** The ranking code is written, tested and mutation-checked; freezing
  it costs nothing and deleting it costs a rewrite. The cost was always the *tuning*, not the code.

The critique's strongest point stands and is why this file changed: an empty directory is the
conversion killer, and the answer is inventory — **real sites, never invented people**.

## 3. What exists

**The product.** Profiles, links, tags, one primary vertical and two secondaries; Top / Trending / New
/ Curated per vertical, webring, feeds, embeddable leaderboard, sitemap and robots. Server-rendered
pages (askama), Svelte dashboard embedded in the binary. Click attribution through `/l/{id}` with
referrer host only and distinct-session counting. Link health with an SSRF guard including redirect
hops. One surface (D9). Personalisation: a lens per member or anonymous visitor, passive accumulation
with a five-page warm-up, `/lens`, an off switch, adoption at signup, 24-tag cap. Governance:
proposals, three-vote quorum, merges that keep tags, TL2+ endorsements as a separate +0.2 term. API
tokens and a write API; `rank-delta` from weekly snapshots. Fetched titles and favicons, generated
social cards. `/status` — the estate as nginx actually serves it. English and Spanish chrome.

**The operations.** `orbital doctor` (exit code = failures), `import`, `snapshot`, `retag`,
`fetch-titles`, `og`, `prune`, `check-links`. Seven user timers (checklinks, backup, restore-test,
prune, snapshot, metadata+cards, uptime). Nightly dumps to `.part` then rename, 14 kept, monthly
restore test into a scratch database. nginx rate limits on the API and the unauthenticated write path,
CORS on the read API, no `Set-Cookie` on API responses. CI on both forges against a Postgres service.
A smoke test that hides its own fixture.

**Not cleaned up yet:** the smoke fixture's five `example.com` links still sit in production under a
hidden user (`links` = 24, 19 distinct domains). Phase 0 deletes them.

## 4. Next, in order

Each phase ships alone and is verified against the live host. Tags: **[DB] [BE] [FE] [OPS]**.

### Phase 0 — entity identity and the importer (no user-visible change)

1. **[DB] Migration 0006**: `entities`, `categories`, `entity_tags`, `entity_edges`,
   `entity_people`, `reviews`, `review_votes`, `entity_edits`, `entity_scores`, `ballots`,
   `import_runs`; `links.entity_id`; nullable `users.password_hash` + `claimed_at`. SPEC §5b.
   *Gate:* suite green; a duplicate `canonical_key` is refused; `canonical_key` normalisation has
   table-driven tests (`www.`, trailing slash, fragment, uppercase host).
2. **[BE] `canonical_key` resolution**: URL → key, and an external-scheme parser for `imdb:`,
   `isbn:`, `tmdb:`, `musicbrainz:`, `atproto:`. One function, used by the importer, the review form
   and the inbound federation path — the key is the join, so there must be exactly one implementation.
   *Gate:* round-trip tests from 20 sample URLs and ids; two spellings of one URL produce one key.
3. **[BE] Importer**: resumable, idempotent on `canonical_key`, provenance on every row, an
   `import_runs` receipt. The estate's import pattern (family cross-move 14) applied to a new corpus.
   *Gate:* run twice — second run inserts 0 and updates 0; a killed run resumes where it stopped.
4. **[OPS] First sources**: ThePornDude's 5,200 adult sites (facts only: name, domain, category), and
   a non-adult seed from a public list (OpenLibrary for works, an awesome-list for dev resources) so
   the catalogue is not 99% one category. **Health and metadata backfill with the existing checker and
   `fetch-titles`; the 500/day limit must move with this task or one pass is an eleven-day backlog.**
   *Gate:* every imported row has `last_checked`/`last_status`; the dead share is quoted per source.
5. **[OPS] Delete the smoke fixture's rows** and assert the hidden user owns nothing.
   *Gate:* `links` drops by 5; the directory still renders.

### Phase 1 — read, review, rate

6. **[FE] `/e/{slug}`**: an entity page — kind, title, description, image, category, tags, health,
   people, similar entities, rating summary, and the reviews ordered by helpfulness.
   *Gate:* a dead site says so; a film and a site render from the same template with no `if kind ==`.
7. **[BE] Reviews**: create/edit/delete, 1–5 stars, markdown body, length cap, one review per user per
   entity, edit history kept. *Gate:* a second review by the same user updates rather than duplicates;
   a deleted review keeps its tombstone for federation.
8. **[BE] Ballots**: "which of these two?" on any listing; one vote per pair per user; feeds
   `preferences` and the entity score. *Gate:* a ballot with two identical entities is refused; a
   user's vector changes after 10 ballots and the change is explainable.
9. **[FE] Home from gravity**: three shelves (for you, new, rising) computed from the viewer's vector
   with the instance vector as the fallback, each card carrying a `why` line. Adult rows appear only
   for a viewer who opted in.
   *Gate:* an anonymous request gets zero adult entities in HTML, JSON, feed and sitemap (the D25
   security-boundary test).

### Phase 2 — user-written metadata, operator-minimal

10. **[BE] Metadata edits**: any user edits title/description/category/tags; TL0–TL1 go to a queue,
    TL2+ apply immediately; every edit is a diff with revert.
    *Gate:* a TL0 edit is invisible until approved; a revert restores the previous text exactly.
11. **[BE] Report and quarantine queue**: reports on reviews, edits and entities; quarantine removes
    from search and ranking without deleting; operator can allowlist a domain or a user.
    *Gate:* a quarantined review is 404 to anonymous and 200 to its author and to a moderator.
12. **[OPS] Operator-absence drill**: run for 30 days with no operator action. Every queue must age
    out or self-resolve, and no surface may go stale without an author.
    *Gate:* after 30 days, the only items awaiting the operator are exceptions that name themselves.

### Phase 3 — the gravity well, measured

13. **[BE] Instance taste vector**: the operator's own signals plus the member aggregate, contributed
    at ≤25% of every score and always separable in `why`.
    *Gate:* setting the operator's vector to zero changes orderings by less than the cap and the `why`
    strings disappear rather than lie.
14. **[BE] Offline eval** for the ranker before it ships (family cross-move 19, kindling's harness):
    two settings over the same seed, overlap and nDCG compared, `NO VERDICT` below the sample floor.
    *Gate:* a run that cannot beat the recency baseline says so.
15. **[FE] Novelty slot** per surface, so a taste-heavy page still shows something it did not predict.
    *Gate:* a fixture user who likes exactly one tag still sees at least one entity outside it.

### Phase 4 — claims (who owns what)

16. **[BE] Entity ownership**: a person claims their own entity (proof on their page, Bluesky token,
    DNS TXT), a site owner claims their site. Merge-on-claim for duplicates.
    *Gate:* a claim with no proof is refused; a claim whose proof disappears within 24h is reversed.

### Phase 5 — federation

17. **[BE] Spike, timeboxed**: depend on gravity's AP layer (or lift it into `polaris-core`) and
    publish one actor + WebFinger for the instance. *Gate:* `curl -H 'Accept: application/activity+json'`
    on the actor returns a valid document; a real Mastodon instance resolves the handle.
18. **[BE] Per-user actors and outbound reviews**: `Create`/`Update`/`Delete` of a `Review`, delivered
    to followers, with the `Note`+schema.org fallback for peers that do not know the type.
    *Gate:* a review appears in a Mastodon timeline with its rating visible, and the `ap_id` is the
    same object on re-delivery.
19. **[BE] Inbound**: `Follow`, `Create` for reviews, canonical-key resolution, unknown key creates the
    entity, unseen domain quarantined. *Gate:* a review from a second local instance lands on the
    right entity page and stays out of search until approved.
20. **[OPS] Domain allowlist, blocklist and a monthly moderation review** — the permanent cost of
    federation, staffed by the operator only as an exception path.

### Deliberately not in this plan

- **The mind map.** Dropped on the owner's call; the `entity_edges` data it would need is collected
  anyway, so it can return as a view without a migration.
- **Seeded members, invented reviews, or operator-written copy.**
- **Ranking/taste tuning beyond the eval gate** until one category holds 30 real reviews.

## 5. Shared with the family — reuse, do not rebuild

| Need | Family component | State | What Orbital does |
|---|---|---|---|
| Accounts | `polaris-id` (W2) | live, 2 sites wired | local accounts today; adopt at W2, one realm per audience |
| Pagination, contract | `polaris-core 0.2.0`, `x-audience`, `GET /api/v1/capabilities` | live | adopt the vocabulary; the read API already speaks ETag |
| Taste / lens | `polaris-taste` (W3) | live — ballots, Glicko, weights, fingerprints | `preference_weights` is a local copy of the same substrate; do not extend it separately |
| Search | `polaris-search` (W3) | live, 98,180 docs | entity search is a client, not a new index |
| Similarity | kindred neighbour search (cross-move 7) | live | "similar entities" via `cosine()` today; migrate when a second surface needs it |
| Import | darkhold/AO3 import (cross-move 14) | live | the catalogue importer copies the pattern: resumable, provenance, per-row outcome |
| Link health | orbital's own (cross-move 13) | live | already the estate's; the catalogue makes it load-bearing |
| Trust levels | orbital's TL → estate-wide (cross-move 3, idea 117) | orbital live | claims use TL2 witnesses; the family adopts the scale via `polaris-authz` |
| Claims, disputes, moderation | `polaris-ledger` (W7) | specified | a claim is a ledger entry, not a new table — wait for the ledger rather than inventing one |
| Export, preservation | `polaris-export` bundle (W5) | live | a catalogue that cannot be exported dies with the host |
| Chrome, tokens, components | `@polaris/ui`, nginx topbar (W1) | live on 16 hosts | orbital takes the chrome; the map is a domain-specific component it may own |
| Attention data | `estate-gates` pageviews per host (W10) | live | the only traffic denominator in the estate |
| ActivityPub | gravity's AP layer (`activitystreams`, `webfinger`, `http_signature`, `discovery`, `federation.rs`, ~7,150 lines) | live | the stack orbital federates with; never a third implementation (D26) |

Rule of two still applies: do not extract a shared package on one caller, and do not fork a family
component to save a day. Where orbital and the family disagree, the family decision wins unless it
contradicts D9 (one surface) or D21 (proportion is a surface property).

## 6. Deliberately not done

- **Inventing members, entities, reviews or metadata.** Real objects with fetched or user-written
  metadata, or nothing.
- **Operator-written copy.** No editorial author, at any scale (D24).
- **The mind map.** Dropped; a view, not a phase.
- **A separate adult surface or hostname.** D9 holds; the filter is per viewer (D25).
- **Ranking and taste tuning** without the eval gate, and none at all until a category holds 30 real
  reviews. Frozen, not deleted.
- **Monetisation.** `docs/monetization.md` stays a design: at 962 pageviews/week an ad slot is worth
  less than the footer promise it would break.
- **Page-body i18n.** Chrome translated; bodies English until members need otherwise.
- **Email, OAuth, custom domains** (D7). Claims are pulled, never mailed.
- **A second comment tree, notification table, search index or ActivityPub stack.** The estate has one
  of each.
- **Copying another directory's editorial content or taxonomy.** Facts only, as before.

## 7. Known risks

- **The adult filter is a security boundary now.** D25 makes adult eligibility a function of the
  viewer, so a cookie, a cache key, a feed or a sitemap that ignores the layer leaks it. The probe is
  explicit: an anonymous request must never receive an adult entity, on any surface, and that is a
  test per surface rather than one middleware rule.
- **The domain's classification exposure is permanent.** 5,200 adult entities on the SFW origin, with
  adult eligible on every surface for opted-in viewers, is a standing risk to Cloudflare, the
  registrar and search. It is reduced by the filter, not eliminated by it.
- **Federation is a permanent moderation cost.** Quarantine by unseen domain means spam arrives with
  every new instance; a domain allowlist is a blocklist in disguise once it is long enough. This is
  the family's P4/P7 trade, accepted knowingly.
- **Entity identity will collide.** `imdb:` and `tmdb:` disagree, a site moves domain, a person has
  three. `same_as` plus `merged_into` is the answer, and merge bugs are the ones that lose reviews —
  the merge must move rows in one transaction and be tested with a rollback.
- **User reviews are user liability.** Defamation and illegal content are now the operator's problem
  the moment a review is published; the report queue, quarantine and a takedown address are part of
  Phase 2, not an afterthought.
- **A catalogue of strangers is still an empty room.** 5,200 imported rows with no reviews is the
  same empty state one layer deeper; the ballot loop and the review prompt on an entity page are what
  fill it, and pageviews are still the judge.
- **The link checker becomes the catalogue's quality gate**, and 5,200 rows at 500/day is an eleven-day
  pass — the limit and concurrency move with the importer or the backlog is structural.
- **Cloudflare in front of everything** (unchanged): every rate limit keys on `CF-Connecting-IP`.

## 8. Where the truth is

`docs/SPEC.md` (§1b entity, §1d reviews, §1e gravity, §5b tables, §6b federation), `docs/DECISIONS.md` (D21 catalogue superseded by D23–D26, D22 claims),
`docs/monetization.md` (measured traffic), and the vault note
`~/secondbrain/10-Projects/2026-10-29-orbital-directory-live.md`. When a route or command is added all
of these move in the same pass, and every route a doc lists is proved with
`curl -s -o /dev/null -w '%{http_code}'`.
