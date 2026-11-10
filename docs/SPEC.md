# Orbital — SPEC

> **Ranking and discovery are superseded by [`SPEC-V3.md`](SPEC-V3.md)** — preference profiles,
> tag-weighted ordering and the personalised ring. Everything else here still applies.

A themed directory of people and their links. Each member gets one profile page

> **Gravity, defined across the estate (2026-11-10).** Two fields blended on every ranked surface:
> each member link carries an **instance field** (the folded taste of declared sources) and each reader
> a **reader field** (their own taste, decaying). A surface ranks `(1 − g(u)) · local + g(u) · instance`
> — Orbital's per-surface `weight` is `g` and its `ceiling` is `g_max`, with `g(u)` meant to rise as a
> function of the reader's own signals, not their wall-clock tenure. Never disclosed; aggregate only;
> no anonymous route may rank the directory by it. Definition: `10-Projects/gravity-model.md`.
(links, bio, taste tags), one **primary vertical** and up to two **secondaries**,
and appears on that vertical's ranked list. Verticals are micro-communities with
their own leaderboard, their own ring, their own feed and their own embeddable
widget.

**North star: the number of people who discover a member's profile.** Every
decision below is ranked against that, not against feature parity with the draft.

Live: `https://orbital.polarisocial.xyz` (SFW) and
`https://orbitalnsfw.polarisocial.xyz` — retired as a surface by DECISIONS D9; it now answers with a
301 to the equivalent path on the main host.

---

## 0. What changed from the v1 draft, and why

The draft was written before anything was measured. These are the deviations.
Every one of them is implemented as described here; the draft's version is gone.

| # | Draft | Built | Why |
|---|---|---|---|
| D1 | SvelteKit frontend for everything | Rust renders **all public pages** (askama); Svelte 5 is the authenticated dashboard only, compiled to static assets and embedded in the binary | Discovery is the product, and discovery starts with a crawler. A client-rendered directory is invisible to search engines and slow on a phone. One binary, no node runtime on the host |
| D2 | `top_score = total_clicks_all_time` | `top_score = unique visitors, all time` (distinct `session_hash`) | Raw clicks is the most gameable metric there is: one `curl` loop wins the leaderboard. Unique visitors per session is the same shape, legible on the page, and a click farm needs distinct sessions to move it. Clicks are still stored and shown |
| D3 | `New` tab is TL1+ only | `New` lists every listed profile, most recent first | A brand-new member is TL0 by definition, so TL1+ would hide new blood from the one tab whose entire purpose is "join early and be seen". Trust is expressed as a multiplier and as a visibility rule for NSFW, not as an exclusion from New |
| D4 | Trust levels + quorum moderation (3 TL2+ votes, 7-day window) | Trust levels and **vertical-scoped flag queues** exist; adjudication is owner/curator resolution, not a vote | A quorum needs a population. With one operator, a vote is a form that never closes. The vertical scope is kept because it is the part that protects culture; the vote is a documented non-goal until there are enough TL3s to constitute one |
| D5 | Co-occurrence matrix built per vertical + globally | Similarity is computed **in SQL** from the tag sets themselves (cosine over weighted tag vectors), per vertical and globally | A materialised matrix is a cache with no invalidation story: it is stale the moment someone edits a tag, and nobody notices. Postgres computes the same cosine for a few thousand profiles in microseconds |
| D6 | NSFW = "subdomain or gated path, your call" | Subdomain, host-scoped server-side, plus cookie age gate | Host scoping makes the segregation a property of the request, not of every template that might forget to check a flag |
| D7 | Email + ATProto signup flows | Handle + password. Optional **Bluesky handle claim** proven by a token in the account's profile description. No email | This host has no SMTP transport (verified: the only mailer in the estate logs to the journal). An email-confirmation signup would issue tokens nobody can receive |
| D8 | Bot can post to Bluesky | `orbital bot` subcommand + systemd timer, app-password auth, spotlight rotation and weekly roundups, `bot_posts` ledger so nothing repeats | Same mechanism already proven in the estate (`polarisocial-hub/announce.py`), in-process so the deploy is one unit |

Not shipping, deliberately: multi-tenant instances (draft phase 9), the LinkStack
button library (brand SVGs are inlined by hand for the ~28 link kinds used),
custom domains per member (a nginx `server_name` file per member is an ops
burden with no member demand; `sameAs` links cover the identity need).

---

## 1. Objects

### Member (profile)
`handle`, `display_name`, `bio`, `accent`, `avatar_url` (optional; otherwise a
deterministic gradient identicon with initials), 3–10 `tags`, one primary
vertical, up to two secondaries, links. Trust level 0–3.

### Vertical
Curated enum, own route, own leaderboard, own ring, own feed, own embed.
Starter set: `gaming`, `podcasters`, `youtube`, `music`, `art`, `dev`, `writers`,
`vtubers`, `indie-web`, `nsfw`. Membership roles: `primary`, `secondary`.
A vertical with fewer than 5 visible members does not get a landing page — it
appears in the "Other" row of the main directory and its members still rank if it
is reached directly. (Implementation note, 2026-11-05: the code hides *empty*
verticals only; `min_members` is selected and never compared. Enforcing it at 5
today would empty the home page, since every vertical has exactly one visible
member — ROADMAP phase 1 makes the floor a configurable value and starts it at 1.)
`nsfw` is served from the same origin as every other vertical (D9).

### Link
`url`, `label`, `icon` (enum), `position`, optional `vertical_slug` scope.
Scope semantics:
- `NULL` → shown everywhere the member is shown.
- set → shown **only** while that member is viewed in that vertical's context.
The NSFW card is the same mechanism: an adult-scoped link renders only for a viewer whose filter
allows it (D25). The safe layer never shows it, and a safe-layer page carries no indication that it
exists.

