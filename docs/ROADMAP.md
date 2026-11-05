# Orbital — roadmap

State at the time of writing: **schema v5**, 91 tests (43 unit, 41 HTTP, 7 store), `orbital doctor`
green on the host, `scripts/smoke.sh` green, seven timers running. Last rewrite **2026-11-05**, the
catalogue direction (D21, D22).

This file is the honest list. "Done" means deployed and verified against the live site, not merged.

---

## 0. Direction change — 2026-11-05

Orbital stops being "a directory of members" and becomes **a catalogue of sites, with the people who
run them attached and claimable**. The member directory becomes one view over the catalogue.

Four decisions, taken with the owner today and recorded as D21/D22:

1. **Sites are first-class objects**, not rows in `links`. A site has a canonical domain, a category,
   tags, health, socials, similar sites, and zero or more people.
2. **Proportion is a surface property, not a row count.** The adult catalogue can hold 5,200 rows and
   still be a small proportion of what the site *shows*: it is a vertical, it is `noindex`, and it is
   absent from the home page, the map and default search results.
3. **The adult catalogue stays on this origin** (D9 holds) with those guarantees, because the
   alternative — a second hostname — reintroduces the five enforcement points D9 deleted.
4. **The home page leads with sites.** People are the layer you reach *from* a site page.

Plus the content rule: **auto metadata for every site** (title, description, favicon, category,
health) and a **hand-written one-liner only for the top ~200**. Orbital is a catalogue with a graph,
not a review site, and copying a review site's copy is the one thing it must never do.

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

### Phase 0 — spec and schema (no user-visible change)

1. **[DB] Migration 0006**: `sites`, `categories`, `site_people`, `site_socials`, `site_edges`,
   `import_runs`. Provenance on every imported row: `origin` (`member|curator|import|claim`),
   `source`, `source_id`, `imported_at`. Specified in SPEC §1b/§1c/§5b.
   *Gate:* suite green; an imported row carries provenance; a site with no category is rejected.
2. **[BE] Import pipeline**: resumable, idempotent per canonical domain, per-row outcome, and an
   `import_runs` receipt. This is the estate's import pattern (family cross-move 14) applied to a new
   corpus, not a new idea.
   *Gate:* run twice — second run inserts 0 and updates 0; a killed run resumes where it stopped.
3. **[OPS] Delete the smoke fixture's rows** and assert in the smoke script that the hidden user owns
   nothing. *Gate:* `select count(*) from links` drops by 5; the directory still renders.

### Phase 1 — the catalogue exists, behind a flag

4. **[OPS] TPD seed**: 5,200 rows — name, canonical domain, their category label. **No descriptions,
   no ratings, no copied copy, no mirrored taxonomy** (EU database right: extracting facts from a
   curated database is the accepted risk; copying its editorial content is not).
   *Gate:* `select count(*) from sites where origin='import'` = 5,200 minus dead domains, reported.
5. **[OPS] Health and metadata backfill** with the existing checker and `fetch-titles`, so "alive" is
   a column, not a hope. **Cost note:** the timer checks 500 links/day, so one pass over 5,200 rows is
   ~11 days — raising `--limit` or adding concurrency is part of this task, not a follow-up.
   *Gate:* every imported row has `last_checked` and `last_status`; the dead share is quoted.
6. **[FE] Make the vertical floor real and configurable.** Today the code hides *empty* verticals only
   (they fall into the "Other" row) and `min_members = 5` is selected but never compared. Every
   vertical has exactly one visible member, so enforcing 5 as written empties the index. The floor
   becomes a config value, introduced at 1 (no visible change), raised to 3 and then 5 as a vertical
   grows; empty verticals stay in the admin list so "adopt a vertical" can still target them.
   *Gate:* the floor is read from config; a test at floor 5 asserts the empty state renders rather
   than a blank page or a 500.

### Phase 2 — a site page worth landing on

7. **[FE] `/s/{slug}`**: title, description, favicon, category, tags, health line, "people here"
   (claimed and unclaimed), socials, similar sites, and which members link to it.
   *Gate:* a dead site says so; a site linked by two members shows both.
8. **[FE] Similar sites** from the existing rarity-weighted `cosine()` — one similarity implementation
   for members and sites. It moves to the family's embedding neighbour search when a second surface
   needs it.
   *Gate:* symmetric, and non-empty for any site with ≥2 tags.
9. **[FE] Home leads with sites**: three shelves (newest healthy, most-clicked, curator picks), adult
   excluded by policy. The member list moves behind `/members`.
   *Gate:* view-source on `/` contains no adult row; each shelf is one SQL statement.
10. **[BE] Search over sites** with an explained score ("matched tag, 2 hops from X"), shaped like the
    family `polaris-search` interface rather than inventing a third query language.
    *Gate:* 10 fixture queries return a reason string for every result.

### Phase 3 — people attach and claim

11. **[DB] Shell people rows**: `password_hash` becomes nullable, `claimed_at` added, and a login guard
    refuses a null hash. One migration; every existing join (tags, verticals, prefs, ring, feeds)
    keeps working, which a separate `creators` table would not.
12. **[BE] Claims**: proof on the person's own page (pull, never mail), the existing Bluesky
    `verify_token` flow, or DNS TXT. Single-use tokens, per-curator cap, merge-on-claim.
    *Gate:* a claim with no proof is refused; one whose proof disappears within 24h is reversed.
13. **[BE] Witness on contest only**: a second claimant escalates to one TL2 witness plus a 24h dispute
    window. No three-TL3 quorum for claims — there is one TL3, and D4 already recorded that a quorum
    without a population never closes.

