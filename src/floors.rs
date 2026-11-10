//! The floors layer: a tag or a category ranked *away*, by the reader or by the instance (D51).
//!
//! The arithmetic is `polaris_candidates::floor` — multiplicative, at most [`MAX_FLOOR`] per rule and
//! [`MAX_FLOOR_TOTAL`] together, so a floor can never become a filter. Orbital supplies the two things the crate
//! deliberately leaves to its caller: what a name means here (a tag, or `v:<slug>` for a category), and what an
//! unapplied floor reads like in a response.
//!
//! Nothing in here is public vocabulary: the words "floor" and "hidden" never reach a page. A floored member is
//! still in its category, still at its URL, still in the sitemap; it is only lower on a list, and the page says so in
//! the reader's own terms ("ranked away").

use serde::Deserialize;

use crate::models::MemberStat;
use polaris_candidates::floor::{self, Applied, FloorRule};

pub use polaris_candidates::floor::{MAX_FLOOR, MAX_FLOOR_TOTAL};

/// The share a bare name takes. Small on purpose: a nudge, not a ban, and low enough that two of them still leave
/// most of a member's standing.
pub const READER_FLOOR_WEIGHT: f64 = 0.25;

/// The words this layer accepts as a category, so `v:` is not a magic prefix nobody can find. Kept as a prefix
/// rather than a separate list because the store already uses it for weights.
const CATEGORY_PREFIX: &str = "v:";

/// What one declared floor did to the pool, for the sentence under the list.
#[derive(Debug, Clone, PartialEq)]
pub struct FloorNote {
    pub name: String,
    pub weight: f64,
    /// How many members carry it.
    pub matched: usize,
}

impl FloorNote {
    /// The line a page prints. Never the words "floor", "prior", "gravity": this is the reader's own preference,
    /// and a member being lower is not a fact worth a vocabulary of its own.
    pub fn sentence(&self, members: &[MemberStat]) -> String {
        if self.matched == 0 {
            return format!("\u{201c}{}\u{201d} matches nothing here — check the spelling", self.name);
        }
        let shown: Vec<String> = members
            .iter()
            .filter(|m| carries(m, &self.name))
            .take(3)
            .map(|m| format!("@{}", m.handle))
            .collect();
        let profiles = if self.matched == 1 { "profile" } else { "profiles" };
        format!(
            "ranked away: {} {} lower ({})",
            self.matched,
            profiles,
            shown.join(", ")
        )
    }
}

/// Does this member carry the name? A tag compares case-insensitively; a `v:<slug>` compares the vertical.
fn carries(m: &MemberStat, name: &str) -> bool {
    if let Some(slug) = name.strip_prefix(CATEGORY_PREFIX) {
        return m.verticals.iter().any(|v| v.eq_ignore_ascii_case(slug));
    }
    m.tags.iter().any(|t| t.eq_ignore_ascii_case(name))
}

/// The declared floors, instance and reader together.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Floors {
    rules: Vec<FloorRule>,
}

/// One rule as it arrives in a declaration file.
#[derive(Debug, Deserialize)]
struct Declared {
    name: String,
    #[serde(default = "default_weight")]
    weight: f64,
}

fn default_weight() -> f64 {
    READER_FLOOR_WEIGHT
}

#[derive(Debug, Deserialize)]
struct File {
    floors: Vec<Declared>,
}

impl Floors {
    pub fn none() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn new(rules: Vec<FloorRule>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[FloorRule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// `rust,crypto:0.4,v:music` — the reader's `?not=` and the declaration file share one grammar. Every rule is
    /// validated with the crate's `checked`, so an out-of-range weight is refused by name rather than clamped.
    pub fn parse_list(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("an empty list is not a declaration".to_string());
        }
        let mut rules = Vec::new();
        for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            // `v:music:0.1` — the weight is the last colon-separated field when it parses as a number.
            let (name, weight) = match part.rsplit_once(':') {
                Some((head, tail)) => match tail.parse::<f64>() {
                    Ok(w) => (head.trim().to_string(), w),
                    Err(_) => (part.to_string(), READER_FLOOR_WEIGHT),
                },
                None => (part.to_string(), READER_FLOOR_WEIGHT),
            };
            if name.is_empty() || name == CATEGORY_PREFIX {
                return Err(format!("{part:?} names nothing"));
            }
            if name.contains(':') && !name.starts_with(CATEGORY_PREFIX) {
                return Err(format!(
                    "{part:?} is not a weight — a name may not contain ':' (a category is written v:<slug>)"
                ));
            }
            rules.push(FloorRule::checked(name, weight)?);
        }
        if rules.is_empty() {
            return Err("an empty list is not a declaration".to_string());
        }
        Ok(Self { rules })
    }

    /// The instance's floors, from `FLOORS_FILE`. Absent file, absent floors — the common case, and not an error.
    pub fn from_env() -> Result<Self, String> {
        let Some(path) = std::env::var("FLOORS_FILE").ok().filter(|p| !p.trim().is_empty()) else {
            return Ok(Self::none());
        };
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("FLOORS_FILE {path}: {e}"))?;
        let file: File = serde_json::from_str(&raw)
            .map_err(|e| format!("FLOORS_FILE {path} is not a floors declaration: {e}"))?;
        let mut rules = Vec::new();
        for d in file.floors {
            if d.name.trim().is_empty() {
                return Err(format!("FLOORS_FILE {path}: a floor names nothing"));
            }
            rules.push(FloorRule::checked(d.name, d.weight)?);
        }
        Ok(Self { rules })
    }

    /// Instance floors first, then the reader's: a reader may add to what the instance declared, and neither can
    /// exceed the crate's caps no matter how many rules arrive together.
    pub fn merged(&self, reader: &Self) -> Self {
        let mut rules = self.rules.clone();
        rules.extend(reader.rules.iter().cloned());
        Self { rules }
    }

    /// The multiplicative factor for one member, from the crate.
    pub fn apply(&self, m: &MemberStat, score: f64) -> (f64, Applied) {
        floor::apply(&self.rules, |name| carries(m, name), score)
    }

    /// One note per declared rule, including the ones that matched nothing.
    pub fn report(&self, members: &[MemberStat]) -> Vec<FloorNote> {
        self.rules
            .iter()
            .map(|r| FloorNote {
                name: r.name.clone(),
                weight: r.weight,
                matched: members.iter().filter(|m| carries(m, &r.name)).count(),
            })
            .collect()
    }
}
