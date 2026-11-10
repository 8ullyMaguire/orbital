//! The reader's gravity dial, as orbital honours it.
//!
//! The arithmetic is `polaris-gravity` (D31: adopt it, delete our copy). One number per reader; it
//! **narrows** a surface's declared ceiling and never widens it; the operator's lock outranks the
//! reader; and the stored value is clamped **on read**, so lowering a ceiling is retroactive.
//!
//! Two nouns sit next to each other here and must not be confused:
//!
//! * the **dial** is the reader's own control — visible, settable, about *them* (`/prefs`);
//! * the **prior** is the instance's (SPEC §2d) — applied and never named (D27).
//!
//! The dial scales how much of the reader's *own* lens is applied. It never reads, scales or
//! discloses the instance's sources, and a reader turning it to zero must not be able to tell
//! whether the instance has a taste at all — that is the invariant this module could break.
//!
//! A surface's ceiling is the **share of a score** its lens term may take at most. The defaults are
//! the constants the rankers already apply, so the default (no stored value) is byte-identical to
//! the ordering the site shipped yesterday; roadmap step 4 replaces these numbers per surface.

use anyhow::{anyhow, Context, Result};
use polaris_gravity::{effective, narrow, Ceiling, DialRefusal};
use serde::Deserialize;

/// The four ranking surfaces SPEC §1e names. A surface is a place that orders things for a reader,
/// not a route: `/` and `/v/{slug}` order members, `/e/{slug}` orders links, the ring orders the
/// next place to go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Home,
    Vertical,
    Entity,
    Ring,
}

impl Surface {
    pub const ALL: [Surface; 4] = [
        Surface::Home,
        Surface::Vertical,
        Surface::Entity,
        Surface::Ring,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Surface::Home => "home",
            Surface::Vertical => "vertical",
            Surface::Entity => "entity",
            Surface::Ring => "ring",
        }
    }

    fn key(self) -> usize {
        match self {
            Surface::Home => 0,
            Surface::Vertical => 1,
            Surface::Entity => 2,
            Surface::Ring => 3,
        }
    }

    pub fn parse(name: &str) -> Option<Surface> {
        Surface::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// One surface's declared policy: the **weight** its relevance-led order gives the lens, the **ordered**
/// weight its velocity-led order gives it (smaller, because the point of those shelves is movement), and
/// the **ceiling** a reader may dial up to. A weight is the operator's own share; a ceiling is the most a
/// reader may ask for. They are different nouns and the file keeps them apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Policy {
    pub weight: f64,
    pub ordered: f64,
    pub ceiling: Ceiling,
}

/// The dial in force for one request. Two numbers because a surface ranks twice: `relevance` for
/// `rank_top` and the `for_you` shelf, `ordered` for `rank_trending`/`rank_new`/`rising`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lens {
    pub relevance: f64,
    pub ordered: f64,
}