### Phase 4 — the graph

14. **[FE] Site ego graph** (SSR SVG, radial, two hops): site → people → socials → similar. No JS
    library and no canvas: the commodity-hardware priority forbids a renderer that needs a CDN.
    *Gate:* the SVG is in the HTML and legible with JavaScript disabled.
15. **[FE] Category map**: a tag-space projection, top N per category, from the same similarity edges.
    A global map of 5,200 nodes is a hairball, so the global view is a list of category maps.
    *Gate:* rendering one category does not scan the whole table (an `EXPLAIN` shows an index scan).

### Phase 5 — the loops, once there is a population

16. **[BE] Ring embedding as a membership condition**: the widget exists; the contract is "you join,
    you embed, you get traffic from the ring". Dormant today (0 ring hits).
17. **[OPS] Weekly post** from the estate's working transport: one new site, one claimed person. Not a
    digest email — there is no email.
18. **[FE] Site rank snapshots** and "you moved 12 → 9" for claimed people, on the existing
    `rank_snapshots` + `rank-delta` machinery.
19. **Only then**: pairwise picks (concord cross-move 4), the family taste substrate, XP and loops.

## 5. Shared with the family — reuse, do not rebuild

| Need | Family component | State | What Orbital does |
|---|---|---|---|
| Accounts | `polaris-id` (W2) | live, 2 sites wired | local accounts today; adopt at W2, one realm per audience |
| Pagination, contract | `polaris-core 0.2.0`, `x-audience`, `GET /api/v1/capabilities` | live | adopt the vocabulary; the read API already speaks ETag |
| Taste / lens | `polaris-taste` (W3) | live — ballots, Glicko, weights, fingerprints | `preference_weights` is a local copy of the same substrate; do not extend it separately |
| Search | `polaris-search` (W3) | live, 98,180 docs | site search is a client, not a new index |
| Similarity | kindred neighbour search (cross-move 7) | live | `cosine()` today; migrate when a second surface needs it |
| Import | darkhold/AO3 import (cross-move 14) | live | the catalogue importer copies the pattern: resumable, provenance, per-row outcome |
| Link health | orbital's own (cross-move 13) | live | already the estate's; the catalogue makes it load-bearing |
| Trust levels | orbital's TL → estate-wide (cross-move 3, idea 117) | orbital live | claims use TL2 witnesses; the family adopts the scale via `polaris-authz` |
| Claims, disputes, moderation | `polaris-ledger` (W7) | specified | a claim is a ledger entry, not a new table — wait for the ledger rather than inventing one |
| Export, preservation | `polaris-export` bundle (W5) | live | a catalogue that cannot be exported dies with the host |
| Chrome, tokens, components | `@polaris/ui`, nginx topbar (W1) | live on 16 hosts | orbital takes the chrome; the map is a domain-specific component it may own |
| Attention data | `estate-gates` pageviews per host (W10) | live | the only traffic denominator in the estate |

Rule of two still applies: do not extract a shared package on one caller, and do not fork a family
component to save a day. Where orbital and the family disagree, the family decision wins unless it
contradicts D9 (one surface) or D21 (proportion is a surface property).

## 6. Deliberately not done

- **Inventing members or site descriptions.** Real sites with honest metadata, or nothing — and this is
  the one thing empty-state pressure must not erode.
- **Ranking and taste tuning** until one vertical holds 30 real members. Frozen, not deleted.
- **Monetisation.** `docs/monetization.md` stays a design: at 962 pageviews/week network-wide an ad
  slot is worth less than the footer promise it would break.
- **Page-body i18n.** Chrome translated; bodies English until members need otherwise.
- **Email, OAuth, custom domains** (D7). Claims are pulled, never mailed.
- **Federation.** Family W6, last, and only behind a bridge policy.
- **A second comment tree, notification table or search index.** The estate has one of each.
- **Copying a review site's editorial content or taxonomy.** Facts only.

## 7. Known risks

- **The adult catalogue changes what this domain is.** 5,200 porn sites on the same origin as
  everything else is a different classification exposure from one adult vertical. Surface exclusion
  and `noindex` reduce it; they do not eliminate it.
- **A catalogue of strangers is still an empty room.** 5,200 imported rows nobody claims moves the
  empty-state problem from "no rows" to "no signal". Health, claim badges and curator picks are the
  answer, and pageviews are still the judge.
- **EU database right.** Substantial extraction from ThePornDude's curated list is a known, accepted
  risk; copying their descriptions, ratings or category structure would make it an obvious one.
- **The link checker becomes the catalogue's quality gate**, and 5,200 rows at 500/day is an 11-day
  pass. A link that dies at 04:21 is offered for almost a day today; at catalogue scale that is a
  backlog unless the limit and concurrency move with the schema.
- **`min_members` is a dead column.** It reads 5 on every row while every vertical has exactly one
  visible member; enforcing it as written empties the index. The floor must be configurable and start
  where the data is, or the one-page fix becomes an empty home page.
- **Cloudflare in front of everything** (unchanged): every rate limit keys on `CF-Connecting-IP`, and
  a request that bypasses the tunnel keys on the socket.

## 8. Where the truth is

`docs/SPEC.md` (§1b/§1c objects, §5b tables), `docs/DECISIONS.md` (D21 catalogue, D22 claims),
`docs/monetization.md` (measured traffic), and the vault note
`~/secondbrain/10-Projects/2026-10-29-orbital-directory-live.md`. When a route or command is added all
of these move in the same pass, and every route a doc lists is proved with
`curl -s -o /dev/null -w '%{http_code}'`.
