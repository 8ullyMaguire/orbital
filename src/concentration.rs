//! How concentrated each surface's attention is: Gini and the top-10 share of impressions.
//!
//! This is the measurement the fame headwind (D34) is judged by, and it exists **before** any damping —
//! a policy that cannot be measured is a preference. Two properties make it useful rather than decorative:
//!
//! * **Per surface, never aggregated.** Home, a vertical, a profile and the ring have different pools and
//!   different lifetimes; one number over all of them would be an average of four questions.
//! * **Weekly, and idempotent.** A row is keyed by `(surface, week)`, so re-running a snapshot inside the
//!   same week corrects the row rather than appending a second one — a series with two rows per week is a
//!   series nobody can read a trend from.
//!
//! Nothing here is served. `orbital doctor` prints it, and `docs/` describes it; there is no route, field
//! or page, because a public concentration number is a statement about whose work the site favours.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;

/// The share that says "the front page belongs to a handful of people".
pub const TOP_K: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub surface: String,
    pub week: NaiveDate,
    pub gini: f64,
    pub top10: f64,
    pub members: i64,
    pub impressions: i64,
}

impl Snapshot {
    /// One operator-facing line. Percentages, because a Gini of 0.83 is a sentence and 0.8302 is not.
    pub fn sentence(&self) -> String {
        format!(
            "{} {}: gini {:.2}, top-{} {:.1}% of {} impressions over {} members",
            self.surface,
            self.week,
            self.gini,
            TOP_K,
            self.top10 * 100.0,
            self.impressions,
            self.members
        )
    }
}

/// The Gini coefficient of a distribution, `0.0` for a perfectly even split and approaching `1.0` when one
/// item holds everything. Computed as the mean absolute difference over twice the mean, which is the form
/// that needs no sorting and no cumulative sums — the pools here are small and the formula is harder to get
/// subtly wrong.
pub fn gini(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    if n == 0.0 {
        return 0.0;
    }
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        // Only empty counts can reach here, and "no impressions" is not "perfectly even".
        return 0.0;
    }
    let mean = total / n;
    let mut difference = 0.0;
    for a in values {
        for b in values {
            difference += (a - b).abs();
        }
    }
    difference / (2.0 * n * n * mean)
}

/// The share of the total held by the `k` heaviest items.
pub fn top_share(values: &[f64], k: usize) -> f64 {
    let total: f64 = values.iter().sum();
    if values.is_empty() || total <= 0.0 {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    sorted.iter().take(k).sum::<f64>() / total
}

/// Impression counts per member on one surface, all time.
pub async fn shares(pool: &PgPool, surface: &str) -> Result<Vec<f64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT count(*) FROM source_hits WHERE surface = $1 AND user_id IS NOT NULL GROUP BY user_id",
    )
    .bind(surface)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(n,)| n as f64).collect())
}

/// The surfaces that actually have rows. Read from the data, never from a list in this file: a surface
/// nobody used is not a surface with a concentration of zero.
pub async fn surfaces(pool: &PgPool) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT surface FROM source_hits WHERE user_id IS NOT NULL ORDER BY surface",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

/// Take this week's snapshot for every surface, correcting the row if it already exists.
pub async fn record(pool: &PgPool, as_of: DateTime<Utc>) -> Result<Vec<Snapshot>> {
    let mut written = Vec::new();
    for surface in surfaces(pool).await? {
        let values = shares(pool, &surface).await?;
        if values.is_empty() {
            continue;
        }
        let gini = gini(&values);
        let top10 = top_share(&values, TOP_K);
        let members = values.len() as i32;
        let impressions: i64 = values.iter().sum::<f64>() as i64;
        let (week,): (NaiveDate,) = sqlx::query_as(
            "INSERT INTO concentration_snapshots (surface, week, gini, top10, members, impressions)
             VALUES ($1, date_trunc('week', $2::timestamptz)::date, $3, $4, $5, $6)
             ON CONFLICT (surface, week) DO UPDATE
                SET gini = EXCLUDED.gini, top10 = EXCLUDED.top10,
                    members = EXCLUDED.members, impressions = EXCLUDED.impressions,
                    taken_at = now()
             RETURNING week",
        )
        .bind(&surface)
        .bind(as_of)
        .bind(gini)
        .bind(top10)
        .bind(members)
        .bind(impressions)
        .fetch_one(pool)
        .await?;
        written.push(Snapshot {
            surface,
            week,
            gini,
            top10,
            members: members as i64,
            impressions,
        });
    }
    Ok(written)
}

/// The most recent row per surface.
pub async fn latest(pool: &PgPool) -> Result<Vec<Snapshot>> {
    let rows: Vec<(String, NaiveDate, f64, f64, i32, i64)> = sqlx::query_as(
        "SELECT DISTINCT ON (surface) surface, week, gini, top10, members, impressions
           FROM concentration_snapshots
          ORDER BY surface, week DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(surface, week, gini, top10, members, impressions)| Snapshot {
            surface,
            week,
            gini,
            top10,
            members: members as i64,
            impressions,
        })
        .collect())
}
