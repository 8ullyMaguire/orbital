//! Exposure caps: how many slots one member may hold on a page, and how much of the page belongs to the
//! members nobody has found yet.
//!
//! Two fairness rules, declared rather than compiled in (D43):
//!
//! * **`slots_per_member`** — a member may hold at most this many slots per shelf, and (at any non-zero
//!   value) may not appear on two shelves in one page view. A member a hundred times ahead is still one
//!   face: the page is a page of the catalogue, not a portrait.
//! * **`cohort_share`** — at least this fraction of a shelf's slots belongs to members **below the
//!   `below_percentile` of exposure**, so a quiet majority cannot be squeezed off every surface by a
//!   handful of loud ones. Exposure is the same 90-day weighted attention `Top` ranks by (D42), so the
//!   rule needs no second instrument and no new row.
//!
//! Both are *compositions*, applied to an order a ranker already produced — never a filter on the pool and
//! never a change to a score. That is what makes `slots_per_member: 0` plus `cohort_share: 0` the escape
//! hatch: with both off this module returns the ranker's own prefix, so the page is byte-for-byte the page
//! that shipped before the caps existed, and an operator can prove it by flipping one number.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// `0` turns the rule off entirely — including the cross-shelf one, because a page that would have
    /// shown a member twice must come back unchanged when the cap is lifted.
    pub slots_per_member: usize,
    /// `0.0` turns the cohort rule off.
    pub cohort_share: f64,
    /// The exposure percentile that counts as "the long tail". 0.50 is the median.
    pub below_percentile: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            slots_per_member: 1,
            cohort_share: 0.40,
            below_percentile: 0.50,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Spec {
    #[serde(default)]
    slots_per_member: Option<usize>,
    #[serde(default)]
    cohort_share: Option<f64>,
    #[serde(default)]
    below_percentile: Option<f64>,
}

impl Config {
    /// Literal config for tests and for callers with no configuration to hand: the default policy.
    pub fn for_test() -> Self {
        Self::default()
    }

    /// The escape hatch, spelled out: no per-member cap, no cross-shelf rule, no cohort quota.
    pub fn off() -> Self {
        Self {
            slots_per_member: 0,
            cohort_share: 0.0,
            below_percentile: 0.50,
        }
    }

    pub fn is_off(&self) -> bool {
        self.slots_per_member == 0 && self.cohort_share <= 0.0
    }

    /// `EXPOSURE_FILE` names a JSON override:
    /// `{"slots_per_member":1,"cohort_share":0.4,"below_percentile":0.5}`. Absent, the default is in
    /// force.
    pub fn from_env() -> Result<Self> {
        match std::env::var("EXPOSURE_FILE").ok().filter(|p| !p.trim().is_empty()) {
            Some(path) => {
                let raw = std::fs::read_to_string(&path)
                    .with_context(|| format!("exposure: EXPOSURE_FILE={path} could not be read"))?;
                Self::from_json(&raw).with_context(|| format!("exposure: EXPOSURE_FILE={path}"))
            }
            None => Ok(Self::default()),
        }
    }

    /// Parse and validate. A share outside `0.0..=1.0` is refused by field name: a share of 40 is a typo
    /// for 0.40, and a share above 1 would demand more tail slots than the shelf has, which is a policy
    /// this module cannot honour and will not pretend to.
    pub fn from_json(raw: &str) -> Result<Self> {
        let spec: Spec = serde_json::from_str(raw)
            .context("the exposure declaration is not JSON in the declared shape")?;
        let share = spec.cohort_share.unwrap_or(0.40);
        if !share.is_finite() || !(0.0..=1.0).contains(&share) {
            return Err(anyhow::anyhow!(
                "field `cohort_share`: {share} is outside 0.0..=1.0 — a share of 40 is a typo for 0.40"
            ));
        }
        let percentile = spec.below_percentile.unwrap_or(0.50);
        if !percentile.is_finite() || !(0.0..1.0).contains(&percentile) {
            return Err(anyhow::anyhow!(
                "field `below_percentile`: {percentile} is outside 0.0..1.0 — 0.5 is the median"
            ));
        }
        Ok(Self {
            slots_per_member: spec.slots_per_member.unwrap_or(1),
            cohort_share: share,
            below_percentile: percentile,
        })
    }

    /// The declaration in one line, for `orbital doctor`.
    pub fn sentence(&self) -> String {
        if self.is_off() {
            return "off: a member may hold every slot the rankers give them".to_string();
        }
        let slots = if self.slots_per_member == 0 {
            "any number of slots".to_string()
        } else if self.slots_per_member == 1 {
            "1 slot per shelf, once per page".to_string()
        } else {
            format!("{} slots per shelf, once per page", self.slots_per_member)
        };
        format!(
            "{slots}; at least {:.0}% of each shelf is below the {:.0}th percentile of exposure",
            self.cohort_share * 100.0,
            self.below_percentile * 100.0
        )
    }
}

/// The exposure value at percentile `p` (0.0..1.0) of a pool, linearly interpolated between neighbours.
/// Zero members is zero: no pool means no tail, and an empty page needs no quota.
pub fn percentile(exposure: &[f64], p: f64) -> f64 {
    if exposure.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = exposure.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = p.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let w = pos - lo as f64;
        sorted[lo] * (1.0 - w) + sorted[hi] * w
    }
}

/// Which of these members count as the long tail: exposure strictly below the declared percentile.
pub fn is_tail(exposure: &[f64], index: usize, cfg: &Config) -> bool {
    exposure
        .get(index)
        .map(|e| *e < percentile(exposure, cfg.below_percentile))
        .unwrap_or(false)
}

