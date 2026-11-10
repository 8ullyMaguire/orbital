//! The impression ceiling and the daily tail rotation (D45).
//!
//! Two different fairness rules, both about the *page* rather than the score:
//!
//! * **A ceiling.** A member may not take more than its declared share of a surface's impressions in the
//!   last 24 hours, once the surface has shown enough cards for a share to mean anything. Fame that keeps
//!   compounding into exposures is a loop; this is where the loop is cut, on the exposure side rather than
//!   the score side, so no member's number is ever rewritten.
//! * **A rotation.** The last `tail_slots` slots of a shelf change daily, chosen by a hash of (date, member).
//!   The top of the shelf is the ranker's judgement and does not move; the tail is where a catalogue can
//!   afford to be arbitrary, and being arbitrary *deterministically* is what makes it testable and
//!   reproducible from the date alone.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

/// A day as an integer since the Unix epoch: the rotation's only clock, so two hosts on the same date agree.
pub fn day_key(day: NaiveDate) -> i64 {
    day.signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1).expect("a real epoch")).num_days()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// The most of a surface's 24-hour impressions one member may hold.
    pub share: f64,
    /// Below this many impressions on the surface, the share is not applied: a two-member page has no
    /// ratios worth enforcing, and capping it would empty the page rather than share it.
    pub min_impressions: i64,
    /// How many trailing slots of a shelf rotate daily.
    pub tail_slots: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            share: 0.50,
            min_impressions: 8,
            tail_slots: 2,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Spec {
    #[serde(default)]
    share: Option<f64>,
    #[serde(default)]
    min_impressions: Option<i64>,
    #[serde(default)]
    tail_slots: Option<usize>,
}

impl Config {
    pub fn for_test() -> Self {
        Self::default()
    }

    /// No ceiling, no rotation: the composition that shipped before D45.
    pub fn off() -> Self {
        Self {
            share: 1.0,
            min_impressions: i64::MAX,
            tail_slots: 0,
        }
    }

    pub fn is_off(&self) -> bool {
        self.tail_slots == 0 && self.min_impressions == i64::MAX
    }

    pub fn from_env() -> Result<Self> {
        match std::env::var("CEILING_FILE").ok().filter(|p| !p.trim().is_empty()) {
            Some(path) => {
                let raw = std::fs::read_to_string(&path)
                    .with_context(|| format!("ceiling: CEILING_FILE={path} could not be read"))?;
                Self::from_json(&raw).with_context(|| format!("ceiling: CEILING_FILE={path}"))
            }
            None => Ok(Self::default()),
        }
    }

    /// `{"share":0.5,"min_impressions":8,"tail_slots":2}`. A share outside `0.0..=1.0` is refused by name:
    /// a share of 50 is a typo for 0.50, and 0 would cap every member the moment the floor is reached.
    pub fn from_json(raw: &str) -> Result<Self> {
        let spec: Spec =
            serde_json::from_str(raw).context("the ceiling declaration is not JSON in the declared shape")?;
        let share = spec.share.unwrap_or(0.50);
        if !share.is_finite() || !(0.0..=1.0).contains(&share) || share == 0.0 {
            return Err(anyhow::anyhow!(
                "field `share`: {share} is outside (0.0, 1.0] — a share of 50 is a typo for 0.50"
            ));
        }
        Ok(Self {
            share,
            min_impressions: spec.min_impressions.unwrap_or(8).max(0),
            tail_slots: spec.tail_slots.unwrap_or(2),
        })
    }

    pub fn sentence(&self) -> String {
        if self.is_off() {
            return "off: no ceiling and no rotation".to_string();
        }
        format!(
            "a member may hold at most {:.0}% of a surface's 24-hour impressions (after {}), and the last {} \
             slot(s) rotate daily",
            self.share * 100.0,
            self.min_impressions,
            self.tail_slots
        )
    }
}

/// Is this member over the ceiling on a surface with `total` impressions in the window, `mine` of them its
/// own? Strictly over: a member exactly at the share is inside it.
pub fn over_ceiling(mine: i64, total: i64, cfg: &Config) -> bool {
    if cfg.is_off() || total < cfg.min_impressions || total <= 0 {
        return false;
    }
    (mine as f64) / (total as f64) > cfg.share
}

/// Deterministic hash of (day, member), FNV-1a over the day and the member's bytes. Written out rather than
/// taken from `std` because `DefaultHasher`'s seed is not guaranteed stable across releases, and a rotation
/// that changes when the compiler changes is not reproducible from the date alone.
pub fn hash64(day: i64, id: &uuid::Uuid) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in day.to_le_bytes().iter().chain(id.as_bytes().iter()) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The rotation: keep `top` in its own order, then fill `tail_slots` from `pool` — the shelf's wider
/// candidate list — ordered by the daily hash. A member already in the top cannot appear again.
pub fn rotate(
    top: &[usize],
    pool: &[usize],
    ids: &[uuid::Uuid],
    day: i64,
    cfg: &Config,
) -> Vec<usize> {
    if cfg.tail_slots == 0 || top.is_empty() {
        return top.to_vec();
    }
    let keep = top.len().saturating_sub(cfg.tail_slots);
    let mut out: Vec<usize> = top[..keep].to_vec();

    let mut tail: Vec<usize> = pool
        .iter()
        .copied()
        .filter(|i| !out.contains(i))
        .collect();
    tail.sort_by_key(|i| ids.get(*i).map(|id| hash64(day, id)).unwrap_or(u64::MAX));
    for index in tail {
        if out.len() >= top.len() {
            break;
        }
        out.push(index);
    }
    // If the pool could not refill the shelf (a tiny pool), the original tail stands rather than a short
    // shelf: rotation may reorder the tail, never shorten the page.
    if out.len() < top.len() {
        return top.to_vec();
    }
    out
}

/// The date the rotation is on, from a clock. One place, so a test can pass a fixed instant.
pub fn day_of(now: DateTime<Utc>) -> NaiveDate {
    now.date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ceiling_is_a_share_after_a_floor() {
        let cfg = Config::default();
        assert!(!over_ceiling(5, 5, &cfg), "under the floor of 8 impressions, shares are not enforced");
        assert!(!over_ceiling(4, 8, &cfg), "exactly half is inside a 50% ceiling");
        assert!(over_ceiling(5, 8, &cfg), "five of eight is over");
        assert!(!over_ceiling(80, 100, &Config { share: 0.9, ..cfg }), "a higher share lets it through");
        assert!(!over_ceiling(i64::MAX / 2, i64::MAX / 2, &Config::off()), "off means off");
    }

    #[test]
    fn the_declaration_refuses_a_share_that_is_a_percentage() {
        assert!(Config::from_json(r#"{"share":50}"#).is_err());
        assert!(Config::from_json(r#"{"share":0}"#).is_err());
        assert_eq!(Config::from_json(r#"{"share":0.4}"#).unwrap().share, 0.4);
        assert!(Config::off().sentence().starts_with("off:"));
    }
}
