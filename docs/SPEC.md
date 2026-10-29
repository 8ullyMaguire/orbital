# Orbital — SPEC

A themed directory of people and their links. Each member gets one profile page
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
A vertical with fewer than 5 members does not get a landing page — it appears in
the "Other" row of the main directory and its members still rank if it is
reached directly. `nsfw` is only served on the NSFW host.

### Link
`url`, `label`, `icon` (enum), `position`, optional `vertical_slug` scope.
Scope semantics:
- `NULL` → shown everywhere the member is shown.
- set → shown **only** while that member is viewed in that vertical's context.
The NSFW card is the same mechanism: NSFW-scoped links never render on the SFW
host, and the SFW page carries no indication that they exist.

### SourceHit
One row per outbound click: member, link, surface (`profile`, `vertical`,
`embed`, `direct`), the vertical context, the slot position, the session hash.
No IP, no user agent, no referrer path beyond the vertical — the analytics are
per-member and aggregate, and the raw table is pruned at 180 days.

### ProfileView
One row per profile render, deduped per (member, session, day) — the denominator
of every conversion claim the dashboard makes.

### Flag
Vertical-scoped when the flag came from a vertical page. Owner/curator resolves
with a reason. Resolving "ranking manipulation" zeroes the member's ranking
contribution and resets trust to TL0.

---

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

**Curated** — owner-pinned, manually ordered, per vertical.

**Similar profiles** (sidebar):

```
0.35 · cosine(tags_candidate, tags_viewer)
0.25 · cosine(tags_candidate, tags_owner)
0.20 · vertical_match   (1.0 same primary, 0.5 secondary, else 0)
0.10 · recency(last_active_at, 30-day half-life)
0.10 · trust_level/3
```

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
| `/` | Verticals as cards (member count + top 3), search, new/trending/curated rails |
| `/v/{slug}` | Four tabs, tag filter, embed snippet, ring footer |
| `/@handle` | Profile: links, tags, verticals, similar sidebar, ring footer, JSON-LD, report |
| `/u/{handle}` | 301 → `/@handle` |
| `/search?q=` | Members, tags, links |
| `/feed.json`, `/feed.xml`, `/v/{slug}/feed.json` | JSON Feed 1.1 + RSS, per vertical too |
| `/v/{slug}/embed?view=top|trending&count=5` | Server-rendered iframe widget with a Join CTA |
| `/l/{link_id}` | Click recorder + 302 |
| `/sitemap.xml`, `/robots.txt` | Discovery plumbing; NSFW host is `noindex` and disallowed |
| `/join`, `/login`, `/app/*` | Svelte dashboard (static assets embedded in the binary) |

NSFW host (`orbitalnsfw.polarisocial.xyz`): age gate sets a cookie on POST, then
the same surface, restricted to `nsfw` members, `noindex, nofollow`, excluded
from the global sitemap and the global feed. A TL0 member is hidden.

---

## 4. Auth and trust

Sessions are server-side rows + `HttpOnly; SameSite=Lax; Secure` cookie.
Passwords are Argon2id. Registration is handle + password + primary vertical.

Trust: TL0 on signup. Nightly job promotes TL0→TL1 at ≥2 links, ≥2 days old, ≥5
unique visitors. TL2 and TL3 are operator-granted (curator). Multipliers above.

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
curation(vertical_slug fk, user_id fk, position int, pk(vertical_slug,user_id))
bot_posts(id bigserial, kind text, subject text, uri text, created_at)
```

Indexes that exist because a query needs them: `source_hits(user_id,
created_at)`, `source_hits(user_id, session_hash, created_at)`,
`links(user_id, position)`, `tags(tag_slug)`, trgm on `users.handle`,
`users.display_name`, `tags.tag_slug` for search.

---

## 6. Verification

- Unit: slug/handle normalisation, ranking maths (including the zero case),
  cosine similarity, ring determinism per day, embed view selection, host
  scoping (SFW never returns an nsfw link), session hash stability.
- Integration (real Postgres): register → login → create links → render profile
  → click → the click appears in analytics and moves the ranking; NSFW link
  never appears on the SFW host; flag → resolve; trending empty hides the tab.
- Live on the host: every route 200 with the right body, a real click recorded
  end-to-end through nginx, sitemap/robots content, embed renders standalone,
  NSFW gate refuses without the cookie, dashboard reachable and authenticates.
