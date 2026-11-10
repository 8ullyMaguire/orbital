//! Attention: what a click is worth.
//!
//! A raw click count makes three mistakes at once, and the fame headwind (D34) is built on the numbers
//! this module produces:
//!
//! * **A fan is not an audience.** Ten clicks from one session is one person looking twice. The corrected
//!   count is capped per `(member, session)`, so a single visitor cannot manufacture fame.
//! * **Slot 1 wins by being in slot 1.** A click on a card at position 9 is more informative than one at
//!   position 1, because the top of the page was examined by everybody. The correction is inverse-propensity
//!   weighting with the declared propensity `exam(p) = 1 / (1 + ln p)`, so a click's weight is `1 + ln p`.
//! * **Differences stop mattering at the top.** Between 5,000 and 5,010 clicks nobody can tell the members
//!   apart; the log normalisation is what makes the ordering describe a leader rather than round the
//!   leader's noise.
//!
//! Two declared modes, and the default is the second:
//!
//! | mode | share of the pool maximum a member gets | a lead of `k×` becomes |
//! |---|---|---|
//! | `raw` | `c / max` — exactly the arithmetic the site shipped before this module | `k×` |
//! | `damped` | `(ln(1+c) / ln(1+max))^alpha` | `(ln(1+k) / ln 2)^alpha` |
//!
//! `alpha` (default 0.5) is the compression: at 0.5 a 10× lead becomes
//! `(ln 11 / ln 2)^0.5 = 1.86×`, and at 1.0 the log alone does the work (`3.46×`). `raw` is the escape
//! hatch a reviewer uses to check that nothing else moved: it must reproduce the old ordering exactly.

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// `c / max`. The arithmetic of every version before this one.
    Raw,
    /// Log-normalised and compressed. The default.
    Damped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub mode: Mode,
    pub alpha: f64,
    /// Members ranked exactly as `raw`, whatever the declaration says (D47). Empty by default and empty in
    /// production: the list exists so an operator *can* say "this one is measured, not compressed", and the
    /// gate proves that with it non-empty the member's attention is the raw share again.
    pub exempt: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Damped,
            alpha: 0.5,
            exempt: Vec::new(),
        }
    }
}

impl Config {
    /// Literal config for tests and for callers with no configuration to hand: the default policy.
    pub fn for_test() -> Self {
        Self::default()
    }

    /// The escape hatch, spelled out: the arithmetic of the previous version, for an A/B or a postmortem.
    pub fn raw() -> Self {
        Self {
            mode: Mode::Raw,
            alpha: 1.0,
            exempt: Vec::new(),
        }
    }

    /// `ATTENTION_FILE` names a JSON override: `{"attention":"raw","alpha":0.5}`. Absent, the default
    /// policy is in force.
    pub fn from_env() -> Result<Self> {
        match std::env::var("ATTENTION_FILE").ok().filter(|p| !p.trim().is_empty()) {
            Some(path) => {
                let raw = std::fs::read_to_string(&path).with_context(|| {
                    format!("attention: ATTENTION_FILE={path} could not be read")
                })?;
                Self::from_json(&raw).with_context(|| format!("attention: ATTENTION_FILE={path}"))
            }
            None => Ok(Self::default()),
        }
    }

    /// Parse and validate. `alpha` outside `(0.0, 1.0]` is refused by field name: an alpha of 0 flattens
    /// every member to the same score, and one above 1 sharpens the lead instead of damping it — both are
    /// typos or policy, and neither belongs in a file that also accepts `damped`.
    pub fn from_json(raw: &str) -> Result<Self> {
        let spec: Spec =
            serde_json::from_str(raw).context("the attention declaration is not JSON in the declared shape")?;
        let alpha = spec.alpha.unwrap_or(0.5);
        if !alpha.is_finite() || alpha <= 0.0 || alpha > 1.0 {
            return Err(anyhow::anyhow!(
                "field `alpha`: {alpha} is outside (0.0, 1.0] — 0 flattens every member and above 1 sharpens the lead"
            ));
        }
        Ok(Self {
            mode: spec.attention.unwrap_or(Mode::Damped),
            alpha,
            exempt: spec.exempt.unwrap_or_default(),
        })
    }

    /// Is this member ranked as `raw` regardless of the declaration?
    pub fn is_exempt(&self, handle: &str) -> bool {
        self.exempt.iter().any(|h| h.eq_ignore_ascii_case(handle))
    }

    /// What a member's corrected count is worth as a share of the pool's busiest, in `0.0..=1.0`.
    pub fn share(&self, value: f64, max: f64) -> f64 {
        match self.mode {
            Mode::Raw => {
                if max <= 0.0 {
                    0.0
                } else {
                    value / max
                }
            }
            Mode::Damped => {
                if max <= 0.0 || value <= 0.0 {
                    0.0
                } else {
                    ((1.0 + value).ln() / (1.0 + max).ln()).powf(self.alpha)
                }
            }
        }
    }

    /// The ratio a lead of `k×` becomes under this policy, for the docs and the tests. This is the number
    /// the roadmap means by "the declared ratio": it is a function of the declaration, not a constant.
    pub fn lead_after(&self, k: f64) -> f64 {
        match self.mode {
            Mode::Raw => k,
            Mode::Damped => ((1.0 + k).ln() / 2.0f64.ln()).powf(self.alpha),
        }
    }

