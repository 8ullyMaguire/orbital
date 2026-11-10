//! The instance's taste: declared sources, folded from the rows orbital really has, reported to the
//! operator alone.
//!
//! The arithmetic is `polaris-taste-source` (D31: adopt, delete our copy). This module owns exactly
//! three things the crate must not know: which sources this instance declares (D30), which of
//! orbital's tables are evidence, and where the report is allowed to appear — `orbital doctor`, and
//! nowhere a reader can see (D27: the prior is applied and never named).
//!
//! Two of orbital's tables are deliberately **not** evidence:
//!
//! * `tags` — the table has no `created_at`, so a row there could never decay, and a prior that
//!   cannot decay is a permanent claim about a person who may have changed.
//! * `source_hits` — a click is not a statement; the crate's vocabulary has no word for it, on
//!   purpose.
//!
//! What is evidence today: `tag_endorsements` (a stated endorsement of one tag → `Like`).

use anyhow::{anyhow, Context, Result};
use polaris_taste_source::{
    fold, Contributor, Declaration, Evidence, Kind, ProfileEntry, Selector, Source, SourceReport,
};
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::BTreeMap;

/// The declaration, plus where it came from. Loading it is a boot-time act: an invalid declaration
/// must stop the process before it can serve anything, naming the field that is wrong.
#[derive(Clone, Debug)]
pub struct TasteConfig {
    pub declaration: Declaration,
    /// `None` when the built-in D30 declaration is in force, `Some(path)` when a file overrode it.
    pub origin: Option<String>,
}

/// The folded profile and the per-source report. Nothing here is public: `report_lines` is read by
/// `orbital doctor`, and the profile itself is read by ranking code that may not name it.
#[derive(Clone, Debug)]
pub struct Taste {
    pub declaration: Declaration,
    pub profile: Vec<ProfileEntry>,
    pub report: Vec<SourceReport>,
}

impl TasteConfig {
    /// `TASTE_FILE` names a JSON declaration. Absent, the instance runs on the declaration D30 fixed
    /// for it — a file is an override, not a requirement, so a fresh checkout boots.
    pub fn from_env() -> Result<Self> {
        match std::env::var("TASTE_FILE").ok().filter(|p| !p.trim().is_empty()) {
            Some(path) => {
                let raw = std::fs::read_to_string(&path).with_context(|| {
                    format!("taste declaration: TASTE_FILE={path} could not be read")
                })?;
                let mut cfg = Self::from_json(&raw)
                    .with_context(|| format!("taste declaration: TASTE_FILE={path}"))?;
                cfg.origin = Some(path);
                Ok(cfg)
            }
            None => Ok(Self {
                declaration: default_declaration(),
                origin: None,
            }),
        }
    }

    /// Literal config for tests: the built-in declaration, never the environment.
    pub fn for_test() -> Self {
        Self {
            declaration: default_declaration(),
            origin: None,
        }
    }

    /// Parse and **validate** a declaration. Validation is the crate's, not ours: a weight of 30 is a
    /// typo, and silently serving 1.0 would hide it.
    pub fn from_json(raw: &str) -> Result<Self> {
        let spec: DeclSpec = serde_json::from_str(raw)
            .context("field `sources`: the declaration is not JSON in the declared shape")?;

        let mut sources = Vec::with_capacity(spec.sources.len());
        for source in spec.sources {
            let mut kinds = Vec::with_capacity(source.kinds.len());
            for word in &source.kinds {
                let kind = Kind::parse(word).ok_or_else(|| {
                    let vocabulary: Vec<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
                    anyhow!(
                        "field `kinds` of source `{}`: `{}` is not one of {}",
                        source.name,
                        word,
                        vocabulary.join(", ")
                    )
                })?;
                kinds.push(kind);
            }
            sources.push(Source {
                name: source.name,
                selector: source.selector.into_selector(),
                weight: source.weight,
                kinds,
            });
        }

        let declaration = Declaration {
            sources,
            half_life_days: spec.half_life_days,
            min_evidence: spec.min_evidence,
        };
        declaration.validate().map_err(|invalid| anyhow!(describe(&invalid)))?;
        Ok(Self {
            declaration,
            origin: None,
        })
    }
}