impl Lens {
    /// Everything: what a caller with no reader and no dial means (`x * 1.0 == x` exactly).
    pub const fn full() -> Self {
        Self {
            relevance: 1.0,
            ordered: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    ceilings: [Ceiling; 4],
    weights: [f64; 4],
    ordered: [f64; 4],
}

/// The shares the rankers apply in code, per surface. A declared weight is applied **relative to this**,
/// so a declaration that equals the reference is a multiplication by `1.0` and changes no ordering. Without
/// that normalisation every existing page would be re-ranked by the act of declaring what it already did.
fn reference(surface: Surface) -> (f64, f64) {
    match surface {
        Surface::Home | Surface::Vertical => (0.40, 0.30),
        Surface::Entity => (0.25, 0.25),
        Surface::Ring => (0.60, 0.60),
    }
}

impl Default for Config {
    fn default() -> Self {
        // The shares the rankers apply today: `rank_top` 0.4, `rank_trending`/`rank_new` 0.3,
        // `ring_next` 0.6. `entity` is 0.25 because SPEC §1e caps the taste term at a quarter of any
        // score and the profile sidebar applies no lens yet — a ceiling is a cap, not a claim.
        Self {
            ceilings: [
                Ceiling::open(0.40),
                Ceiling::open(0.40),
                Ceiling::open(0.25),
                Ceiling::open(0.60),
            ],
            weights: [0.40, 0.40, 0.25, 0.60],
            ordered: [0.30, 0.30, 0.25, 0.60],
        }
    }
}

impl Config {
    /// `GRAVITY_FILE` names a JSON override: `{"home": {"strength": 0.25, "locked": false}, ...}`.
    /// Absent, the built-in ceilings above are in force, so a fresh checkout boots.
    pub fn from_env() -> Result<Self> {
        match std::env::var("GRAVITY_FILE").ok().filter(|p| !p.trim().is_empty()) {
            Some(path) => {
                let raw = std::fs::read_to_string(&path).with_context(|| {
                    format!("gravity dial: GRAVITY_FILE={path} could not be read")
                })?;
                Self::from_json(&raw).with_context(|| format!("gravity dial: GRAVITY_FILE={path}"))
            }
            None => Ok(Self::default()),
        }
    }

    /// Literal config for tests: the built-in ceilings, never the environment.
    pub fn for_test() -> Self {
        Self::default()
    }

    /// Parse and validate per-surface ceilings. A strength outside `0.0..=1.0` is refused with the
    /// surface named: a share of 40 is a typo, and clamping it to 1.0 would hide the typo forever.
    pub fn from_json(raw: &str) -> Result<Self> {
        let spec: Spec = serde_json::from_str(raw)
            .context("the dial declaration is not JSON in the declared shape")?;
        let mut config = Self::default();
        for entry in spec.surfaces {
            let Some(surface) = Surface::parse(&entry.name) else {
                return Err(anyhow!(
                    "field `surfaces[].name`: `{}` is not one of home, vertical, entity, ring",
                    entry.name
                ));
            };
            if !(0.0..=1.0).contains(&entry.strength) || !entry.strength.is_finite() {
                return Err(anyhow!(
                    "field `strength` of surface `{}`: {} is outside 0.0..=1.0",
                    entry.name,
                    entry.strength
                ));
            }
            config.ceilings[surface.key()] = Ceiling {
                strength: entry.strength,
                locked: entry.locked,
            };
            if let Some(weight) = entry.weight {
                if !(0.0..=1.0).contains(&weight) || !weight.is_finite() {
                    return Err(anyhow!(
                        "field `weight` of surface `{}`: {weight} is outside 0.0..=1.0",
                        entry.name
                    ));
                }
                config.weights[surface.key()] = weight;
            }
            if let Some(ordered) = entry.ordered {
                if !(0.0..=1.0).contains(&ordered) || !ordered.is_finite() {
                    return Err(anyhow!(
                        "field `ordered` of surface `{}`: {ordered} is outside 0.0..=1.0",
                        entry.name
                    ));
                }
                config.ordered[surface.key()] = ordered;
            }
        }
        Ok(config)
    }

    pub fn ceiling(&self, surface: Surface) -> Ceiling {
        self.ceilings[surface.key()]
    }

    /// The share of a score this reader gets on this surface: their stored choice, clamped to the
    /// declared ceiling, or the ceiling itself when they have not chosen. This is the value the gate
    /// names, and the one an operator can reason about.
    pub fn share(&self, surface: Surface, stored: Option<f64>) -> f64 {
        effective(self.ceiling(surface).strength, stored)
    }

    /// The declared policy for one surface: its weights and its ceiling, and nothing of any other's.
    pub fn policy(&self, surface: Surface) -> Policy {
        Policy {
            weight: self.weights[surface.key()],
            ordered: self.ordered[surface.key()],
            ceiling: self.ceilings[surface.key()],
        }
    }

    /// [`share`] as a fraction of the ceiling, which is what a ranker multiplies its own constant by:
    /// `1.0` means "apply the score share the code already applies", `0.0` means "no lens at all".
    /// Normalising here is what keeps the default byte-identical to yesterday's ordering.
    pub fn factor(&self, surface: Surface, stored: Option<f64>) -> f64 {
        self.factor_at(surface, stored, true)
    }

    /// The same, for a velocity-led order (`rank_trending`, `rank_new`, the `rising` shelf), which declares
    /// its own weight: an ordered shelf leans on movement and only lightly on the lens.
    pub fn factor_ordered(&self, surface: Surface, stored: Option<f64>) -> f64 {
        self.factor_at(surface, stored, false)
    }

    fn factor_at(&self, surface: Surface, stored: Option<f64>, relevance: bool) -> f64 {
        let ceiling = self.ceiling(surface).strength;
        if ceiling <= 0.0 {
            return 0.0;
        }
        let (reference, declared) = {
            let (r_relevance, r_ordered) = reference(surface);
            let policy = self.policy(surface);
            if relevance {
                (r_relevance, policy.weight)
            } else {
                (r_ordered, policy.ordered)
            }
        };
        if reference <= 0.0 {
            return 0.0;
        }
        // The reader's share of the ceiling, mapped onto the weight this surface declares. At the
        // defaults `weight == reference`, so this is exactly `share / ceiling` and the ordering is
        // byte-identical to the one the site served before the declaration existed.
        (self.share(surface, stored) / ceiling).clamp(0.0, 1.0) * (declared / reference)
    }

    /// What to store for a reader's request, or why it is refused. Refused, never clamped: a reader
    /// who asked for 1.5 meant something, and serving 1.0 answers a question they did not ask.
    pub fn set(&self, surface: Surface, requested: f64) -> Result<f64, DialRefusal> {
        let ceiling = self.ceiling(surface);
        narrow(ceiling.strength, requested, ceiling.locked)
    }
}

/// One reader's current dial, as the ranking code reads it. `None` when the database cannot be
/// asked: a broken dial read must not take the page down, and the ceiling is the safe answer.
pub async fn stored(pool: &sqlx::PgPool, visitor: &str) -> Option<f64> {
    match crate::store::gravity_of(pool, visitor).await {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(error = %e, "gravity dial: falling back to the surface ceiling");
            None
        }
    }
}

/// What a reader is told when their number is refused. It names the ceiling, never the prior: the
/// dial is about the reader's own lens, and a refusal is the only place the site mentions a limit.
pub fn refusal_sentence(refusal: DialRefusal) -> String {
    match refusal {
        DialRefusal::Locked => "This list's personalisation is fixed by the operator.".to_string(),
        DialRefusal::OutOfRange => "That is not a number between 0 and 100.".to_string(),
        DialRefusal::AboveInstance(limit) => {
            format!("This list personalises at most {}%.", (limit * 100.0).round() as i64)
        }
    }
}

/// The dial in force for one request, as a factor a ranker multiplies its own constant by.
pub async fn lens(st: &crate::AppState, visitor: &str, surface: Surface) -> Lens {
    let hash = crate::prefs::visitor_hash(visitor);
    let stored = stored(&st.pool, &hash).await;
    Lens {
        relevance: st.cfg.dial.factor(surface, stored),
        ordered: st.cfg.dial.factor_ordered(surface, stored),
    }
}

#[derive(Deserialize)]
struct Spec {
    #[serde(default)]
    surfaces: Vec<SpecSurface>,
}

#[derive(Deserialize)]
struct SpecSurface {
    name: String,
    strength: f64,
    #[serde(default)]
    locked: bool,
    /// The share this surface's relevance-led order gives the lens. Optional: absent means the
    /// reference the rankers already apply.
    #[serde(default)]
    weight: Option<f64>,
    /// The share its velocity-led order gives it (trending, new, rising).
    #[serde(default)]
    ordered: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reader_may_narrow_the_ceiling_and_never_widen_it() {
        let cfg = Config::default();
        let home = cfg.ceiling(Surface::Home);
        assert_eq!(home.strength, 0.40, "the ceiling is the share the code applies today");
        assert_eq!(cfg.set(Surface::Home, 0.0), Ok(0.0), "a reader may switch the lens off");
        assert_eq!(cfg.set(Surface::Home, 0.10), Ok(0.10));
        assert_eq!(cfg.set(Surface::Home, 0.40), Ok(0.40), "asking for what they already get is allowed");
        assert_eq!(
            cfg.set(Surface::Home, 0.60),
            Err(DialRefusal::AboveInstance(0.40)),
            "a reader may not choose a stronger hand than the surface declares"
        );
        assert_eq!(cfg.set(Surface::Home, 1.5), Err(DialRefusal::OutOfRange));
    }