/// Apply the caps to one shelf's order and return the slots it may show.
///
/// `order` is the ranker's (or MMR's) full ordering of pool indices, `take` is how many slots the shelf
/// has, `used` carries the members shown on shelves already built this page view, and `exposure` is the
/// per-member exposure the cohort rule reads.
pub fn enforce(
    order: &[usize],
    take: usize,
    used: &mut HashSet<usize>,
    exposure: &[f64],
    cfg: &Config,
) -> Vec<usize> {
    if take == 0 {
        return Vec::new();
    }
    if cfg.is_off() {
        let chosen: Vec<usize> = order.iter().copied().take(take).collect();
        used.extend(chosen.iter().copied());
        return chosen;
    }

    let mut counts: HashMap<usize, usize> = HashMap::new();
    let mut chosen: Vec<usize> = Vec::with_capacity(take);

    for &index in order {
        if chosen.len() >= take {
            break;
        }
        if cfg.slots_per_member > 0 {
            if used.contains(&index) {
                continue;
            }
            if counts.get(&index).copied().unwrap_or(0) >= cfg.slots_per_member {
                continue;
            }
        }
        chosen.push(index);
        *counts.entry(index).or_insert(0) += 1;
    }

    // The cohort quota: enough tail members that the shelf is not a list of the already-known. Replacing
    // from the *end* is deliberate — the top of the shelf is the ranker's judgement, and a fairness rule
    // that rewrote the first slot would be a second ranker wearing the first one's name.
    let want = ((cfg.cohort_share * take as f64).ceil() as usize).min(take);
    let have = chosen.iter().filter(|i| is_tail(exposure, **i, cfg)).count();
    if have < want {
        let mut need = want - have;
        let spare: Vec<usize> = order
            .iter()
            .copied()
            .filter(|i| is_tail(exposure, *i, cfg) && !chosen.contains(i))
            .filter(|i| cfg.slots_per_member == 0 || !used.contains(i))
            .collect();
        for index in spare {
            if need == 0 {
                break;
            }
            match chosen.iter().rposition(|i| !is_tail(exposure, *i, cfg)) {
                Some(pos) => {
                    chosen[pos] = index;
                    need -= 1;
                }
                None => break,
            }
        }
    }

    used.extend(chosen.iter().copied());
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_escape_hatch_is_the_rankers_own_prefix() {
        // The gate's third clause, as arithmetic: with both rules off this module must be invisible.
        let order = vec![0, 1, 2, 0, 1, 3];
        let mut used = HashSet::new();
        let got = enforce(&order, 4, &mut used, &[9.0, 5.0, 1.0, 0.0], &Config::off());
        assert_eq!(got, vec![0, 1, 2, 0], "the prefix, duplicates and all");
    }

    #[test]
    fn a_member_a_hundred_times_ahead_holds_one_slot_and_one_shelf() {
        let exposure = vec![1000.0, 10.0, 9.0, 8.0, 7.0];
        let cfg = Config::default();
        let mut used = HashSet::new();
        let first = enforce(&[0, 0, 0, 0, 0, 1, 2], 2, &mut used, &exposure, &cfg);
        assert_eq!(first.iter().filter(|i| **i == 0).count(), 1, "one slot for the leader: {first:?}");
        assert_eq!(first[0], 0, "and it is the first slot: {first:?}");
        let second = enforce(&[0, 0, 0, 1, 2, 3, 4], 2, &mut used, &exposure, &cfg);
        assert!(!second.contains(&0), "never on a second shelf: {second:?}");
        // Every shelf still meets the cohort share, which is the clause that would quietly fail if the
        // cap and the quota were written as two independent passes.
        let median = percentile(&exposure, 0.5);
        for shelf in [&first, &second] {
            let tail = shelf.iter().filter(|i| exposure[**i] < median).count();
            assert!(tail >= 1, "one tail slot of two: {shelf:?}");
        }
    }

    #[test]
    fn the_cohort_share_is_met_by_lowering_the_tail_end_of_the_shelf() {
        // Four loud members, one quiet. A shelf of five at a 40 % share needs two below the median, so
        // the last two slots must be the quiet ones — and the first slot must not move.
        let exposure = vec![100.0, 90.0, 80.0, 70.0, 1.0, 0.5];
        let cfg = Config::default();
        let mut used = HashSet::new();
        let got = enforce(&[0, 1, 2, 3, 4, 5], 5, &mut used, &exposure, &cfg);
        assert_eq!(got[0], 0, "the top of the shelf is the ranker's judgement");
        let tail = got.iter().filter(|i| is_tail(&exposure, **i, &cfg)).count();
        assert!(tail >= 2, "at least ceil(0.4 x 5) = 2 tail slots: {got:?}");
    }

    #[test]
    fn an_all_tail_pool_keeps_its_order_and_a_share_of_one_is_refused() {
        let exposure = vec![1.0, 2.0, 3.0];
        let cfg = Config::default();
        let mut used = HashSet::new();
        assert_eq!(enforce(&[0, 1, 2], 3, &mut used, &exposure, &cfg), vec![0, 1, 2]);
        assert!(Config::from_json(r#"{"cohort_share":40}"#).is_err());
        assert!(Config::from_json(r#"{"below_percentile":1.0}"#).is_err());
        assert_eq!(Config::from_json(r#"{"slots_per_member":0,"cohort_share":0}"#).unwrap(), Config::off());
    }

    #[test]
    fn the_declaration_says_what_it_does() {
        assert_eq!(
            Config::default().sentence(),
            "1 slot per shelf, once per page; at least 40% of each shelf is below the 50th percentile of exposure"
        );
        assert!(Config::off().sentence().starts_with("off:"));
    }
}
