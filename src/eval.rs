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
    let mut cases = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut missing = 0usize;
    for session in seeds {
        if !seen.insert(session.clone()) {
            continue;
        }
        // The rows this session produced. A seed with no row is counted, never replaced.
        let hits: Vec<(Uuid, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT user_id, vertical_context, viewer_hash FROM source_hits WHERE session_hash = $1",
        )
        .bind(session)
        .fetch_all(pool)
        .await?;
        if hits.is_empty() {
            missing += 1;
            continue;
        }

        let clicked: Vec<Uuid> = hits
            .iter()
            .map(|(user, _, _)| *user)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        // The pool is the vertical the session was browsing; a session that browsed none is judged
        // against the whole catalogue, because that is the page it was given.
        let context = hits.iter().find_map(|(_, ctx, _)| ctx.clone());
        let pool_members = match &context {
            Some(slug) => crate::store::vertical_members(pool, slug).await?,
            None => crate::store::all_members(pool, true).await?,
        };
        // The same visibility rule the pages use, so the eval judges the pool a reader was given.
        let members: Vec<MemberStat> = pool_members
            .into_iter()
            .filter(crate::routes::public::visible)
            .collect();

        // The lens the session carried. `viewer_hash` is the unsalted visitor id the preference table
        // is keyed by; the caveat that this is the visitor's *current* profile is in the rule text.
        let mut lens_rows: Vec<(String, f64)> = Vec::new();
        for viewer in hits.iter().filter_map(|(_, _, viewer)| viewer.clone()) {
            let rows: Vec<(String, f64)> = sqlx::query_as(
                "SELECT tag_slug, weight::float8 FROM preference_weights
                  WHERE owner_kind = 'visitor' AND owner_id = $1",
            )
            .bind(&viewer)
            .fetch_all(pool)
            .await?;
            lens_rows.extend(rows);
        }

        cases.push(Case {
            session: session.clone(),
            members,
            weights: lens_of(&lens_rows),
            clicked,
        });
    }
    Ok((cases, missing))
}

/// Score both arms over the cases and compare them.
pub fn score(cases: &[Case], now: chrono::DateTime<chrono::Utc>) -> (Arm, Arm) {
    let mut arm_a = Arm::new();
    let mut arm_b = Arm::new();
    for (index, case) in cases.iter().enumerate() {
        let judged = judgements(case);
        if judged.is_empty() || case.members.is_empty() {
            // A case with nothing clicked is not a zero: it is not a case.
            continue;
        }
        let recency = crate::ranking::rank_new(case.members.clone(), now, &[], 1.0);
        let candidate = crate::ranking::rank_top(case.members.clone(), &case.weights, 1.0, crate::attention::Config::raw(), crate::ranking::Window::Lifetime);
        let a = ndcg_at(&handles(&recency), &judged, K);
        let b = ndcg_at(&handles(&candidate), &judged, K);
        if let (Some(a), Some(b)) = (a, b) {
            arm_a.insert(index as i64, a);
            arm_b.insert(index as i64, b);
        }
    }
    (arm_a, arm_b)
}

/// Run the whole thing: load, score, compare, decide.
pub async fn run(pool: &PgPool, golden: &Golden) -> Result<Report> {
    let (cases, missing) = collect(pool, &golden.seeds).await?;
    let (arm_a, arm_b) = score(&cases, chrono::Utc::now());
    let compared = paired::compare(&arm_a, &arm_b, golden.rule.min_cases.max(MIN_CASES));
    Ok(Report {
        cases: cases.len(),
        missing,
        paired: compared,
    })
}

/// The seed list, emitted from real rows: every session that clicked a member card, newest first.
pub async fn emit_seeds(pool: &PgPool, limit: i64) -> Result<Golden> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT session_hash FROM source_hits
          WHERE user_id IS NOT NULL
          GROUP BY session_hash
          ORDER BY max(created_at) DESC
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(Golden {
        seeds: rows.into_iter().map(|(session,)| session).collect(),
        ..Golden::empty()
    })
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

// ------------------------------------------------------------------------ entities (M6a)

/// One entity in an entity-evaluation pool. The ids are `entities.id`, not `users.id`: this arm
/// measures the catalogue the site now serves rather than the member roster it grew from.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EntityEvalRow {
    pub id: Uuid,
    pub slug: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tags: Vec<String>,
    pub visits: i64,
    pub clicks: i64,
}

#[derive(Debug, Clone)]
pub struct EntityCase {
    pub session: String,
    pub entities: Vec<EntityEvalRow>,
    pub weights: Vec<(String, f64)>,
    pub clicked: Vec<Uuid>,
}

impl Rule {
    pub fn fixed_entity() -> Self {
        Self {
            metric: format!("nDCG@{K}, graded relevance (a clicked entity = 2)"),
            arm_a: "recency — entities.created_at, newest first".into(),
            arm_b: "visits and clicks multiplied by the viewer's entity-tag lens".into(),
            min_cases: MIN_CASES,
            interval: "95% paired bootstrap, deterministic resampling (polaris-eval::paired)".into(),
            caveat: "entity clicks were collected while the site already ranked entities; a positive verdict bounds how much worse the candidate is, not how much better"
                .into(),
        }
    }
}