    #[test]
    fn a_lowered_ceiling_clamps_a_value_that_was_legal_when_it_was_written() {
        let before = Config::default();
        assert_eq!(before.share(Surface::Home, Some(0.40)), 0.40);

        // The operator lowers the home ceiling. The same stored row now reads as the new number —
        // not as 0.40, and not as a 0.10 the reader never chose.
        let after = Config::from_json(r#"{"surfaces":[{"name":"home","strength":0.10}]}"#).unwrap();
        assert_eq!(after.share(Surface::Home, Some(0.40)), 0.10);
        assert_eq!(after.factor(Surface::Home, Some(0.40)), 1.0);
        assert_eq!(after.factor(Surface::Home, Some(0.0)), 0.0);
        assert_eq!(
            after.share(Surface::Home, Some(0.0)),
            0.0,
            "an explicit zero stays zero, whatever the ceiling"
        );
    }

    #[test]
    fn a_locked_surface_ignores_the_reader_and_says_which_refusal_it_is() {
        let cfg = Config::from_json(r#"{"surfaces":[{"name":"ring","strength":0.60,"locked":true}]}"#)
            .unwrap();
        assert_eq!(cfg.set(Surface::Ring, 0.30), Err(DialRefusal::Locked));
        assert_eq!(cfg.set(Surface::Ring, 5.0), Err(DialRefusal::Locked), "the lock is checked first");
        assert_eq!(cfg.share(Surface::Ring, Some(0.30)), 0.30, "a lock stops writes, not a value already stored");
    }

    #[test]
    fn an_unknown_surface_or_a_share_of_forty_is_refused_naming_the_field() {
        let err = Config::from_json(r#"{"surfaces":[{"name":"sidebar","strength":0.2}]}"#).unwrap_err();
        assert!(format!("{err:#}").contains("sidebar"), "{err:#}");

        let err = Config::from_json(r#"{"surfaces":[{"name":"home","strength":40}]}"#).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("strength") && msg.contains("home"), "{msg}");
    }

    #[test]
    fn the_default_ceiling_is_the_constant_each_ranker_already_applies() {
        let cfg = Config::default();
        // If these drift from the rankers, the default stops being byte-identical to yesterday and
        // the dial starts moving an ordering nobody re-measured.
        assert_eq!(cfg.share(Surface::Home, None), 0.40);
        assert_eq!(cfg.share(Surface::Vertical, None), 0.40);
        assert_eq!(cfg.share(Surface::Ring, None), 0.60);
        assert!(cfg.share(Surface::Entity, None) <= 0.25, "SPEC §1e caps the taste term at a quarter");
    }
}