/// The declaration the owner fixed on 2026-11-10 (D30). Declaration order is meaningful — the crate
/// gives a contributor the **first** source that admits them, so a named account is never read as
/// "everyone". The trust selector is declared *nowhere* on purpose: D30 has it off until there are
/// trust levels worth reading, and a source that admits nobody would only add a line of noise.
pub fn default_declaration() -> Declaration {
    Declaration {
        sources: vec![
            Source {
                name: "criticalnexus".into(),
                selector: Selector::Explicit {
                    ids: vec!["criticalnexus".into()],
                },
                weight: 0.3,
                kinds: vec![Kind::Seed, Kind::Like, Kind::Ballot],
            },
            Source {
                name: "steeltitan".into(),
                selector: Selector::Explicit {
                    ids: vec!["steeltitan".into()],
                },
                weight: 0.7,
                kinds: vec![Kind::Seed, Kind::Like, Kind::Ballot],
            },
            Source {
                name: "attuned".into(),
                selector: Selector::Affinity {
                    at_least: 0.5,
                    min_evidence: 20,
                },
                weight: 0.2,
                kinds: vec![Kind::Like, Kind::Ballot],
            },
            Source {
                name: "everyone".into(),
                selector: Selector::Everyone,
                weight: 0.05,
                kinds: vec![Kind::Like, Kind::Ballot],
            },
        ],
        half_life_days: 90.0,
        min_evidence: 1,
    }
}

/// Fold the declaration over the rows orbital has. `absent` is a status here, never a zero: a source
/// that admits nobody is reported and the profile is simply empty (the crate's third rule).
pub async fn load(pool: &PgPool, declaration: &Declaration) -> Result<Taste> {
    let contributors = contributors(pool).await?;
    // Owned rows first, then borrowed evidence: the crate takes `&str`, and leaking a String per
    // endorsement per `doctor` run is not a trade worth making for a lifetime annotation.
    let rows: Vec<(String, String, f64)> = sqlx::query_as(
        "SELECT u.handle, e.tag_slug,
                (EXTRACT(EPOCH FROM (now() - e.created_at)) / 86400.0)::float8
           FROM tag_endorsements e
           JOIN users u ON u.id = e.user_id",
    )
    .fetch_all(pool)
    .await?;
    let evidence: Vec<Evidence> = rows
        .iter()
        .map(|(handle, tag, age_days)| Evidence {
            contributor: handle.as_str(),
            kind: Kind::Like,
            subject: tag.as_str(),
            strength: 1.0,
            age_days: *age_days,
        })
        .collect();
    let (profile, report) = fold(declaration, &contributors, &evidence);
    Ok(Taste {
        declaration: declaration.clone(),
        profile,
        report,
    })
}

impl Taste {
    /// One `(what, detail)` pair per declared source, plus a profile summary. The status word is the
    /// point: `absent` says the source exists and admitted nothing, which is a different answer from
    /// a source that contributed a weight of 0.0 to every subject.
    pub fn report_lines(&self) -> Vec<(String, String)> {
        self.report
            .iter()
            .map(|source| {
                let status = if source.contributed > 0 { "active" } else { "absent" };
                let mut detail =
                    format!("{status}: admitted {}, contributed {}", source.admitted, source.contributed);
                if !source.note.is_empty() {
                    detail.push_str(" — ");
                    detail.push_str(&source.note);
                }
                (format!("taste/{}", source.name), detail)
            })
            .collect()
    }

    /// Subject → folded weight, for the rankers that consume the prior. Absent subjects are absent,
    /// not `0.0`: `get` returning `None` is the whole contract.
    pub fn weights(&self) -> BTreeMap<String, f64> {
        self.profile
            .iter()
            .map(|e| (e.subject.clone(), e.weight))
            .collect()
    }