### SourceHit
One row per outbound click: member, link, surface (`profile`, `vertical`,
`embed`, `direct`), the vertical context, the slot position, the session hash.
No IP, no user agent, no referrer path beyond the vertical — the analytics are
per-member and aggregate, and the raw table is pruned at 180 days.

### ProfileView
One row per profile render, deduped per (member, session, day) — the denominator
of every conversion claim the dashboard makes.

### Flag
Vertical-scoped when the flag came from a vertical page. A moderator resolves
with a reason. Resolving "ranking manipulation" zeroes the member's ranking
contribution and resets trust to TL0.

---

## 1b. Entity — anything reviewable (D23)

The object is not a site. Anything that can be rated and reviewed is an **entity**: a person, a
website, a film, a book, a record, a place, a product, a fanwork, a feed. One table, one page type,
one review model, one ranking.

- `kind` — `person | site | work | place | product | other`: a display and filtering hint, not a
  behaviour switch.
- `canonical_key` — the identity. A normalised URL (`url:https://example.com`) for anything with a
  home, or an external scheme (`imdb:tt0133093`, `tmdb:movie:603`, `isbn:9781526622426`,
  `musicbrainz:release-group:…`, `atproto:did:plc:…`) for works and people. Unique, so two users
  adding the same film get one page.
- `same_as` edges — the merge path when two keys describe one thing (a site and its Wikidata entry).
  Merging moves reviews, ratings and tags and leaves a redirect; nothing is deleted.
- Metadata: `title`, `description` (fetched from the URL or written by a user — never by the operator,
  D24), `image`, `category`, tags, `nsfw` (derived from the category and from flagged reviews), health
  for anything with a URL.
- Provenance: `origin` (`user | import | federation`), `source`, `source_id`, `imported_at`. An
  imported row is a starting point, not an authority: any user may edit its metadata, and the edit
  history is the record.

Rules that are spec:

- **A review attaches to an entity by `canonical_key`, not by local id.** That is what lets a remote
  instance review the same film without knowing our UUIDs (D26).
- **Nothing is deleted for being unreachable.** A dead site keeps its page and says so.
- **A person is an entity too**, so people are reviewable. Reviews of a person carry the same
  report/queue path as everything else, and a person can claim their own entity.

## 1c. Category and tags

**Categories** are subject matter — a curated enum with `sort` and `nsfw`. **Verticals** are who a
member is. They are separate axes and must not be conflated. Tags are free text, canonicalised and
aliasable through `tag_aliases`, which the estate adopts as its thesaurus (family cross-move 52).

## 1d. Review and rating (D24)

A **review** is a user's text plus an optional **1–5 rating**, on one entity, by one user; editable by
its author, removable by its author or a moderator. Fields: `entity_id`, `user_id`, `rating`
(nullable), `body` (markdown, length-capped), `language`, `nsfw`, `created_at`, `updated_at`,
`ap_id` (once federated), `visibility`, `deleted_at`.

- **Stars are for display and aggregation.** An entity's `rating_avg` and `rating_count` are derived
  and recountable, never the only copy.
- **Taste comes from pairwise ballots, not from stars.** A star is a claim about an object; a choice
  between two objects is a statement about the chooser, and at a few dozen users the pairwise ballot
  is the only signal that survives. Ballots feed one vector per user and one per instance.
- **Review quality is itself reviewable**: `helpful` votes, and a review's own aggregate orders the
  reviews under an entity. Every ordering explains itself (family cross-move 2).
- **Metadata edits are user content too**: an edit is a small review-shaped act with a diff, a
  reporter and a revert. The operator is never the author of record.

## 1e. Taste gravity (D25, revised by D27 and D30)

Every surface orders by a blend that includes a **taste term**: the viewer's vector (their ballots,
stars, reviews, clicks) and the **instance's** prior, so a fresh visitor's first page looks like the
instance rather than like a random draw.

The prior is folded from **declared sources** (D30) — named accounts with their own weights, users at or
above a trust level, readers with high affinity to the viewer, and everyone weighted by affinity — and it
is **per surface**: home, a vertical, an entity page and the ring each declare their own strength and
ceiling (D29).

Constraints that keep it safe:

- the taste term is **at most 25% of any score** (a declared ceiling, not a constant in code);
- a **novelty slot** on every surface, so gravity never becomes an echo;
- **the prior is silent** (D27): applied to ordering and disclosed nowhere — no `why` string, field,
  page, badge, class name, URL parameter, feed, sitemap or error message may reveal that it exists or
  what it contains. A `why` line may cite clicks, recency, the viewer's own tags, endorsements and link
  health, and nothing else. `docs/` is the only place the policy is written;
- **adult content is eligible on every gravity surface** — it is not a separate surface — but behind a
  per-viewer opt-in: a first-time anonymous visitor gets the safe layer, and the choice is a cookie
  (anonymous) or an account setting. The three layers (safe / opt-in / everything) are read by the
  home page, search and `/lens` alike.

## 2. Rankings

**Top** — `unique_visitors_all_time` per member, within the vertical (a member's
visitors in that vertical context + their unscoped traffic). Monument: no decay.

**Trending** — last 7 days:

```
trending = (visitors_7d / max(visitors_7d) over the vertical)
         × (1 + 0.15 / log2(account_age_days + 2))
         × trust_mult(trust_level)          # 0.5 / 0.8 / 1.0 / 1.1
```

A member with zero visitors in the window is **not** on Trending; the tab is
hidden when empty rather than backfilled with zero rows (a zero-filled "rising"
list is a recency list wearing a costume).

**New** — `created_at DESC`, all listed members, badge for <14 days.

**Curated** — *retired* (D44). There is no owner-pinned list on any surface: the operator authors nothing, and
a rail headed "hand-picked" announced the one thing the instance must never announce. The `curation` table
survives until a migration can drop it without taking history with it, and nothing reads it.

