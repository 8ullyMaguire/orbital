//! The offline evaluation: does the ranking put where a reader actually went above where a recency
//! list would put it?
//!
//! The arithmetic is `polaris-eval` (D31): nDCG@10 over graded judgements, and a **paired** bootstrap
//! that refuses to call a difference a result when the sample is small or the interval straddles zero.
//! Orbital owns the cases — which are real rows, never invented ones — and the decision rule.
//!
//! # The cases
//!
//! One case per **session that clicked a member card**. The judgement is the click itself: grade 2 for
//! a member this session clicked, nothing for the rest. There are no grade-0 judgements, because the
//! site does not record what it *showed* — only what was clicked — and inventing "they saw it and did
//! not click" would be inventing rows.
//!
//! # The arms
//!
//! * **A — recency**: `rank_new`, the simplest defensible order. Any ranker that cannot beat "newest
//!   first" on real clicks has not earned its arithmetic.
//! * **B — candidate**: `rank_top` with the viewer's lens, which is the ranking the site serves.
//!
//! # What this instrument cannot tell you
//!
//! The clicks were collected while the site was already ordering by the candidate, so a positive
//! verdict is evidence that the candidate is *not worse than recency for the member somebody clicked* —
//! not proof that it is a better ranking. That is why the verdict is a property of the interval and the
//! sample floor, and why the first honest answer on a two-member site is `NO VERDICT`.

use anyhow::{Context, Result};
use polaris_eval::{ndcg_at, paired, Judgement};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::models::MemberStat;

/// The metric's `k`: the first page is what a reader sees, and nothing below it is measured.
pub const K: usize = 10;

/// The sample floor, fixed **before** any data arrives. `polaris-eval` ships 30; orbital states its own
/// so that raising it is a reviewed change to this line rather than a quiet edit somewhere else.
pub const MIN_CASES: usize = 30;

/// One replayable case: a session, the pool it was looking at, the lens it carried, and what it clicked.
#[derive(Debug, Clone)]
pub struct Case {
    /// The session hash, which is the case id and the pairing key. Real, from `source_hits`.
    pub session: String,
    pub members: Vec<MemberStat>,
    pub weights: Vec<(String, f64)>,
    /// The members this session clicked, with the grade the click earns.
    pub clicked: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Golden {
    /// When the seed list was emitted from the live rows.
    pub generated: String,
    /// The decision rule, recorded beside the seeds so a later run cannot quietly move it.
    pub rule: Rule,
    /// Session hashes, real rows, in the order they were emitted.
    pub seeds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub metric: String,
    pub arm_a: String,
    pub arm_b: String,
    pub min_cases: usize,
    pub interval: String,
    /// What this instrument does **not** prove. Recorded with the numbers so the caveat cannot be lost.
    pub caveat: String,
}

impl Rule {
    pub fn fixed() -> Self {
        Self {
            metric: format!("nDCG@{K}, graded relevance (a clicked member = 2)"),
            arm_a: "rank_new — newest first".into(),
            arm_b: "rank_top — clicks and the viewer's own lens".into(),
            min_cases: MIN_CASES,
            interval: "95% paired bootstrap, deterministic resampling (polaris-eval::paired)".into(),
            caveat: "clicks were collected under the candidate's own ordering, so a positive verdict \
                     bounds how much worse the candidate is, not how much better"
                .into(),
        }
    }
}

impl Golden {
    pub fn empty() -> Self {
        Self {
            generated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            rule: Rule::fixed(),
            seeds: Vec::new(),
        }
    }

    pub fn from_json(raw: &str) -> Result<Self> {
        let golden: Golden =
            serde_json::from_str(raw).context("the golden set is not JSON in the declared shape")?;
        if golden.seeds.is_empty() {
            return Err(anyhow::anyhow!(
                "the golden set holds no seeds: emit them with `orbital eval --emit`"
            ));
        }
        Ok(golden)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
}

/// What one arm scored for one case.
pub type Arm = BTreeMap<i64, f64>;

#[derive(Debug, Clone)]
pub struct Report {
    pub cases: usize,
    /// Seeds in the golden set that no longer match a row: a golden set that silently shrinks is a
    /// number that improves while nothing does.
    pub missing: usize,
    pub paired: paired::Paired,
}

impl Report {
    /// The verdict as a line an operator can paste. `NO VERDICT` is a result, not a failure of the run.
    pub fn sentence(&self) -> String {
        format!(
            "{} — {} cases, {} seeds missing, A {:.4} / B {:.4}, delta {:+.4} [{:+.4}, {:+.4}]. {}",
            match self.paired.verdict {
                paired::Verdict::Better => "VERDICT: better",
                paired::Verdict::Worse => "VERDICT: worse",
                paired::Verdict::NoVerdict => "NO VERDICT",
            },
            self.cases,
            self.missing,
            self.paired.mean_a,
            self.paired.mean_b,
            self.paired.delta,
            self.paired.ci_lo,
            self.paired.ci_hi,
            self.paired.why
        )
    }

    /// A run that cannot beat the baseline blocks the change: exit code 1.
    pub fn exit_code(&self) -> i32 {
        match self.paired.verdict {
            paired::Verdict::Better => 0,
            _ => 1,
        }
    }
}

/// Load the cases for a seed list. Missing seeds are counted, never replaced.
pub async fn collect(pool: &PgPool, seeds: &[String]) -> Result<(Vec<Case>, usize)> {
    unimplemented!("RED: load the cases a golden set names")
}

/// Score both arms over the cases and compare them.
pub fn score(cases: &[Case], now: chrono::DateTime<chrono::Utc>) -> (Arm, Arm) {
    unimplemented!("RED: score recency against the candidate")
}

/// Run the whole thing: load, score, compare, decide.
pub async fn run(pool: &PgPool, golden: &Golden) -> Result<Report> {
    unimplemented!("RED: run the comparison")
}

/// The seed list, emitted from real rows: every session that clicked a member card, newest first.
pub async fn emit_seeds(pool: &PgPool, limit: i64) -> Result<Golden> {
    unimplemented!("RED: emit seeds from what the database holds")
}

/// Turn one case into the two ranked lists and the judgements the metric reads.
fn judgements(case: &Case) -> Vec<Judgement> {
    case.clicked
        .iter()
        .map(|id| Judgement {
            work_id: id.to_string(),
            grade: 2,
        })
        .collect()
}

fn handles(members: &[MemberStat]) -> Vec<String> {
    members.iter().map(|m| m.id.to_string()).collect()
}

/// The lens a case carries, deduplicated and ordered, so two runs over the same rows agree.
fn lens_of(rows: &[(String, f64)]) -> Vec<(String, f64)> {
    let mut map: BTreeMap<String, f64> = BTreeMap::new();
    for (tag, weight) in rows {
        let slot = map.entry(tag.clone()).or_insert(0.0);
        *slot = slot.max(*weight);
    }
    map.into_iter().collect()
}
