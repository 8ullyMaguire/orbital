# PLAN v3 — implementing tag-weighted discovery

Every step names the file, the change, the verification command and the expected result. Steps are
ordered so the repo is green after each one.

## Step 1 — schema (migration `0003_preferences.sql`)

```sql
CREATE TABLE preference_weights (
  owner_kind text NOT NULL CHECK (owner_kind IN ('user','visitor')),
  owner_id   text NOT NULL,
  tag_slug   text NOT NULL,
  weight     real NOT NULL CHECK (weight >= 0 AND weight <= 1),
  source     text NOT NULL CHECK (source IN ('explicit','passive')),
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (owner_kind, owner_id, tag_slug)
);
CREATE INDEX preference_weights_tag ON preference_weights (tag_slug);

CREATE TABLE visitor_profiles (
  visitor_hash  text PRIMARY KEY,
  pages_visited int NOT NULL DEFAULT 0,
  first_seen_at timestamptz NOT NULL DEFAULT now(),
  last_seen_at  timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE source_hits ADD COLUMN surface text;
ALTER TABLE source_hits ADD COLUMN viewer_hash text;
CREATE INDEX source_hits_viewer ON source_hits (viewer_hash);
```

Verify: `cargo run -- migrate` against the local DB → `schema version Some(3)`;
`select count(*) from preference_weights` → 0.

## Step 2 — preference store (`src/store.rs`)

- `pub async fn prefs(pool, owner_kind, owner_id) -> Vec<(String, f64)>` — explicit entries plus
  passive entries decayed by `0.5f64.powf(days / 30.0)`, explicit winning on collision, sorted by
  weight desc.
- `pub async fn set_explicit_prefs(pool, owner_kind, owner_id, entries: &[(String, f64)])` —
  delete then insert in one transaction; `weight <= 0` deletes.
- `pub async fn accumulate_passive(pool, visitor_hash, tags: &[(String, f64)])` — upsert
  `+delta` capped at 1.0, only for tags without an explicit entry.
- `pub async fn touch_visitor(pool, visitor_hash) -> i64` — bump `pages_visited`, return the new
  count.
- `pub fn visitor_hash(raw: &str) -> String` — SHA-256 hex; `sha2` is already a dependency
  (`Cargo.toml`), otherwise `blake3`.

Verify: unit test on `visitor_hash` stability; integration test that a passive entry decays.

## Step 3 — ranking (`src/ranking.rs`)

- `pub fn tag_relevance(tags: &[String], weights: &[(String, f64)]) -> f64`
- `pub fn normalize_weights(weights) -> HashMap<String, f64>` (drop non-positive, cap 1.0)
- `rank_top/rank_trending/rank_new` gain a `&[(String, f64)]` argument and score as SPEC v3 §2;
  with empty weights they must produce **exactly** the previous ordering.
- `similarity(...)` gains `viewer_weights` and the rescaled no-preference case; `hub_owner` weight
  0.35 → 0.15.
- `ring(...)` gains `viewer_weights` and `visited: &HashSet<Uuid>`; empty weights → previous order.

Verify: `cargo test ranking` → old ordering tests still pass unchanged (that is the regression
guard), new tests:
- `tag_relevance` is 0 for an empty viewer and 1.0 for an exact match at any single weight;
- a viewer weighted to `chiptune` moves a chiptune member above a higher-click generic one in `Top`;
- empty weights reproduce `rank_top`'s click ordering exactly.

## Step 4 — viewer context (`src/lib.rs`, `src/auth.rs`)

- `visitor_middleware` already hashes/inserts `VisitorId`; extend it to load the viewer's weights
  into `ViewerPrefs { weights: Vec<(String,f64)>, pages_visited: i64, personalize: bool }` and
  record a passive visit for profile/vertical pages (skip `/api/`, `/app/`, `/static/`, `/prefs`,
  bots).
- `personalize` is false below 5 pages unless the viewer has explicit weights (the tuner is an
  opt-in) — SPEC v3 §3.
- Cache the load in the request extensions; one query per request is affordable at this size but
  appears twice on a profile page, so call it once.

Verify: `cargo test --test http the_first_four_pages_are_not_personalised`.

## Step 5 — tuner + POST /prefs (`src/routes/prefs.rs`, `templates/tuner.html`, `src/render.rs`)

- `GET /prefs` is not needed; the widget is rendered by the page renderer with the current weights.
- `POST /prefs` parses `tag=<slug>&w=<0..10>` pairs plus `tag_free` + `w_free`, normalises to 0..1,
  writes `explicit` entries, redirects to `Referer` (same-origin only, else `/`).
- `POST /prefs/reset` deletes the viewer's explicit entries.
- Widget is a `<details open>` block with `input[type=range]` per tag and a free-text row; no JS.

Verify: `curl -s -X POST -d 'tag=chiptune&w=10' -b cookie ... /prefs` → 303; then the directory
order changes for that cookie only; a browser-UA curl of `/` shows `Chiptune` above the previous #1.

## Step 6 — webring (`src/routes/public.rs`)

`vertical_next` scores candidates with `0.6 × tag_relevance + 0.3 × cosine(next, current) + 0.1 ×
novelty`, falling back to the ring order when the viewer has no weights; visited members are
recorded in `visitor_profiles` (a `ring_visits` row would be a third table for a 7-day memory — the
visitor's `preference_weights` already carry the vertical pseudo-tag, so novelty is computed from
`last_seen_at` on that row).

Verify: `cargo test --test http the_ring_follows_what_the_visitor_asked_for` (two members, viewer
weighted to the second one's tag, next → second).

## Step 7 — analytics (`src/routes/api.rs`, `src/routes/track.rs`, Svelte)

- `track` writes `surface` (from `?s=` when present, else derived from `Referer` path) and
  `viewer_hash`.
- `/api/analytics` gains `by_surface` and `audience_tags: [{tag, strength, mean}]`.
- New Svelte card in `web/src/views/Overview.svelte`: "How people found you" (bars) and
  "What your visitors care about" (chips).

Verify: click a link with `?s=ring` and see `surface='ring'` in `select surface, count(*) ...`;
dashboard renders both cards with non-zero bars.

## Step 8 — docs, deploy, live verification

- `docs/DECISIONS.md` D14 (server-side anonymous vector, why not the cookie).
- `README.md` — a "Personalisation" section; `docs/SPEC.md` links to v3.
- Deploy, then on the live host: tuner visible on `/` and `/v/dev`; a weighted cookie gets a
  different `/v/dev` order than a clean one; ring follows the weight; dashboard cards render;
  `orbital-restore-test` still passes (schema 3).

Expected: 48+ tests green, smoke `0 failure(s)`.