async fn entity_pool(pool: &PgPool, context: Option<&str>) -> Result<Vec<EntityEvalRow>> {
    Ok(sqlx::query_as::<_, EntityEvalRow>(
        "SELECT e.id, e.slug, e.created_at,
                COALESCE((SELECT array_agg(et.tag_slug) FROM entity_tags et
                           WHERE et.entity_id = e.id), ARRAY[]::text[]) AS tags,
                (SELECT count(DISTINCT sh.session_hash)::bigint FROM source_hits sh
                   JOIN entity_people ep ON ep.user_id = sh.user_id
                  WHERE ep.entity_id = e.id) AS visits,
                (SELECT count(*)::bigint FROM source_hits sh
                   JOIN entity_people ep ON ep.user_id = sh.user_id
                  WHERE ep.entity_id = e.id) AS clicks
           FROM entities e
          WHERE e.hidden = false AND e.merged_into IS NULL
            AND ($1::text IS NULL OR EXISTS (
                    SELECT 1 FROM categories c WHERE c.slug = e.category_slug AND c.slug = $1))",
    )
    .bind(context)
    .fetch_all(pool)
    .await?)
}

/// Load entity cases for the seed list. A session whose clicks do not reach any entity is counted as
/// missing: the golden set must not quietly shrink into a better-looking number.
pub async fn collect_entities(pool: &PgPool, seeds: &[String]) -> Result<(Vec<EntityCase>, usize)> {
    let mut cases = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut missing = 0usize;
    for session in seeds {
        if !seen.insert(session.clone()) {
            continue;
        }
        let hits: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT vertical_context, viewer_hash FROM source_hits WHERE session_hash = $1",
        )
        .bind(session)
        .fetch_all(pool)
        .await?;
        let clicked: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT ep.entity_id FROM entity_people ep
               JOIN source_hits sh ON sh.user_id = ep.user_id
              WHERE sh.session_hash = $1 AND ep.role = 'subject' AND ep.entity_id IS NOT NULL",
        )
        .bind(session)
        .fetch_all(pool)
        .await?;
        if hits.is_empty() || clicked.is_empty() {
            missing += 1;
            continue;
        }
        let context = hits.iter().find_map(|(ctx, _)| ctx.clone());
        let entities = entity_pool(pool, context.as_deref()).await?;
        if entities.is_empty() {
            missing += 1;
            continue;
        }
        let mut lens_rows: Vec<(String, f64)> = Vec::new();
        for viewer in hits.iter().filter_map(|(_, viewer)| viewer.clone()) {
            let rows: Vec<(String, f64)> = sqlx::query_as(
                "SELECT tag_slug, weight::float8 FROM preference_weights
                  WHERE owner_kind = 'visitor' AND owner_id = $1",
            )
            .bind(&viewer)
            .fetch_all(pool)
            .await?;
            lens_rows.extend(rows);
        }
        cases.push(EntityCase {
            session: session.clone(),
            entities,
            weights: lens_of(&lens_rows),
            clicked,
        });
    }
    Ok((cases, missing))
}


/// Arm A is the simplest honest order for a catalogue: newest first. Arm B rewards the evidence the
/// site has (visits and clicks) scaled by the viewer's tag lens — a lens matching two tags doubles the
/// evidence rather than adding a fixed bonus that would dominate a quiet catalogue.
fn rank_entities(case: &EntityCase) -> (Vec<String>, Vec<String>) {
    let mut recency: Vec<&EntityEvalRow> = case.entities.iter().collect();
    recency.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
    let lens: BTreeMap<&str, f64> = case
        .weights
        .iter()
        .map(|(tag, weight)| (tag.as_str(), *weight))
        .collect();
    let mut candidate: Vec<(f64, &EntityEvalRow)> = case
        .entities
        .iter()
        .map(|e| {
            let matched: f64 = e.tags.iter().filter_map(|t| lens.get(t.as_str()).copied()).sum();
            let evidence = e.visits as f64 + e.clicks as f64;
            (evidence * (1.0 + matched.max(0.0)), e)
        })
        .collect();
    candidate.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.created_at.cmp(&a.1.created_at))
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    (
        recency.into_iter().map(|e| e.id.to_string()).collect(),
        candidate.into_iter().map(|(_, e)| e.id.to_string()).collect(),
    )
}

pub fn score_entities(cases: &[EntityCase]) -> (Arm, Arm) {
    let mut arm_a = Arm::new();
    let mut arm_b = Arm::new();
    for (index, case) in cases.iter().enumerate() {
        let judged: Vec<Judgement> = case
            .clicked
            .iter()
            .map(|id| Judgement {
                work_id: id.to_string(),
                grade: 2,
            })
            .collect();
        if judged.is_empty() || case.entities.is_empty() {
            continue;
        }
        let (a_ids, b_ids) = rank_entities(case);
        if let (Some(a), Some(b)) = (ndcg_at(&a_ids, &judged, K), ndcg_at(&b_ids, &judged, K)) {
            arm_a.insert(index as i64, a);
            arm_b.insert(index as i64, b);
        }
    }
    (arm_a, arm_b)
}

pub async fn run_entities(pool: &PgPool, golden: &Golden) -> Result<Report> {
    let (cases, missing) = collect_entities(pool, &golden.seeds).await?;
    let (arm_a, arm_b) = score_entities(&cases);
    let compared = paired::compare(&arm_a, &arm_b, golden.rule.min_cases.max(MIN_CASES));
    Ok(Report {
        cases: cases.len(),
        missing,
        paired: compared,
    })
}

/// Seeds for the entity arm: sessions whose clicks actually reached an entity through a subject link.
pub async fn emit_entity_seeds(pool: &PgPool, limit: i64) -> Result<Golden> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT sh.session_hash FROM source_hits sh
           JOIN entity_people ep ON ep.user_id = sh.user_id AND ep.role = 'subject'
          WHERE ep.entity_id IS NOT NULL
          GROUP BY sh.session_hash
          ORDER BY max(sh.created_at) DESC
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(Golden {
        seeds: rows.into_iter().map(|(session,)| session).collect(),
        ..Golden {
            generated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            rule: Rule::fixed_entity(),
            seeds: Vec::new(),
        }
    })
}
