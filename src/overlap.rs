//! Tag overlap, with the rarity weighting the comment always claimed and the code never did.
//!
//! `ranking.rs::cosine` said "weighted so a rare tag counts for more than a tag everybody has" and then
//! counted shared tags with no weights at all. That is the kind of comment that outlives its function: the
//! ranking looked tuned and was not. This module is the honest version, and it is the only place the two
//! entry points live (`the_old_definitions_survive_only_in_the_adapter` reads `src/` and fails if a second
//! definition appears).
//!
//! The arithmetic: each tag is weighted by its inverse document frequency across the candidate pool,
//! `idf(t) = ln(1 + N / df(t))`, normalised by the largest idf in the pool so the rarest tag weighs exactly
//! `1.0`. Normalising is what keeps the change honest — on a pool where every tag is equally common every
//! weight is `1.0`, the sums are the old sums, and the result is bit-identical to the formula this replaces.
//! That is the step's gate, and `tests/overlap.rs` compares 10,000+ pairs against the old formula to prove it
//! rather than assert it.
//!
//! The seam uses the family's contract (`polaris_candidates::Signal`): absence is `None`, never `0.0` — a
//! member with no tags has nothing to say about overlap, and "we share nothing" is a different fact. The
//! rankers still read `None` as zero, because that is what they meant; the difference is that the seam can
//! now say which one it is.
//!
//! What the crate does *not* supply: the string-keyed arithmetic (its `Profile` takes `&'static str` signal
//! names, not user tags) and any tag taxonomy. So this module keeps the arithmetic and adopts the contract,
//! and `docs/SHARED-RECOMMENDATION.md` records that boundary rather than pretending otherwise.

use std::collections::HashMap;

use polaris_candidates::{Id, Signal, TagId};

/// Inverse-document-frequency weights over one candidate pool, normalised so the rarest tag weighs `1.0`.
#[derive(Debug, Clone, Default)]
pub struct Overlap {
    weight: HashMap<String, f64>,
}

impl Overlap {
    /// No pool, no rarity: every tag weighs `1.0`, which is the arithmetic this module replaced.
    pub fn uniform() -> Self {
        Self::default()
    }

    /// Build the weights from a pool's tag sets. `df` counts candidates, not occurrences: a member who lists
    /// `rust` twice still makes `rust` common once.
    pub fn from_sets<'a, I>(sets: I) -> Self
    where
        I: IntoIterator<Item = &'a [String]>,
    {
        let mut df: HashMap<&'a str, usize> = HashMap::new();
        let mut n = 0usize;
        let sets = sets.into_iter();
        for tags in sets {
            n += 1;
            let mut seen: Vec<&str> = Vec::with_capacity(tags.len());
            for t in tags {
                if !seen.contains(&t.as_str()) {
                    seen.push(t.as_str());
                }
            }
            for t in seen {
                *df.entry(t).or_insert(0) += 1;
            }
        }
        if n == 0 || df.is_empty() {
            return Self::uniform();
        }
        let idf = |d: usize| (1.0 + n as f64 / d.max(1) as f64).ln();
        let max = df.values().map(|d| idf(*d)).fold(0.0f64, f64::max);
        if max <= 0.0 {
            return Self::uniform();
        }
        Self {
            weight: df
                .into_iter()
                .map(|(t, d)| (t.to_string(), idf(d) / max))
                .collect(),
        }
    }

    /// The weight a tag carries, for callers that want to say *why* two members are neighbours: a shared tag the
    /// pool barely uses is the reason, and a shared tag everybody has is decoration.
    pub fn rarity(&self, tag: &str) -> f64 {
        self.w(tag)
    }

    fn w(&self, tag: &str) -> f64 {
        // A tag the pool has never seen is as rare as it gets; `1.0` is the normalised maximum.
        self.weight.get(tag).copied().unwrap_or(1.0)
    }

    /// Weighted cosine of two tag sets, or `None` when either side has nothing to say.
    pub fn pair(&self, a: &[String], b: &[String]) -> Option<f64> {
        if a.is_empty() || b.is_empty() {
            return None;
        }
        let mut dot = 0.0f64;
        for t in a {
            if b.contains(t) {
                let w = self.w(t);
                dot += w * w;
            }
        }
        let na: f64 = a.iter().map(|t| self.w(t).powi(2)).sum();
        let nb: f64 = b.iter().map(|t| self.w(t).powi(2)).sum();
        if na <= 0.0 || nb <= 0.0 {
            return None;
        }
        // Not clamped: the old formula was not, and a self-overlap of three tags is `1.0000000000000002` —
        // one ulp above one, which no clamp should be allowed to hide while the claim is "the same arithmetic".
        // The `[0,1]` bound belongs at the signal seam, where the estate's contract asks for it.
        Some(dot / (na.sqrt() * nb.sqrt()))
    }

    /// The same, with the rankers' reading of absence: no opinion contributes nothing.
    pub fn pair_or_zero(&self, a: &[String], b: &[String]) -> f64 {
        self.pair(a, b).unwrap_or(0.0)
    }
}

/// The seam a signal crosses: ids in, `Option<f64>` out. `idf` is indexed by `TagId`.
#[derive(Debug, Clone)]
pub struct TagSignal {
    idf: Vec<f64>,
    seed: Vec<TagId>,
}

impl TagSignal {
    pub fn new(idf: Vec<f64>, seed: Vec<TagId>) -> Self {
        Self { idf, seed }
    }
}

impl Signal for TagSignal {
    fn name(&self) -> &'static str {
        "tag-overlap"
    }

    fn score(&self, _seed: Id, _candidate: Id, tags: &[TagId]) -> Option<f64> {
        if tags.is_empty() || self.seed.is_empty() {
            // Nothing to say: the candidate has no tags, or there is no seed to compare against.
            return None;
        }
        let w = |t: TagId| self.idf.get(t as usize).copied().unwrap_or(1.0);
        let dot: f64 = self
            .seed
            .iter()
            .filter(|t| tags.contains(t))
            .map(|t| w(*t).powi(2))
            .sum();
        let na: f64 = self.seed.iter().map(|t| w(*t).powi(2)).sum();
        let nb: f64 = tags.iter().map(|t| w(*t).powi(2)).sum();
        if na <= 0.0 || nb <= 0.0 {
            return None;
        }
        Some((dot / (na.sqrt() * nb.sqrt())).clamp(0.0, 1.0))
    }
}

/// Tag-set overlap for a caller that has no pool: every tag weighs the same.
///
/// Kept under its old name because four rankers and a dozen tests call it, and a rename would be churn
/// dressed as progress. What changed is where it lives and what it delegates to.
pub fn cosine(a: &[String], b: &[String]) -> f64 {
    Overlap::uniform().pair_or_zero(a, b)
}

/// How much of what the viewer asked for this member actually is: `Σ(w[t] · has_tag[t]) / Σw[t]`, so `1.0`
/// means every tag the viewer cares about is on this profile, whatever the number of weights.
///
/// No IDF here on purpose: the weights *are* the viewer's statement of importance, and scaling them by the
/// pool's rarity would answer a different question than the one the viewer asked.
pub fn tag_relevance(tags: &[String], weights: &[(String, f64)]) -> f64 {
    let total: f64 = weights.iter().map(|(_, w)| w.max(0.0)).sum();
    if total <= 0.0 || tags.is_empty() {
        return 0.0;
    }
    let hit: f64 = weights
        .iter()
        .filter(|(t, w)| *w > 0.0 && tags.iter().any(|tag| tag == t))
        .map(|(_, w)| *w)
        .sum();
    (hit / total).clamp(0.0, 1.0)
}
