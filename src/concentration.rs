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

/// The floor under a long-tail claim, in impressions. Chosen so a day of traffic cannot produce one: below this
/// many impressions the share moves by whole percents on a single visitor's page view, and reporting it would be
/// reporting noise with a number attached.
pub const LONG_TAIL_FLOOR: i64 = 200;

/// And a floor on the *pool*, because impressions are not the only way to be too small: with two members the
/// "long tail" is one member and its share of the surface is half by arithmetic, not by any property of the shelf.
/// A share over a pool this thin measures the instrument, not the thing the instrument is pointed at.
pub const LONG_TAIL_MIN_MEMBERS: usize = 8;

/// What share of a window's impressions went to the members below the median exposure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongTail {
    pub total: i64,
    pub tail_impressions: i64,
    pub tail_members: usize,
    pub share: f64,
    pub window_hours: i64,
    /// Members in the pool (not only the ones with impressions): the second way a sample can be too small.
    pub pool_members: usize,
}

impl LongTail {
    /// The line `orbital impressions` prints — and the step's gate reads. Under the floor it refuses.
    pub fn verdict(&self) -> String {
        if self.total < LONG_TAIL_FLOOR {
            return format!(
                "NO VERDICT — only {} impression(s) in {}h; the floor is {}",
                self.total, self.window_hours, LONG_TAIL_FLOOR
            );
        }
        if self.pool_members < LONG_TAIL_MIN_MEMBERS {
            return format!(
                "NO VERDICT — {} member(s) in the pool; a tail needs at least {} to mean anything",
                self.pool_members, LONG_TAIL_MIN_MEMBERS
            );
        }
        format!(
            "long tail: {:.1}% of {} impression(s) in {}h went to the {} member(s) below the median exposure",
            self.share * 100.0,
            self.total,
            self.window_hours,
            self.tail_members
        )
    }
}

/// `rows` is `(exposure, impressions)` per member. The tail is the members whose exposure is *strictly* below the
/// median of the pool's exposure: an all-flat pool has no tail, and the answer is a zero share rather than a
/// fabricated half. `None` when the window holds no impressions at all — "nobody was shown anything" and "the tail
/// got nothing" are different facts.
pub fn long_tail(rows: &[(f64, i64)]) -> Option<LongTail> {
    let total: i64 = rows.iter().map(|(_, i)| *i).sum();
    if total <= 0 {
        return None;
    }
    let exposures: Vec<f64> = rows.iter().map(|(e, _)| *e).collect();
    let median = crate::exposure::percentile(&exposures, 0.5);
    let tail: Vec<&(f64, i64)> = rows.iter().filter(|(e, _)| *e < median).collect();
    let tail_impressions: i64 = tail.iter().map(|(_, i)| *i).sum();
    Some(LongTail {
        total,
        tail_impressions,
        tail_members: tail.len(),
        share: tail_impressions as f64 / total as f64,
        window_hours: 24,
        pool_members: rows.len(),
    })
}
