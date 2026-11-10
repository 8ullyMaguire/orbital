# What orbital shares with kindred's recommender — and what it must not

Written 2026-11-10 after reading both trees, because the roadmap said "adopt `polaris-affinity`" and
**that crate does not exist**. The arithmetic orbital needs is in `polaris-candidates`, extracted from
kindred; the two others are `polaris-taste-source` and `polaris-gravity`. This file is the mapping from
orbital's surfaces onto those three, with the evidence for each claim, so the next step does not
re-derive it.

## 1. What kindred actually runs (measured, not remembered)

`~/code-local/rust/kindred`, 17,721 lines in `src/`. The recommendation path is these modules:

| module | lines | what it owns |
|---|---|---|
| `profile.rs` | 478 | `owner_taste(c) = Σ w_t·e_t / Σ e_t`, and an **evidence gate**: below ≥100 tags from ≥20 works the coefficient is 0 and the answer says why |
| `signals.rs` | 133 | `tag_overlap` with IDF, returning `Option<f64>` — `None` means "could not compute", which is not `0.0` |
| `rerank.rs` | 168 | a **declarative SPEC** (`rerank.spec.json`): fields, caps, defaults, tie-break, named weight vectors — interpreted by Rust *and* by a browser, with a parity test |
| `mmr.rs` | ~90 | relevance-vs-diversity; **now an adapter** over `polaris_candidates::select` |
| `floor.rs` | ~120 | soft floors: `score *= 1 - Σw`, capped (`MAX_FLOOR`, `MAX_FLOOR_TOTAL`), reported when a floor matches nothing |
| `tune.rs` | 390 | named weight vectors plus `w.<signal>=` overrides parsed as 400s, never silently ignored |
| `embed.rs` | — | embeddings as a **sidecar file** (`KEMBED01`, 384 f32/row, L2-normalised on load), no model linked |
| `taggraph.rs` | 590 | a precomputed tag→works / tag→neighbours graph, truncated to each tag's top 64 with the cut in the header |
| `eval.rs`, `glicko.rs`, `arena_replay.rs` | — | paired-bootstrap eval (now `polaris-eval`), ratings with uncertainty, arena replay |

Two of those numbers are why this file exists: kindred cost 26 s for `corpus.neighbours` cold and
102 s for `tags_for_works(1200)`, and both fixes (truncate the graph, sort the ids) are irrelevant at
orbital's size and would be pure complexity. **Depth is not the same as copying the hardest part.**

## 2. What is already shared

`~/code-local/rust/polaris-candidates` — "the arithmetic every recommender repeats", extracted from
kindred against `kindred/docs/DISCOVERY-CONTRACT.md`. Its own table:

| module | owns | does not own |
|---|---|---|
| `compose` | the source union: budget, priority, dedupe, cap, attribution | the queries that produce each source's list |
| `profile` | the Σw = 1 invariant | which weights a site chooses |
| `blend` | a **capped convex prior** | what the prior means |
| `mmr` | relevance-vs-diversity selection | the similarity metric (a closure) |
| `floor` | multiplicative, capped, reported rank-down | what a floor matches |
| `signal` | the `Option`-returning contract | any signal's arithmetic |

That table is orbital's home shelves, orbital's per-surface gravity, orbital's novelty slot and
orbital's "rank this away" — already written, already tested by two callers.

## 3. The three crates orbital adopts, and what it deletes

| orbital surface (roadmap step) | adopt | what goes away |
|---|---|---|
| home shelves + novelty slot (step 15) | `polaris-candidates::{compose, blend, mmr}` | the shelf loop and its ad-hoc cap in `src/routes/public.rs` |
| per-surface gravity (steps 17–19) | `polaris-candidates::blend` + `polaris-gravity` (the reader's dial) | the weight constants in `src/ranking.rs` and the "narrow only" check in `prefs.rs` |
| the prior's sources (step 17) | `polaris-taste-source` — `declaration` (validated sources), `selector`, `fold` with per-source provenance | any local notion of "trust weight" |
| cross-viewer similarity, hidden gems (steps 14, 15) | `polaris-candidates::signal` (Option contract) + `tag_overlap` from kindred's shape | `ranking.rs::cosine`, `ranking.rs::tag_relevance` |
| ranking eval (step 21) | `polaris-eval` (already the contract: paired bootstrap, decision rule, `NO VERDICT`) | the eval scaffolding I would otherwise write |
| a reader ranking a tag *away* (step 24) | `polaris-candidates::floor` | nothing yet — it is new |

`polaris-taste-source` states the three rules orbital's D30 already asked for, in the crate's own
words: "no evidence is an empty profile with a reason, never zeros", "every entry carries provenance",
"a source that admits nobody is reported, not an error". `polaris-gravity` states the dial's invariant:
"a reader sets one number; it narrows the surface's declared ceiling and never widens it; the stored
value is clamped on read".

## 4. What orbital must not take

- **`taggraph`** — it exists to avoid 26 s cold reads over 7.75M edge rows. Orbital has 24 links.
- **`embed`** — a 384-dim sidecar over a fanfic mirror. Orbital's similarity is tag-set overlap until
  the catalogue is large enough that overlap stops discriminating; that is a measurement, not a guess.
- **`glicko`** — ratings with uncertainty need a population. Two members.
- **`arena_replay`** — kindred's arena is its taste input surface; orbital's ballot (step 7) is the
  same idea at a size where a replay log is overkill. Revisit when the ballot table has rows.
- **`rerank.spec.json` as a dependency** — it is a *format*, not a crate. Orbital's `orbital.toml`
  (step 22) is the same idea per surface, and if both need the same validator, that is the moment a
  second caller exists and a `polaris-rerank` crate earns its place. Rule of two: not yet.

## 5. The adoption order, with its gate

1. **`polaris-candidates` first**, because it is the crate with two existing callers and the one
   orbital's shelves need. Build host detail, measured: the deploy compiles
   `/personal/documents/code/rust/orbital`, and the crate lives at
   `/personal/documents/code/polaris-candidates` — so the host needs the same sibling arrangement
   kindred documents ("the build host needs a sibling symlink at `../polaris-candidates`").
   *Gate:* `cargo tree -i polaris-candidates` names orbital; top-10 for 200 seeds byte-identical
   before and after; `git grep -n "fn cosine\|fn tag_relevance" src/` finds only the adapter.
2. **`polaris-taste-source`**, then **`polaris-gravity`**, each behind the same byte-identical gate over
   a golden set.
3. **`polaris-eval`** before any weight changes, so the change has a decision rule to fail.

Every step keeps D27: none of these crates' outputs may reach a public response. They decide an order;
the order is all a visitor sees.
