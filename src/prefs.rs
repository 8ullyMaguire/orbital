//! The viewer's preference profile: what they asked to see more of.
//!
//! Two kinds of entry share one table. **Explicit** entries come from the tuner and never decay —
//! a person who said "chiptune, 9" three months ago still means it. **Passive** entries are
//! accumulated from browsing and decay with a 30-day half-life, because a page view is a weaker
//! signal than a slider and should fade the way a memory does.
//!
//! Personalisation is not applied before the fifth page view unless the viewer has explicit
//! weights. A list that reshuffles under someone who has seen four pages looks broken, not tailored.

use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::store;
use sqlx::PgPool;

/// Pages a visitor must have seen before passive preferences are allowed to reorder anything.
pub const WARMUP_PAGES: i64 = 5;
/// How many weights one owner may hold. The rate limit bounds requests, not size: without a cap a
/// single loop fills the table with tags, and a lens of two hundred tags is not a lens.
pub const MAX_WEIGHTS: usize = 24;
/// Cookie that disables personalisation for this browser. Set and cleared from /prefs.
pub const OFF_COOKIE: &str = "orbital_off";
/// Half-life of a passive weight, in days.
pub const DECAY_HALF_LIFE_DAYS: f64 = 30.0;

pub fn visitor_hash(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    hex::encode(h.finalize())[..32].to_string()
}

/// One slider row in the tuner.
pub struct TunerRow {
    /// The key that is stored (`chiptune`, or `v:music` for a whole vertical).
    pub tag: String,
    /// What the visitor reads. A passive vertical weight is "in music", not "#v:music" — the
    /// stored key is an implementation detail and looking at it is how this shipped as a bug.
    pub label: String,
    /// 0..10, what the slider shows; the stored weight is this divided by ten.
    pub weight: i64,
}

/// What the tuner block needs to render itself.
pub struct TunerView {
    pub rows: Vec<TunerRow>,
    pub suggestions: Vec<String>,
    /// True once the viewer has weights of their own (so the block opens expanded).
    pub active: bool,
    /// True when those weights are actually reordering anything (`ViewerPrefs::applied`). A passive
    /// weight from one page view is a note to self, not a lens — claiming "ranked with your lens"
    /// after a single visit would be a lie the page tells on the site's behalf.
    pub personalized: bool,
    /// True when this browser asked not to be personalised at all.
    pub off: bool,
}

