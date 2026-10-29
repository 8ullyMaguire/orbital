# SPEC v3 — Tag-weighted discovery

Supersedes the viewer half of §3 in `docs/SPEC.md` (ranking) and adds the preference profile.
Everything else in `SPEC.md` carries forward unchanged: one surface (D9), no imagery, trust levels,
`rel="nofollow ugc"`, the link checker.

The shift: discovery is driven by **what the viewer says they want**, not by what the social graph
implies. The ranking formulas are otherwise the same shapes as v2, with a tag-relevance term added
and the hub-owner's weight reduced.

## 1. Preference profile

One concept, two owners: a **member** (by user id) or an **anonymous visitor** (by visitor id,
already minted as an HttpOnly cookie `orbital_v`).

Each entry is `tag → weight` in `[0,1]`, with a `source` of `explicit` (tuner) or `passive`
(browsing). Explicit entries win over passive entries for the same tag; passive entries decay with a
30-day half-life at read time, explicit entries do not decay.

**Deviation from v3 as written, and why.** v3 stores the anonymous vector in an "encrypted cookie"
and keeps a `PreferenceWeight` table with a nullable `session_hash`. Two 0600-ish problems with
that: a cookie is a client-controlled blob (signing it correctly is work we would rather not do
twice), and it caps the vector at ~40 tags before the cookie is dropped by the browser. The vector
now lives in Postgres, keyed by the existing visitor id, hashed with SHA-256 before storage. The
cookie keeps doing only what it already did: identify a returning browser. Nothing about the
preference vector is trusted from the client, so there is nothing to encrypt.

## 2. Ranking

`tag_relevance(user_tags, viewer_weights) = Σ(w[t] · has_tag[t]) / Σ w[t]`, 0 when the viewer has no
weights. All four directory tabs take it:

| view | formula |
|---|---|
| Top | `0.6 × clicks_normalised + 0.4 × tag_relevance` |
| Trending | `0.5 × velocity_7d + 0.3 × tag_relevance + 0.1 × new_account_boost + 0.1 × trust_multiplier` |
| New | `0.7 × recency + 0.3 × tag_relevance` |
| Curated | untouched — editorial, not personalized |

Similar-profiles sidebar:

```
0.40 × tag_relevance(candidate, viewer)
+ 0.25 × cosine(candidate, page_owner)
+ 0.15 × cosine(candidate, hub_owner)      (was 0.35 in v1/v2)
+ 0.10 × vertical_match
+ 0.05 × recency
+ 0.05 × trust
```

For a viewer with no weights the 0.40 term is 0 and the multipliers are rescaled by
`1 / 0.60`, so the "no preference" ordering is the v2 ordering rather than one with its top two
terms silently deleted — otherwise everyone with no profile sorts the sidebar at 60% amplitude
against a different baseline.

Webring `next`:

```
0.6 × tag_relevance(candidate, viewer)
+ 0.3 × cosine(candidate, current)
+ 0.1 × novelty(candidate)      = 1 unless visited in the last 7 days (tracked per visitor)
```

With no viewer weights it falls back to the v2 ring (affinity to the hub owner + daily noise).

## 3. The tuner

A sidebar block on `/` and `/v/{slug}`: a `<form method=post action=/prefs>` of range inputs
(0–10) for the viewer's current top tags plus a free-text tag field, with `<noscript>` honesty —
the form works with JavaScript off because it is a form. Applying re-ranks immediately (a redirect
back to the same URL) and persists: DB for members, DB keyed by visitor id for everyone else.

Passive accumulation: every profile page view adds `+0.15` (cap 1.0) to each tag of that profile for
the viewer; every vertical page view adds `+0.05` to a vertical pseudo-tag. It starts affecting order
after the visitor has `pages_visited >= 5`, so the first four pages a person sees are the honest
default order — a ranking that shuffles under someone before they have seen it is worse than no
personalization.

## 4. Analytics

Two additions to the member dashboard, both aggregated over their own visitors:

- **How people found you** — share of clicks by `surface` (`vertical`, `similar`, `ring`, `direct`),
  recorded on the outbound click because that is the only moment we know both the member and where
  the click came from.
- **What your visitors care about** — the viewer-side preference weights of the visitors who clicked,
  rolled into `tag (strength)` where strength is the mean weight bucketed `strong ≥ 0.6`,
  `moderate ≥ 0.3`, `weak < 0.3`. Anonymized: counts only, no visitor ids leave the query.

## 5. Data model

```sql
preference_weights(owner_kind text CHECK (owner_kind IN ('user','visitor')),
                   owner_id text, tag_slug text, weight real CHECK (weight BETWEEN 0 AND 1),
                   source text CHECK (source IN ('explicit','passive')),
                   updated_at timestamptz, PRIMARY KEY (owner_kind, owner_id, tag_slug));

visitor_profiles(visitor_hash text PRIMARY KEY, pages_visited int NOT NULL DEFAULT 0,
                 first_seen_at timestamptz, last_seen_at timestamptz);

ALTER source_hits ADD surface text;   -- vertical | similar | ring | direct | profile
ALTER source_hits ADD viewer_hash text;
```

## 6. Deliberately not built in this pass

- **Negative weights.** v3's own recommendation is authenticated-only with a −0.5 floor; it is a
  brigading lever and it changes the meaning of "weight" from a lens to a vote. Revisit with member
  tokens, where an account exists to throttle.
- **`discovery_mode` enum.** Three states where one flag ("personalize: on/off") does the job; the
  enum is a UI concept, not a storage concept.
- **Tag aliasing / nightly normalisation.** Correct, and the right time is when there are enough
  members to produce variants — today the corpus is authored, not emergent.
- **`session_hash` conversion tracking.** The funnel analysis it enables needs volume to mean
  anything; `visitor_profiles.converted_at` is one column away when it does.