    pub fn sentence(&self) -> String {
        let mut out = match self.mode {
            Mode::Raw => "raw: a lead of k times stays k times".to_string(),
            Mode::Damped => format!(
                "damped: a lead of 10 times becomes {:.2} times (alpha {:.2})",
                self.lead_after(10.0),
                self.alpha
            ),
        };
        // A declaration that exempts somebody says so here, by name, because this line is the only place the
        // policy is ever written down in full — and an operator who forgot a name must be able to see it.
        if !self.exempt.is_empty() {
            out.push_str(&format!("; {} exempt: {}", self.exempt.len(), self.exempt.join(", ")));
        }
        out
    }
}

/// Inverse-propensity weight for a click seen at `position`: `1 + ln p`, so a click at slot 9 counts about
/// 3.2× a click at slot 1 instead of a third of it. A position we never recorded is slot 1 — the least
/// credit, because we cannot show it was hard to find.
pub fn exposure(position: Option<i32>) -> f64 {
    match position {
        Some(p) if p > 1 => 1.0 + (p as f64).ln(),
        _ => 1.0,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    #[serde(default)]
    exempt: Option<Vec<String>>,
    #[serde(default)]
    attention: Option<Mode>,
    #[serde(default)]
    alpha: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_compresses_a_ten_fold_lead_to_the_declared_ratio() {
        let cfg = Config::default();
        let lead = cfg.lead_after(10.0);
        assert!((lead - 1.86).abs() < 0.02, "10x becomes {lead}, expected ≈1.86 at alpha 0.5");
        // And the share the ranker reads says the same thing. The pair is (10, max 10) against
        // (1, max 10), which is the same arithmetic as a lead of ten with the same additive offset.
        let busiest = cfg.share(10.0, 10.0);
        let tenth = cfg.share(1.0, 10.0);
        assert!((busiest / tenth - lead).abs() < 0.05, "{} vs {}", busiest / tenth, lead);
    }

    #[test]
    fn raw_is_exactly_the_old_arithmetic() {
        let cfg = Config::raw();
        assert_eq!(cfg.share(3.0, 12.0), 0.25);
        assert_eq!(cfg.lead_after(10.0), 10.0);
        assert_eq!(cfg.share(0.0, 0.0), 0.0);
    }

    #[test]
    fn exposure_credits_the_far_slot_more_than_the_top() {
        assert_eq!(exposure(Some(1)), 1.0);
        assert_eq!(exposure(None), 1.0, "an unrecorded position gets the least credit");
        assert!(exposure(Some(9)) > 3.0, "slot 9 is ~3.2x a slot-1 click: {}", exposure(Some(9)));
        assert!(exposure(Some(0)) == 1.0, "position 0 is not a real slot");
    }

    #[test]
    fn an_alpha_outside_the_range_is_refused_by_name() {
        for bad in ["0", "-1", "2"] {
            let err = Config::from_json(&format!(r#"{{"alpha":{bad}}}"#)).expect_err("refused");
            assert!(format!("{err:#}").contains("alpha"), "{err:#}");
        }
        assert!(Config::from_json(r#"{"alpha":1.0}"#).is_ok());
        assert!(Config::from_json(r#"{"attention":"damped","alpha":0.7}"#).is_ok());
        assert!(Config::from_json(r#"{"attention":"raw"}"#).is_ok());
        // Serde names both legal values when it refuses an unknown one, which is the whole message needed.
        let err = Config::from_json(r#"{"attention":"loud"}"#).expect_err("refused");
        let text = format!("{err:#}");
        assert!(text.contains("unknown variant") && text.contains("damped"), "{text}");
    }
}

#[cfg(test)]
mod declaration {
    use super::*;

    /// A misspelt key is refused by name rather than ignored: a declaration that is silently read as "the
    /// default" is worse than no declaration at all, because the operator believes it took effect.
    #[test]
    fn a_misspelt_field_is_refused_not_ignored() {
        let err = Config::from_json(r#"{"exmpt":["steeltitan"]}"#).unwrap_err().to_string();
        assert!(err.contains("exmpt") || err.contains("unknown field"), "{err}");
    }

    /// The wrong *type* is refused too: a list of names is not a string, and a declaration that says
    /// `"exempt": "steeltitan"` must fail loudly instead of exempting nobody.
    #[test]
    fn an_exempt_that_is_not_a_list_is_refused() {
        assert!(Config::from_json(r#"{"exempt":"steeltitan"}"#).is_err());
    }

    /// The list round-trips, case-insensitively, and the sentence names it — the one surface where an
    /// operator can see what they declared.
    #[test]
    fn the_list_is_read_and_reported() {
        let cfg = Config::from_json(r#"{"alpha":0.5,"exempt":["SteelTitan","criticalnexus"]}"#).unwrap();
        assert!(cfg.is_exempt("steeltitan"));
        assert!(cfg.is_exempt("CRITICALNEXUS"));
        assert!(!cfg.is_exempt("nobody"));
        let sentence = cfg.sentence();
        assert!(sentence.contains("2 exempt"), "{sentence}");
        assert!(sentence.contains("SteelTitan"), "the declaration is named back: {sentence}");
    }
}