**Similar profiles** (sidebar):

```
0.35 · cosine(tags_candidate, tags_viewer)
0.25 · cosine(tags_candidate, tags_owner)
0.20 · vertical_match   (1.0 same primary, 0.5 secondary, else 0)
0.10 · recency(last_active_at, 30-day half-life)
0.10 · trust_level/3
```

## 2b. The home shelves

The home page carries three shelves — `for_you`, `rising`, `new` — and each is a **composition**, not a
sort: named sources, a declared per-source budget, provenance, then selection. The arithmetic is
`polaris-candidates` (`compose`, `select`, `convex`), the crate kindred and darkhold already run; orbital
owns only the source lists, the shelf names and the `why` line (SPEC §1e's silence rule applies to it).

| shelf | sources | selection |
|---|---|---|
| `for_you` | viewer tag relevance (`rank_top`) + raw clicks | MMR at `for_you_lambda` (default 0.85) |
| `rising` | `rank_trending` | MMR at 1.0 — pure relevance, i.e. the ranker's own order |
| `new` | `rank_new` | MMR at 1.0 — the ranker's own order |

`lambda = 1.0` is what makes the ordered shelves **byte-identical** to the lists they replaced; a change
to those two shelves is therefore a declared change and needs its own gate. A shelf whose candidates are
all filtered out (e.g. `rising` with no visitors in seven days) renders no heading at all: an empty shelf
is not backfilled, because a zero-filled "rising" list is a recency list wearing a costume.

Every card carries a `why` string built only from facts the reader can observe: a tag the *viewer* set,
clicks, visitors this week, or age. It may never cite the instance's prior (D27) — the same silence rule
as every other surface.

## 2c. The fame headwind (visibility damping)

Attention is the score, the score buys position, position buys attention. Left alone, the loop makes the
first members permanent and the rest unfindable — the "everyone is top 0.1%" problem, which is structural
and appears on every attention-ranked catalogue. Orbital damps it **by default**, with the strength
declared as data (D34):

| lever | default | what it does |
|---|---|---|
| attention signal | `damped` | `log1p(clicks)` (exponent configurable) instead of `clicks / max` |
| exposure correction | on | divides clicks by the impressions the member received at the positions shown |
| per-session cap | on | one session counts at most once per member per day |
| `Top` half-life | 90 days | lifetime fame decays; a **monument** tab keeps the lifetime total reachable |
| slot quota | `k = 1` per shelf | a member takes at most one card per shelf; no member on two shelves at once |
| long-tail cohort | 40% of slots | reserved for members below median exposure |
| impression ceiling | rolling 24h share | a member over the cap loses position for the day, not tomorrow |
| tail rotation | last `N` slots, daily | deterministic by date, so the tail is reachable without a button |
| `exempt` list | empty | a "monument" member ranks exactly as raw |

Guardrails, in order of importance:

1. **Damped, never hidden.** Attenuation costs position, never reachability: the profile, `/search`, the
   vertical page, the sitemap and the API always include every member, and a damped score may never exceed
   its raw score. An assertion, not a convention.
2. **Measured before tunable.** Gini and top-10 impression share per surface are snapshotted weekly; a
   ranking change may not increase concentration without a recorded decision.
3. **Stated, not exposed.** One sentence on the site may say the already-visible are ranked lower. No
   per-entity damping number, badge or message: that is the D27 leak wearing a fairness costume.
4. **`raw` is always reachable.** Flipping the declaration to `raw` reproduces the undamped order
   byte-identically, which is what makes the damping falsifiable.

## 2d. The instance's taste: declared sources and the fold

The ordering has a lean — the instance's own taste — and it is *declared*, not inferred: a list of
sources, each with a weight and a set of evidence words it may contribute. The arithmetic is
`polaris-taste-source` (D31); orbital owns the declaration and the mapping from its own tables, nothing
else.

**Declared (D30):** `criticalnexus` 0.3, `steeltitan` 0.7 — named accounts, so a bootstrap cohort needs no
roles; `attuned` with an evidence floor of 20 rows and a 0.5 threshold — close readers, admitted only when
the measurement is thick enough to mean something; and `everyone` last at 0.05. Declaration order is
semantic: the crate gives a contributor the **first** source that admits them, so a named account is never
also read as "everyone". The trust ladder is declared nowhere — it is empty on this instance, and a source
that admits nobody would only add a line of noise to the report.

**What orbital counts as evidence**, and what it deliberately does not:

| table | a statement? | timestamped? | used |
|---|---|---|---|
| `tag_endorsements` | yes — a person said this tag belongs here | yes | **`Like`** |
| `tags` (a member's own tags) | yes | **no** | **no**: a row that cannot decay is a permanent claim about someone who may have changed. A migration to date them is the price of using them. |
| `source_hits` | **no** — a click is not a statement | yes | **no**: the crate's vocabulary has no word for it, on purpose |

**No evidence is an empty profile with a reason, never a row of zeros.** `orbital doctor` prints one line
per declared source with the word `absent` when it admitted nobody or contributed nothing, followed by the
crate's own explanation. The profile is folded at boot and read on demand; it is never exposed — there is no
`/api/v1/taste`, no field, and no sentence (D27). `orbital doctor` is its only home, and `docs/` is the only
place it is described.

## 2e-bis. The offline evaluation (`polaris-eval`, D37)

No weight, ceiling or shelf may move because it looks better. A change earns its place by beating a
baseline over **real clicks**, with an interval, or it does not ship.

**Cases.** One per session in `source_hits` that clicked a member card. The judgement is the click
itself — grade 2 for a member that session clicked, and nothing else, because the site records what was
clicked and not what was shown, and "they saw it and did not click" would be invented data. A case whose
pool is a vertical is judged inside that vertical; a session that browsed none is judged against the
catalogue it was given.

**Arms.** A is `rank_new` (newest first) — the simplest defensible order; B is `rank_top` with the
session's own lens, which is what the site serves. Both are scored with `ndcg_at(…, 10)`.

**The decision rule, fixed before the data:**

| | |
|---|---|
| metric | nDCG@10, graded (a clicked member = 2) |
| floor | **30 paired cases** — below it the answer is `NO VERDICT`, whatever the means say |
| interval | 95% paired bootstrap, deterministic resampling |
| verdict | `better` only when the interval excludes zero; a straddling interval is `NO VERDICT` |
| exit code | 0 only for `better`; everything else exits **1** and blocks the change |

`orbit eval` replays `eval/golden.json` (or `--file`), and `--emit N` writes a new seed list from the rows
the database actually holds. A seed that no longer matches a row is counted as **missing**, never
replaced: a golden set that silently shrinks makes a number improve while nothing does.

**What it cannot tell you.** The clicks were collected while the site was already ordering by the
candidate, so a positive verdict bounds how much *worse* the candidate is than recency on the member
somebody clicked; it is not proof of a better ranking. That caveat is stored in the golden set's `rule`
block so it travels with the numbers.

### 2c-quinquies. The impression ceiling and the daily rotation (D45)

Exposure is measured, not inferred: every render records one impression per member per hour
(`impressions(surface, user_id, bucket, count)`), with no visitor identity in the row. Two rules read it.

| field | default | what it does |
|---|---|---|
| `share` | `0.50` | a member may hold at most this share of a surface's impressions in the last 24 h |
| `min_impressions` | `8` | below this many impressions the share is not applied — a two-member page has no ratio worth enforcing |
| `tail_slots` | `2` | how many trailing slots of a shelf are refilled daily |

**The ceiling** narrows the *pool* before composition, so no ranker has to know it exists, and it is skipped
if applying it would empty the page: the rule is a way of sharing a surface, not of blanking it. Reachability
is never lost — a capped member is still on its profile, its vertical's other tabs, and the monument (D42).

**The rotation** keeps the top of a shelf and refills the last `tail_slots` from the same candidate order,
sorted by `hash64(day, member)` — FNV-1a over the day and the member's bytes, written out because
`DefaultHasher`'s seed is not guaranteed stable across releases. The same date and the same member always
give the same slot, on any host, which is what makes the page reproducible from the date alone.

The clock is a date, not a timestamp: `day_key(date)` is days since the epoch, so two hosts that disagree by
minutes still agree on the rotation.

### 2c-sexies. The "undiscovered" shelf (D46)

The fourth home shelf is the only one that exists to show a member the *other three would not*: candidates are
the members below the **sixtieth percentile of exposure** (`shelf::UNDISCOVERED_PERCENTILE`, a constant because
it belongs to one surface), ordered by **momentum**:

```
momentum = (visitors_7d + 1) / (visitors_prev_21d / 3 + 1)
```

`+1` on both sides is the whole smoothing, and it is why the number is honest: a member with two visitors and
no history scores 3.0, a member with a hundred a week for a year scores 1.0, and a member falling from three
hundred to ten scores 0.4. Nobody's click count is rewritten to produce it — `visitors_prev` is a second
measurement in the same stats fragment, over the three weeks *before* the last seven days.

Three properties that are deliberate:

* **Empty is an answer.** If no member is below the cut (an all-flat pool, a single member) the shelf is not
  pushed and the template renders nothing. A padded "undiscovered" list would be the same faces under a new
  heading.
* **The cross-shelf rule still holds** (D43): a member the page already shows is not shown again here. On a
  catalogue smaller than its own page that makes the shelf absent, which is correct — there is nothing
  undiscovered when everything is already on the page.
* **The cohort share is a page-level promise** (D43, amended here): each shelf takes a tail slot when one is
  left, but only the first shelf can be held to the share exactly, because four shelves × 40 % needs more tail
  members than a small catalogue has. The suite asserts the page-level share and the first shelf's exact
  compliance, and says so in the assertion rather than pretending every shelf can meet it.

### 2c-quater. Exposure caps (D43)

A page view has three fairness rules, all declared in `EXPOSURE_FILE`:

| field | default | what it does |
|---|---|---|
| `slots_per_member` | `1` | at most this many slots per shelf — and at any non-zero value, **a member appears on one shelf only** in a page view |
| `cohort_share` | `0.40` | at least this share of each shelf's slots belongs to members below the exposure percentile |
| `below_percentile` | `0.50` | which members count as the long tail (the median by default) |

Exposure is the same 90-day weighted attention `Top` ranks by (D42) — the rule needs no second instrument.

The caps are a **composition**: they take the order a ranker or MMR already produced and choose which slots
it may fill. They never filter the pool and never touch a score, which is why the top of a shelf is still the
ranker's judgement: the cohort quota replaces from the *end*, never the first slot.

`slots_per_member: 0` plus `cohort_share: 0` is the escape hatch — with both off the module returns the
ranker's own prefix and the page is byte-for-byte the page that shipped before the caps existed. That is
measured on the live host, not asserted: with the declaration off, `/` compared `cmp`-identical to the
capture taken before this step.

Consequences worth stating plainly:

* a pool smaller than the page shows **fewer shelves**, not repeats — on a two-member site the home page is
  one shelf with two cards, which is what a directory that size honestly has;
* the page view includes every rail on the page, so the "picks" rail obeys the same rule (and with the
  caps off, it obeys nothing, so the escape hatch is total).

### 2c-ter. Two windows of fame (D42)

Fame is one measurement read through two windows, computed in the same stats fragment so a surface cannot
read the wrong one by accident:

| surface | column | unit | lens | damping |
|---|---|---|---|---|
| `Top`, For You, embeds | `recent` | attention × 0.5^(age / 90 days) | yes | yes |
| monument tab (`?tab=monument`) | `attention` | lifetime attention, undamped | no | exempt |

Why a half-life. `Top` is a claim about who is being read *now*, and with a lifetime total the first members
hold the first rows for as long as the site exists — a permanent incumbency that no amount of new quality
can displace. Ninety days is long enough that a member's week off costs nothing and short enough that a year
of inertia costs almost everything: a stale 10,000 is worth 98 at 600 days, which is below 200 recent.

Why a monument. Amortising fame must cost a member *position*, never *reachability* — so the lifetime total
stays one tab away, in full digits (`10,000`, not `10.0k`), and D39's damping band exempts it: a monument is
the point of a monument. The tab is part of every vertical page's tab list, so it is reachable from the
ranking it amends, in one click, without an account.

What the two numbers mean. `clicks, all time` is a raw count and can only grow. The 90-day number is
*weighted* attention (D41's `1 + ln position` per session), so it can exceed the raw count when a member's
clicks were seen lower down a page — `doctor` prints both, and the labels say which is which because a
weighted count and a click count are different numbers.

### 2c-bis. What a click is worth (D41)

Attention is the score every discovery surface starts from, so it gets a declaration of its own:
`attention = damped | raw`, plus `alpha`. Default: `damped`, `alpha = 0.50`.

| input | `raw` | `damped` (default) |
|---|---|---|
| count | `clicks` — every hit | one hit per `(member, session)`: a fan looking ten times is one person |
| weight per hit | 1 | `1 + ln position` — inverse-propensity, `exam(p) = 1/(1 + ln p)`; an unrecorded position is slot 1 |
| normalisation | `c / max` | `ln(1+c) / ln(1+max)` |
| compression | none | `^ alpha` |
| a lead of `k×` becomes | `k×` | `(ln(1+k) / ln 2)^alpha` |

At the default, a 10× lead becomes **1.86×** — the number `orbital doctor` prints, because a promise stated
as a ratio can be checked and a constant cannot. `raw` is the reviewer's escape hatch: it reads the **raw
count** and never the corrected column, so it is byte-for-byte the arithmetic of every version before this
step, which the eval confirms by reproducing the previous run exactly.

Three things this deliberately does not do: it does not record impressions (only clicks carry a position, so
a member nobody was shown is invisible to the correction); it does not touch `rank_trending` (velocity is a
week of distinct visitors, already an audience) or `rank_new` (recency); and it never hides anybody —
attenuation costs position, and a damped score cannot exceed a raw one.

**The band, and why it is a band.** The coefficient is a **guardrail, not a target**: a ranking change may
not *increase* concentration on a discovery surface without a recorded decision, and the level is read
against a band rather than aimed at a bullseye.

| surface | aim | band | why |
|---|---|---|---|
| home, vertical | **~0.35** | 0.30–0.45 | concentrated enough that quality and engagement visibly move a member, damped enough that a newcomer joining a vertical still has a path to being seen |
| ring | ~0.25 | 0.20–0.30 | it exists to circulate; a ring that concentrates has stopped being a ring |
| monument (`Top`, no decay) | higher, expected | — | a monument is the point of a monument; it does not drag the default surface |

Companion number, because Gini and the top-10 share answer different halves of the question: **top-10
members should take 30–50 % of impressions** on discovery surfaces. Above 60 % is a red flag; below 25 %
means the ranking has stopped doing work. Below a Gini of 0.25 the `Top` tab has no signal left and the
leaderboard is decoration; above 0.50 it is the 1 % problem §2c exists to solve. **A stable or falling
trend is the evidence that damping works; a rising one is the investigation, whatever the level.** With
three members the maximum attainable Gini is 0.667, so today's recorded numbers describe the pool's shape,
not anybody's fame — they are not tuned against.

**The measurement.** `concentration_snapshots` holds one row per surface per ISO week — Gini and the
top-10 share of impressions, over the members that received any. Two properties are load-bearing: it is
**per surface** (home, a vertical, a profile and the ring have different pools and different lifetimes, and
one aggregate number would be an average of four questions) and it is **keyed by the week**, so a re-run
inside the same week corrects the row instead of appending a second one. `orbital snapshot` records it;
`orbital doctor` prints it; nothing serves it — a public Gini is a sentence about whose work the site
favours. The first numbers on the live site are `profile 0.39 / vertical 0.30` over 187 and 5 impressions,
which is three and two members respectively: **the metric is honest and the sample is not yet a trend**, and
the report says so by printing the counts beside the coefficient.

## 2e-ter. The silence gate (D40)

D27 says the prior is applied and never named. That promise is kept by a **program**, not by care:
`src/silence.rs` holds the vocabulary, and two tests use it.

| what counts | where it is checked |
|---|---|
| the prior's words — `gravity`, `prior`, `taste`, `attuned`, `affinity`, `taste-source`, `instance lens` — in any case, as whole words | every public surface in the suite (18 paths: pages, feeds, sitemap, the read API) and, live, `scripts/check-silence.sh` |
| a **declared source's own name**, whatever the operator called it ("the regulars" is a decision a reader can read) | the same walk, with a declaration that names one |
| a **field** that offers to report the prior | the same matcher, because a key is text |

Whole-word matching is deliberate: `priority` is not `prior`, and a gate that fires on ordinary English is a
gate somebody switches off. The one word left out on purpose is `folded` — it is UI language for a closed
panel, and it is what the gate's first live run caught in an inline script comment.

**One exemption, written into the script rather than into a list:** the estate's capabilities document says
`"taste": "off"` — the family's name for a capability this instance does not offer, generated from a spec
every site shares (D32). The check strips exactly that field and fails on any other occurrence, with
whitespace tolerance, because an exemption that matches one formatting stops being checked the day the
renderer is re-run.

**`/api/v1/taste` is the reader's, not the instance's.** It answers two fields and no more —
`tags`, the weights the reader set, and `dial`, the number they own per surface — with
`cache-control: private, no-store` (a per-reader answer must never come out of a shared cache). The handler
never reads the declaration. The path's word is the reader's taste; nothing in the body names a source.

## 2e. The reader's dial (`polaris-gravity`, D36)

The instance's taste is not the only pull. A reader may say **how much of their own lens to apply** — and
that control is theirs, visible, and named in their words.

Every surface declares **three** numbers and reads no other surface's: its `weight` (the share its
relevance-led order gives the lens), its `ordered` weight (the share its velocity-led order gives it —
trending, new, rising), and its `ceiling` (the most a reader may dial up to). They live in one JSON file
named by `GRAVITY_FILE`; absent, the built-in numbers below are in force, and every one of them equals the
share the ranker already applies, so a fresh checkout ranks exactly as yesterday.

| surface | weight | ordered | ceiling | what reads it |
|---|---|---|---|---|
| home | 0.40 | 0.30 | 0.40 | `rank_top`, the `for_you` shelf / `rank_trending`, `rank_new`, the `rising` shelf |
| vertical | 0.40 | 0.30 | 0.40 | the same rankers on `/v/{slug}` |
| entity | 0.25 | 0.25 | 0.25 | declared; no lens term on the profile sidebar yet (SPEC §1e's quarter-cap) |
| ring | 0.60 | 0.60 | 0.60 | `ring_next`'s relevance term |

A declared weight is applied **against the share in this table**, not in place of it: declaring `0.10` where
the reference is `0.40` multiplies the lens term by a quarter, and declaring `0.40` where it already was
`0.40` multiplies by exactly `1.0`. Without that normalisation the act of writing down what the code already
did would re-rank every page. `orbital doctor` prints the four resolved policies, one line each.

The ceilings are the shares the rankers **already** applied, which is why the default (a reader who never
touched the dial) is byte-identical to the ordering the site served before the dial existed: a full lens is
a multiplication by `1.0`. Three rules come from `polaris-gravity` and are not re-implemented:

1. a reader value **narrows** the ceiling and never widens it — asking for more is refused, naming the
   number the surface allows, not clamped;
2. an operator `locked` ceiling refuses the write with its own reason;
3. a stored value is **clamped on read**, so lowering a ceiling is retroactive and the page never claims
   the reader chose the clamped number.

Storage is `visitor_profiles.gravity` (`double precision`; `NULL` = no choice, `0.0` = no lens at all, and
the two are different answers). The form field is `lens`, never the estate's private word for the pull: the
markup a reader receives must not name the mechanism (D27), and the test that guards the home page fails on
that word. A dial of 0 leaves a page ordered by clicks, velocity and recency — the reader cannot tell from
it whether the instance has a taste at all.

Context vertical: the vertical page being viewed, else the profile's primary.

**Ring** — per vertical and global, ordered by taste-similarity to the hub owner
with a deterministic per-day noise injection (stable within a day, reshuffles
daily, so nobody is permanently first). Footer shows both rings:

```
← prev gaming | Gaming | next gaming →
← prev (all)   | Everyone | next (all) →
```

**Anti-gaming** — the counters are distinct session hashes, so N clicks from one
browser move the number by 1. Flags of "ranking manipulation" zero the member and
reset trust. Every counter in the response is `BIGINT` cast in SQL.

---

## 3. Surfaces

| Route | What |
|---|---|
| `/` | Verticals as cards (member count + top 3), search, and three composed shelves (D27/D43: no curated rail) |
| `/v/{slug}` | Four tabs, tag filter, embed snippet, ring footer |
| `/@handle` | Profile: links, tags, verticals, similar sidebar, ring footer, JSON-LD, report |
| `/u/{handle}` | 301 → `/@handle` |
| `/search?q=` | Members, tags, links |
| `/feed.json`, `/feed.xml`, `/v/{slug}/feed.json` | JSON Feed 1.1 + RSS, per vertical too |
| `/v/{slug}/embed?view=top|trending&count=5` | Server-rendered iframe widget with a Join CTA |
| `/l/{link_id}` | Click recorder + 302 |
| `/sitemap.xml`, `/robots.txt` | Discovery plumbing; adult entities are excluded from both |
| `/join`, `/login`, `/app/*` | Svelte dashboard (static assets embedded in the binary) |
| `/lens` | the visitor's weights as an editable table; "this browser" or "your account" |
| `/prefs/off`, `/prefs/on` | personalisation switch for one browser (cookie) |
| `/lang/{en,es}` | language switch; sets `orbital_lang`, redirects back |
| `/status` | the estate: every hostname nginx serves, with HTTP code and latency |
| `/og/{handle}.{png,svg}` | generated social card, from disk when rendered, on demand otherwise |

**Adult content is a layer, not a host (D25).** A viewer's filter has three states — safe (default,
anonymous), opted in, everything — carried by a cookie for anonymous visitors and by an account
setting for members. Safe excludes every entity whose category or reviews mark it adult, on every
surface: home, category, entity page, search, feeds, sitemap. Opted-in viewers get adult entities in
ranking and search like anything else. There is no separate hostname; `orbitalnsfw.polarisocial.xyz`
is a 301 (D9). A TL0 member is hidden from the global sitemap and feed as before.

---

## 3b. Writing without a browser (implemented 2026-10-29)

Reading is anonymous; writing belongs to a member. A long-lived token is the credential that fits a
script, and a session cookie is not:

```
POST   /api/me/tokens           session-auth → {"token": "orb_…"} shown once
GET    /api/me/tokens           id, label, created_at, last_used_at — never the token
POST   /api/me/tokens/{id}/revoke
GET    /api/v1/me/links         Authorization: Bearer orb_…   (hidden links included)
POST   /api/v1/me/links         {"url","label","vertical"} → 201, metadata fetched in background
DELETE /api/v1/me/links?url=…
```

Rules that are part of the spec, not of the implementation:

- **Only a hash is stored.** `sha256(salt || token)`; a dump cannot be replayed.
- **Ten live tokens, forty links.** Both refusals are sentences, not silence.
- **A URL is validated before it is stored** (`https` only, no credentials in the URL), and the
  same SSRF guard the link checker uses applies when the title is fetched.
- **A duplicate URL on the same profile is a refusal**, not a second row.

---

## 3c. Governance (implemented 2026-10-29)

D4 deferred the quorum "until there are enough TL3s to constitute one". That time is now, so the
deferral is retired for two decisions and kept for moderation:

| Decision | Who | Rule |
|---|---|---|
| A new vertical exists | any member proposes, TL3 votes | 3 votes accept; the deciding vote applies it in the same transaction |
| A vertical merges into another | same | members move with their tags, links re-point, the retired vertical is deleted |
| A tag is endorsed | TL2+ | toggles; 3 endorsements reach a +0.2 cap on relevance |

A merge is a rename of the room, not an eviction: nobody leaves the directory and nobody's primary
listing becomes a secondary one. The owner may accept or reject without a quorum, which is what a
directory of four actually needs; the quorum path exists so the rule is written down and tested
before it is needed.

Moderation flag resolution stays with the owner (D4), unchanged: a quorum over flags would let
three accounts remove a fourth.

---

## 3d. Language (implemented 2026-10-29)

Two languages, chosen per request: cookie `orbital_lang` → `Accept-Language` → English. The choice is
rendered from a static table keyed by string (`src/i18n.rs`), and templates ask for keys. Two
invariants are enforced by tests: every key used in `templates/` exists in the table, and no entry is
empty or accidentally identical to its English source.

Status: chrome translated (nav, footer, CTA, skip link, metadata `og:locale`), page bodies pending —
see `ROADMAP.md`. A half-translated page is acceptable only because the fallback is the English
string, never a blank.

---

## 4. Auth and trust

Sessions are server-side rows + `HttpOnly; SameSite=Lax; Secure` cookie.
Passwords are Argon2id. Registration is handle + password + primary vertical.

Trust: TL0 on signup. Nightly job promotes TL0→TL1 at ≥2 links, ≥2 days old, ≥5
unique visitors. TL2 and TL3 are granted by the instance. Multipliers above.

Bluesky claim: `/app` prints `orbital-verify:<token>`; the member pastes it into
their Bluesky description; `POST /api/me/verify-bsky` fetches the public profile
record and matches. Verified handles are shown with a check and are added to the
profile's `sameAs` (a real discovery loop: the bsky profile links here, this page
links back).

---

## 5. Data model

```
users(id uuid pk, handle citext unique, email citext null, display_name,
      bio, accent, avatar_url, bsky_did, bsky_handle, bsky_verified_at,
      verify_token, trust_level int default 0, nsfw_enabled bool default false,
      is_owner bool default false, hidden bool default false,
      password_hash, created_at, last_active_at, last_login_at)

sessions(id text pk, user_id fk cascade, created_at, expires_at)
verticals(slug citext pk, label, description, icon, nsfw bool, sort int,
          min_members int default 5, created_at)
user_verticals(user_id fk, vertical_slug fk, role text, joined_at,
               pk(user_id, vertical_slug))
links(id uuid pk, user_id fk cascade, vertical_slug fk null, url, label, icon,
      position int, click_count bigint, hidden bool, created_at)
tags(user_id fk, tag_slug citext, primary key(user_id, tag_slug))
source_hits(id bigserial pk, user_id fk, link_id uuid null, surface text,
            vertical_context citext null, position int null, session_hash text,
            created_at timestamptz)
profile_views(id bigserial pk, user_id fk, session_hash, vertical_context,
              view_date date, unique(user_id, session_hash, view_date))
flags(id uuid pk, flagger_id fk null, target_user_id fk, vertical_slug null,
      reason, status text, created_at, resolved_at, resolver_id, note)
curation(vertical_slug fk, user_id fk, position int, pk(vertical_slug,user_id)) -- unused since D44, kept until a migration can drop it
bot_posts(id bigserial, kind text, subject text, uri text, created_at)
-- 0002..0005
links(… title text, favicon text, metadata_at timestamptz)
source_hits(… referrer_host text, viewer_hash text, fingerprint text)
preference_weights(owner_kind, owner_id, tag_slug, weight real, source, updated_at)
visitor_profiles(owner_id pk, pages_seen, first_seen_at, last_seen_at)
api_tokens(id uuid pk, user_id fk, token_hash unique, label, created_at, last_used_at, revoked_at)
rank_snapshots(id bigserial, user_id fk, vertical_slug, rank, taken_at)
tag_aliases(from_tag pk, to_tag, created_at)
vertical_proposals(id uuid pk, slug, label, description, proposed_by fk, merged_into fk null,
                   status text, created_at, decided_at)
vertical_votes(proposal_id fk, user_id fk, weight, created_at, pk(proposal_id, user_id))
tag_endorsements(tag_slug, user_id fk, created_at, pk(tag_slug, user_id))
```

Indexes that exist because a query needs them: `source_hits(user_id,
created_at)`, `source_hits(user_id, session_hash, created_at)`,
`links(user_id, position)`, `tags(tag_slug)`, trgm on `users.handle`,
`users.display_name`, `tags.tag_slug` for search.

---

## 5b. Entity, review and taste tables — planned, schema v6 (D23–D26)

```
entities(id uuid pk, slug citext unique,
         kind text not null,                  -- person|site|work|place|product|other
         canonical_key text unique not null,  -- url:https://… | imdb:tt… | isbn:…
         title text, description text, image text,
         category_slug fk categories, nsfw bool default false,
         origin text not null, source text, source_id text, imported_at timestamptz,
         created_by uuid null references users(id),
         first_seen timestamptz, last_checked timestamptz, last_status int,
         fail_count int default 0, hidden bool default false, merged_into uuid null)

categories(slug citext pk, label, description, sort int, nsfw bool)
entity_tags(entity_id fk, tag_slug citext, pk(entity_id, tag_slug))
entity_edges(a_entity fk, b_entity fk, relation text,   -- same_as|related|runs|mentions
             score real, computed_at, pk(a_entity, b_entity, relation))
entity_people(entity_id fk, user_id fk, role text, claimed_at, pk(entity_id, user_id))

reviews(id uuid pk, entity_id fk, user_id fk, rating smallint, body text, language text,
        nsfw bool, ap_id text unique, visibility text default 'public',
        created_at, updated_at, deleted_at)
review_votes(review_id fk, user_id fk, helpful bool, pk(review_id, user_id))
entity_edits(id bigserial pk, entity_id fk, user_id fk, diff jsonb, note text,
             created_at, reverted_at)
entity_scores(entity_id pk fk, rating_avg real, rating_count int, ballot_count int, computed_at)
ballots(id bigserial pk, user_id fk, a_entity fk, b_entity fk, winner char(1), created_at)
import_runs(id bigserial pk, source text, started_at, finished_at, rows_seen int,
            rows_inserted int, rows_updated int, rows_failed int, note text)

links.entity_id uuid null references entities(id) on delete set null
users.password_hash text null        -- shell rows / federation-only actors (D22)
users.claimed_at timestamptz null
```

Invariants the migration must enforce, not the caller:

- `canonical_key` is normalised at write time (lowercase host, strip `www.`, trailing slash and
  fragment) and unique. It is the federation join key, so it cannot be a per-instance UUID.
- `entities.nsfw` derives from its category **and** from any review flagged `nsfw`, so no surface has
  to guess whether a page is adult.
- `reviews.ap_id` is unique, so redelivery of the same federated activity is idempotent.
- Nothing cascades from `users` to `entities`: a person leaving leaves the catalogue intact.
- `merged_into` makes `same_as` non-destructive — a merged entity redirects and its reviews move in
  the same transaction.

## 6. Verification

- Unit: slug/handle normalisation, ranking maths (including the zero case),
  cosine similarity, ring determinism per day, embed view selection, host
  scoping (the safe layer never returns an adult entity), session hash stability.
- Integration (real Postgres): register → login → create links → render profile
  → click → the click appears in analytics and moves the ranking; an anonymous request never
  receives an adult entity on any surface (D25); flag → resolve; trending empty hides the tab.
- Platform (2026-10-29): a token adds a link without a cookie and a bad token is refused; a
  category is a weight while an invented one is not; a lens stops at 24; personalisation can be
  switched off and says so; three votes create a vertical and two do not; a merge moves members,
  keeps their tags and deletes the retired vertical; an endorsement lifts a member by at most 0.2;
  Spanish renders from a cookie and from `Accept-Language`; `/status` and `/og` answer.
- Mutation-checked: each of those rules was removed once and the named test went red.
- Live on the host: every route 200 with the right body, a real click recorded
  end-to-end through nginx, sitemap/robots content, embed renders standalone,
  dashboard reachable and authenticates.

## 6b. Federation (D26)

Orbital federates as an ActivityPub **server**, not as a client of somebody else's. The estate already
runs two ActivityPub implementations in two languages, so the first rule is: **do not write a third**.

- **Stack.** Gravity's Rust/Axum layer is the reference — `crates/core/src/activitystreams.rs`,
  `webfinger.rs`, `http_signature.rs`, `discovery.rs` and `crates/db/src/services/federation.rs`,
  about 7,150 lines that already do discovery, signatures and delivery. Orbital depends on it as a
  package, or it moves into `polaris-core` once a second crate needs it (family rule of two). PieFed
  is the second data point and the second language, not the plan.
- **Actors.** Per-user actors created on the first federation act
  (`@handle@orbital.polarisocial.xyz`), plus one instance actor for the instance's own
  recommendations. WebFinger resolves handles, HTTP Signatures sign every delivery, NodeInfo describes
  the software.
- **Objects.** An entity is an AP `Object` whose `id` is ours and whose `url` is the canonical key's
  URL. A review is a custom `Review` object — `type: "Review"`, `inReplyToEntity`, `rating`,
  `content`, `attributedTo` — **and a plain `Note` carrying `schema:reviewRating` and
  `schema:itemReviewed` for peers that do not know the extension**. BookWyrm is the precedent:
  custom types between peers that share them, downgrade to `Article`/`Note` for everyone else.
- **Joining on identity.** An inbound `Create` of a review carries either our entity URI or a
  canonical-key-shaped reference (a URL, an `imdb:`/`isbn:` scheme). We resolve by key, then by
  `same_as`; a review for an unknown key **creates the entity from its own metadata**, which is how
  the catalogue grows without an importer.
- **Moderation, stated as a cost.** Inbound content from a domain never seen before is quarantined:
  visible to its author and to moderators, absent from search and from rankings, until a threshold of
  local `helpful` votes or one TL2 approval. Domain suspend, per-review report, and the family's
  "quarantine invisible to search" rule (W6) are the same shape. The operator may allowlist domains
  and never has to approve anything one by one.
- **What a user gets.** A review written here appears on their followers' timelines elsewhere; a
  review written on another instance appears on the entity page here, attributed, with a link home;
  following a reviewer brings their reviews like any other feed.
