//! The taxonomy, after launch.
//!
//! Two decisions a directory cannot keep to itself forever: which verticals exist, and which tags
//! are worth taking seriously. Both are proposals with a quorum rather than votes that bind, and
//! both leave a trail.

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::User;

/// How many TL3 votes a proposal needs. Three is small enough that a directory of four can still
/// decide anything, and large enough that one person cannot rename the site.
pub const QUORUM: i64 = 3;
/// TL2 and up may endorse a tag.
pub const ENDORSE_TRUST: i16 = 2;
/// Three endorsements reach the full boost.
pub const ENDORSE_FULL: f64 = 3.0;
/// The most an endorsement can add to the relevance term.
pub const ENDORSE_BOOST: f64 = 0.2;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct Proposal {
    pub id: Uuid,
    pub slug: String,
    pub label: String,
    pub description: String,
    pub merged_into: Option<String>,
    pub status: String,
    pub votes: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub proposer: String,
}

pub async fn list(pool: &PgPool) -> Result<Vec<Proposal>, AppError> {
    Ok(sqlx::query_as(
        "SELECT p.id, p.slug, p.label, p.description, p.merged_into, p.status,
                (SELECT count(*) FROM vertical_votes v WHERE v.proposal_id = p.id) AS votes,
                p.created_at, u.handle AS proposer
           FROM vertical_proposals p JOIN users u ON u.id = p.proposed_by
          ORDER BY (p.status = 'open') DESC, p.created_at DESC",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn propose(
    pool: &PgPool,
    user: &User,
    slug: &str,
    label: &str,
    description: &str,
    merged_into: Option<&str>,
) -> Result<Uuid, AppError> {
    let slug = crate::text::normalise_slug(slug)?;
    let label = crate::text::truncate(label.trim(), 40);
    if label.is_empty() {
        return Err(AppError::Validation("a proposal needs a label".into()));
    }
    if let Some(target) = merged_into {
        // A merge moves people, so the target has to exist before the vote, not after it.
        if target == slug {
            return Err(AppError::Validation("a vertical cannot merge into itself".into()));
        }
        if crate::store::vertical_by_slug(pool, target).await?.is_none() {
            return Err(AppError::Validation(format!("no vertical `{target}`")));
        }
        if crate::store::vertical_by_slug(pool, &slug).await?.is_none() {
            return Err(AppError::Validation(format!("no vertical `{slug}` to merge")));
        }
    } else if crate::store::vertical_by_slug(pool, &slug).await?.is_some() {
        return Err(AppError::Conflict(format!("`{slug}` already exists")));
    }
    let id: Uuid = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO vertical_proposals (slug, label, description, proposed_by, merged_into)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&slug)
    .bind(&label)
    .bind(crate::text::truncate(description.trim(), 200))
    .bind(user.id)
    .bind(merged_into)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Vote for a proposal. Returns the tally and whether the proposal was just accepted.
///
/// The vote that crosses the quorum accepts the proposal in the same transaction: leaving it open
/// for a second call would let two voters race and merge a vertical twice.
pub async fn vote(pool: &PgPool, user: &User, id: Uuid) -> Result<(i64, bool), AppError> {
    if user.trust_level < 3 && !user.is_owner {
        return Err(AppError::Forbidden);
    }
    let mut tx = pool.begin().await?;
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT slug, merged_into, status FROM vertical_proposals WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (slug, merged_into, status) = row.ok_or(AppError::NotFound)?;
    if status != "open" {
        return Err(AppError::Conflict("this proposal is already decided".into()));
    }
    sqlx::query(
        "INSERT INTO vertical_votes (proposal_id, user_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    let votes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vertical_votes WHERE proposal_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    let accepted = votes >= QUORUM;
    if accepted {
        apply(&mut tx, &slug, &merged_into).await?;
        sqlx::query(
            "UPDATE vertical_proposals SET status = 'accepted', decided_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((votes, accepted))
}

/// Reject or accept by hand, for the case where the quorum is not the right tool.
pub async fn decide(pool: &PgPool, id: Uuid, accept: bool) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT slug, merged_into FROM vertical_proposals WHERE id = $1 AND status = 'open'",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (slug, merged_into) = row.ok_or(AppError::NotFound)?;
    if accept {
        apply(&mut tx, &slug, &merged_into).await?;
    }
    sqlx::query(
        "UPDATE vertical_proposals SET status = $2, decided_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(if accept { "accepted" } else { "rejected" })
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Carry out an accepted proposal: create the vertical, or move everyone off the retiring one.
async fn apply(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    slug: &str,
    merged_into: &Option<String>,
) -> Result<(), AppError> {
    match merged_into {
        None => {
            // The proposal carries the words the vertical is made of, so it is one statement.
            sqlx::query(
                "INSERT INTO verticals (slug, label, description, icon, nsfw, sort)
                 SELECT p.slug, p.label, p.description, 'link', false, 500
                   FROM vertical_proposals p
                  WHERE p.slug = $1 AND p.status = 'open'
                 ON CONFLICT (slug) DO UPDATE SET label = EXCLUDED.label,
                                                 description = EXCLUDED.description",
            )
            .bind(slug)
            .execute(&mut **tx)
            .await?;
        }
        Some(target) => {
            // The members come with their tags: a merge is a rename of the room, not an eviction.
            // Someone whose primary vertical disappears becomes primary in the target.
            sqlx::query(
                "UPDATE user_verticals SET vertical_slug = $2
                  WHERE vertical_slug = $1
                    AND NOT EXISTS (SELECT 1 FROM user_verticals x
                                     WHERE x.user_id = user_verticals.user_id
                                       AND x.vertical_slug = $2)",
            )
            .bind(slug)
            .bind(target)
            .execute(&mut **tx)
            .await?;
            // Anyone who already had the target keeps the higher role.
            sqlx::query(
                "UPDATE user_verticals uv SET role = 'primary'
                  WHERE uv.vertical_slug = $2
                    AND EXISTS (SELECT 1 FROM user_verticals x
                                 WHERE x.user_id = uv.user_id AND x.vertical_slug = $1
                                   AND x.role = 'primary')",
            )
            .bind(slug)
            .bind(target)
            .execute(&mut **tx)
            .await?;
            sqlx::query("DELETE FROM user_verticals WHERE vertical_slug = $1")
                .bind(slug)
                .execute(&mut **tx)
                .await?;
            sqlx::query("UPDATE links SET vertical_slug = $2 WHERE vertical_slug = $1")
                .bind(slug)
                .bind(target)
                .execute(&mut **tx)
                .await?;
            sqlx::query("DELETE FROM verticals WHERE slug = $1")
                .bind(slug)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

/// Endorse a tag, or take the endorsement back. Returns the new count.
pub async fn endorse(pool: &PgPool, user: &User, tag: &str) -> Result<i64, AppError> {
    if user.trust_level < ENDORSE_TRUST && !user.is_owner {
        return Err(AppError::Forbidden);
    }
    let tag = crate::text::normalise_tag(tag)?;
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM tag_endorsements WHERE tag_slug = $1 AND user_id = $2",
    )
    .bind(&tag)
    .bind(user.id)
    .fetch_optional(pool)
    .await?;
    if existing.is_some() {
        sqlx::query("DELETE FROM tag_endorsements WHERE tag_slug = $1 AND user_id = $2")
            .bind(&tag)
            .bind(user.id)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("INSERT INTO tag_endorsements (tag_slug, user_id) VALUES ($1, $2)")
            .bind(&tag)
            .bind(user.id)
            .execute(pool)
            .await?;
    }
    Ok(count(pool, &tag).await?)
}

pub async fn count(pool: &PgPool, tag: &str) -> Result<i64, AppError> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM tag_endorsements WHERE tag_slug = $1")
            .bind(tag)
            .fetch_one(pool)
            .await?,
    )
}

/// Per-tag boost, 0..=0.2, from the endorsements a tag has.
pub async fn boosts(pool: &PgPool) -> Result<HashMap<String, f64>, AppError> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT tag_slug, count(*)::bigint FROM tag_endorsements GROUP BY 1")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(tag, n)| {
            let share = (n as f64 / ENDORSE_FULL).min(1.0);
            (tag, ENDORSE_BOOST * share)
        })
        .collect())
}