    /// One operator-facing line: how many subjects, and the heaviest three. Handles are safe here
    /// because the report never leaves this process.
    pub fn summary(&self) -> String {
        if self.profile.is_empty() {
            return "empty (no declared source contributed evidence of a kind it may carry)".into();
        }
        let heaviest: Vec<String> = self
            .profile
            .iter()
            .take(3)
            .map(|e| format!("{} {:.2}", e.subject, e.weight))
            .collect();
        format!(
            "{} subjects; heaviest: {}",
            self.profile.len(),
            heaviest.join(", ")
        )
    }
}

/// Real members, with the facts the selectors read. `affinity` is `None` because orbital has no
/// affinity engine yet: the `attuned` selector therefore reports `absent` rather than admitting
/// everybody on a zero.
async fn contributors(pool: &PgPool) -> Result<Vec<Contributor>> {
    let rows: Vec<(String, i16, bool)> =
        sqlx::query_as("SELECT handle, trust_level, is_owner FROM users ORDER BY handle")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(handle, trust, owner)| Contributor {
            id: handle,
            roles: if owner {
                vec!["role:operator".to_string()]
            } else {
                Vec::new()
            },
            trust_level: trust.clamp(0, 3) as u8,
            badges: Vec::new(),
            affinity: None,
            affinity_evidence: 0,
        })
        .collect())
}

/// The JSON shape of a declaration file. `kinds` uses the crate's vocabulary verbatim: an unknown
/// word is an error that names it, because a vocabulary that grows per site is not a vocabulary.
#[derive(Deserialize)]
struct DeclSpec {
    sources: Vec<SourceSpec>,
    #[serde(default = "default_half_life")]
    half_life_days: f64,
    #[serde(default = "default_min_evidence")]
    min_evidence: u32,
}

#[derive(Deserialize)]
struct SourceSpec {
    name: String,
    selector: SelectorSpec,
    weight: f64,
    kinds: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SelectorSpec {
    Role { role: String },
    TrustLevel { at_least: u8 },
    Badge { badge: String },
    Affinity { at_least: f64, min_evidence: u32 },
    Explicit { ids: Vec<String> },
    Everyone,
}

impl SelectorSpec {
    fn into_selector(self) -> Selector {
        match self {
            SelectorSpec::Role { role } => Selector::Role { role },
            SelectorSpec::TrustLevel { at_least } => Selector::TrustLevel { at_least },
            SelectorSpec::Badge { badge } => Selector::Badge { badge },
            SelectorSpec::Affinity {
                at_least,
                min_evidence,
            } => Selector::Affinity {
                at_least,
                min_evidence,
            },
            SelectorSpec::Explicit { ids } => Selector::Explicit { ids },
            SelectorSpec::Everyone => Selector::Everyone,
        }
    }
}

fn default_half_life() -> f64 {
    90.0
}

fn default_min_evidence() -> u32 {
    1
}

/// Turn the crate's refusal into a sentence that names the **field** and the **source**, because the
/// operator's next action is to edit one line of one file.
fn describe(invalid: &polaris_taste_source::Invalid) -> String {
    use polaris_taste_source::Invalid;
    match invalid {
        Invalid::NoSources => "field `sources`: a declaration with no sources can never contribute".into(),
        Invalid::DuplicateSource(name) => {
            format!("field `sources.name`: two sources are named `{name}`")
        }
        Invalid::WeightOutOfRange(name, w) => {
            format!("field `weight` of source `{name}`: {w} is outside 0.0..=1.0")
        }
        Invalid::NoKinds(name) => {
            format!("field `kinds` of source `{name}`: empty, so the source may say nothing")
        }
        Invalid::BadHalfLife(d) => format!("field `half_life_days`: {d} is not a positive number"),
        Invalid::EmptyName => "field `sources.name`: a source needs a stable name".into(),
    }
}