impl TunerView {
    pub fn new(viewer: &ViewerPrefs, suggestions: Vec<String>) -> Self {
        let rows: Vec<TunerRow> = viewer
            .weights
            .iter()
            .take(8)
            .map(|(tag, w)| TunerRow {
                label: match tag.strip_prefix("v:") {
                    Some(slug) => format!("in {slug}"),
                    None => format!("#{tag}"),
                },
                tag: tag.clone(),
                weight: (w * 10.0).round().clamp(0.0, 10.0) as i64,
            })
            .collect();
        let suggestions = suggestions
            .into_iter()
            .filter(|s| !viewer.weights.iter().any(|(t, _)| t == s))
            .take(24)
            .collect();
        Self {
            active: !rows.is_empty(),
            personalized: viewer.applied,
            off: false,
            rows,
            suggestions,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ViewerPrefs {
    /// Normalised 0..1 weights, strongest first. Empty means "rank honestly".
    pub weights: Vec<(String, f64)>,
    pub pages_seen: i64,
    /// False during warm-up and for a viewer who has never tuned anything.
    pub applied: bool,
}

impl ViewerPrefs {
    /// What the rankers should use. Empty when personalisation does not apply yet, so every caller
    /// can pass this without thinking about the warm-up rule.
    pub fn ranking(&self) -> &[(String, f64)] {
        if self.applied {
            &self.weights
        } else {
            &[]
        }
    }

    pub fn weight_of(&self, tag: &str) -> f64 {
        self.weights
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, w)| *w)
            .unwrap_or(0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }
}

/// Load a viewer's profile: their weights, how much they have seen, and whether it applies yet.
pub async fn load(pool: &PgPool, raw_visitor: &str) -> Result<ViewerPrefs, AppError> {
    let hash = visitor_hash(raw_visitor);
    let pages = store::visitor_pages(pool, &hash).await?;
    let weights = store::prefs(pool, "visitor", &hash).await?;
    // Passive weights are a note to self, not an instruction: only an explicit weight (or the
    // warm-up) may reorder a list. Asking "are there any weights" conflated the two.
    let explicit = store::has_explicit_prefs(pool, "visitor", &hash).await?;
    Ok(ViewerPrefs {
        applied: explicit || pages >= WARMUP_PAGES,
        pages_seen: pages,
        weights,
    })
}

/// The profile a request gets when the browser has asked not to be personalised.
pub fn off() -> ViewerPrefs {
    ViewerPrefs::default()
}

/// Same, for a signed-in member: their weights are explicit-only (they tuned, or they did not).
pub async fn load_member(pool: &PgPool, user_id: uuid::Uuid) -> Result<ViewerPrefs, AppError> {
    let id = user_id.to_string();
    let weights = store::prefs(pool, "user", &id).await?;
    let applied = store::has_explicit_prefs(pool, "user", &id).await?;
    Ok(ViewerPrefs {
        applied,
        pages_seen: 0,
        weights,
    })
}

/// Record a page view and, if any, the passive signal it carries.
///
/// `signal` is `(tag, delta)` pairs: a profile contributes its own tags, a vertical contributes a
/// pseudo-tag (`v:gaming`) so that "this person keeps reading gaming" is expressible without
/// pretending a vertical is a tag.
pub async fn visit(
    pool: &PgPool,
    raw_visitor: &str,
    signal: &[(String, f64)],
) -> Result<i64, AppError> {
    let hash = visitor_hash(raw_visitor);
    let pages = store::touch_visitor(pool, &hash).await?;
    if !signal.is_empty() {
        store::accumulate_passive(pool, &hash, signal).await?;
    }
    Ok(pages)
}

/// Store explicit weights from the tuner. Values arrive as 0..10 (a slider) and become 0..1.
pub async fn set_explicit(
    pool: &PgPool,
    owner_kind: &str,
    owner_id: &str,
    raw: &[(String, f64)],
) -> Result<usize, AppError> {
    // A `v:` key is a category, not a tag: `normalise_tag` refuses the colon (rightly), so the
    // category row on /lens was silently storing nothing. Vertical keys are validated against the
    // real taxonomy instead, which is the check that was wanted all along.
    let mut kept: Vec<(String, f64)> = Vec::new();
    for (t, w) in raw {
        let key = if let Some(slug) = t.trim().strip_prefix("v:") {
            match crate::text::normalise_slug(slug) {
                Ok(slug) if store::vertical_by_slug(pool, &slug).await?.is_some() => {
                    format!("v:{slug}")
                }
                _ => continue,
            }
        } else {
            match crate::text::normalise_tag(t) {
                Ok(tag) => tag,
                Err(_) => continue,
            }
        };
        kept.push((key, (*w / 10.0).clamp(0.0, 1.0)));
    }

    // Strongest first, then cap: the cap should drop the weakest wishes, not the last typed ones.
    kept.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    kept.truncate(MAX_WEIGHTS);
    store::set_explicit_prefs(pool, owner_kind, owner_id, &kept).await?;
    Ok(kept.len())
}

/// Effective weight of a passive entry `age_days` after it was last reinforced.
pub fn decayed(weight: f64, age_days: f64) -> f64 {
    if age_days <= 0.0 {
        return weight;
    }
    weight * 0.5f64.powf(age_days / DECAY_HALF_LIFE_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_visitor_hash_is_stable_and_hides_the_cookie() {
        let a = visitor_hash("3f9a1c");
        assert_eq!(a, visitor_hash("3f9a1c"), "the same cookie must land on the same row");
        assert_ne!(a, visitor_hash("3f9a1d"));
        assert_eq!(a.len(), 32);
        assert!(!a.contains("3f9a1c"), "the raw id must not survive in the hash");
    }

    #[test]
    fn passive_weights_decay_but_never_reach_zero() {
        assert!((decayed(1.0, 0.0) - 1.0).abs() < 1e-9);
        assert!((decayed(1.0, 30.0) - 0.5).abs() < 1e-9);
        assert!((decayed(1.0, 60.0) - 0.25).abs() < 1e-9);
        // A year later it is small, not absent: a returning reader is not a stranger.
        assert!(decayed(1.0, 365.0) > 0.0);
    }

    #[test]
    fn personalisation_waits_for_the_warm_up() {
        let cold = ViewerPrefs {
            weights: vec![("chiptune".into(), 0.9)],
            pages_seen: 2,
            applied: false,
        };
        assert!(cold.ranking().is_empty(), "two pages is not a preference");

        let tuned = ViewerPrefs {
            weights: vec![("chiptune".into(), 0.9)],
            pages_seen: 0,
            applied: true,
        };
        assert_eq!(tuned.ranking().len(), 1, "an explicit choice applies at once");
    }
}
