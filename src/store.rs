//! All SQL lives here. Every column list is explicit: a `SELECT *` plus a
//! serializer that reads a column the query omits is a panic in the request
//! worker, and every aggregate is cast because `count(*)` is int4 and the
//! structs hold i64.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::*;

const USER_COLS: &str = "id, handle, display_name, bio, accent, avatar_url, bsky_handle, \
     bsky_verified_at, verify_token, trust_level, nsfw_enabled, is_owner, hidden, created_at, \
     last_active_at";

/// Qualified variant: `sessions` also has `id` and `created_at`, so the
/// unqualified list is an ambiguous reference in the session lookup.
const USER_COLS_U: &str = "u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, \
     u.bsky_handle, u.bsky_verified_at, u.verify_token, u.trust_level, u.nsfw_enabled, \
     u.is_owner, u.hidden, u.created_at, u.last_active_at";

const LINK_COLS: &str = "id, user_id, vertical_slug, url, label, icon, position, click_count, \
     hidden, created_at, last_status, last_checked, fail_count, title, favicon";

/// Same list, qualified: `links_for` joins `verticals`, which also has `label`
/// and `icon` columns, so an unqualified list is an ambiguous-reference error.
const LINK_COLS_L: &str = "l.id, l.user_id, l.vertical_slug, l.url, l.label, l.icon, \
     l.position, l.click_count, l.hidden, l.created_at, l.last_status, l.last_checked, l.fail_count, \
     l.title, l.favicon";

const VERTICAL_COLS: &str = "slug, label, description, icon, nsfw, sort, min_members";

/// A member whose PRIMARY vertical is the adult one must never appear on the SFW
/// host, even when they also list an SFW vertical as a secondary: that listing
/// would disclose their adult membership. Written once and reused, because four
/// restatements of a visibility rule are four different rules.
const SFW_PRIMARY_ONLY: &str = "NOT EXISTS (SELECT 1 FROM user_verticals uv3 \
     WHERE uv3.user_id = u.id AND uv3.role = 'primary' AND uv3.vertical_slug = 'nsfw')";

/// Visitor and click stats computed **in one vertical's context**: a click
/// recorded while the visitor was looking at /v/<slug> counts for that
/// vertical, and a click with no vertical context counts everywhere (it came
/// from a profile page or a direct link, not from a leaderboard).
/// Builds the visitor/click fragment with the caller's parameter index for
/// the vertical context. Callers that have no context pass `NULL::text` and
/// bind nothing, which keeps the numbering local to each query instead of
/// depending on a fragment that silently renumbers it.
fn member_stats(ctx: &str) -> String {
    format!(
        r#"
    (SELECT count(DISTINCT sh.session_hash) FROM source_hits sh
       WHERE sh.user_id = u.id
         AND ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
    )::bigint AS visitors,
    (SELECT count(DISTINCT sh.session_hash) FROM source_hits sh
       WHERE sh.user_id = u.id
         AND ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
         AND sh.created_at > now() - interval '7 days'
    )::bigint AS visitors_7d,
    (SELECT count(DISTINCT sh.session_hash) FROM source_hits sh
       WHERE sh.user_id = u.id
         AND sh.created_at >= now() - interval '28 days'
         AND sh.created_at <  now() - interval '7 days'
         AND ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
    )::bigint AS visitors_prev,
    (SELECT count(*) FROM source_hits sh
       WHERE sh.user_id = u.id
         AND ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
    )::bigint AS clicks,
    -- What the ranker reads instead of `clicks` (D41): one hit per session (a fan looking ten times is
    -- one person), each weighted by where it was seen (a click at slot 9 is harder to get than one at
    -- slot 1), so the sum is an audience rather than a tally. NULL position means slot 1 — the least
    -- credit, because nothing proves it was hard to find.
    COALESCE((SELECT sum(1.0 + ln(greatest(coalesce(h.position, 1), 1)::float8))
                FROM (SELECT DISTINCT ON (sh.session_hash) sh.session_hash, sh.position
                        FROM source_hits sh
                       WHERE sh.user_id = u.id
                         AND ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
                       ORDER BY sh.session_hash, sh.created_at) h), 0)::float8 AS attention,
    -- The same measurement with fame on a 90-day half-life (D42): a click six months ago is worth a
    -- sixty-fourth of one today, so an early member stops being permanently first without being erased —
    -- the lifetime total stays in `attention`, and the monument tab reads it.
    COALESCE((SELECT sum((1.0 + ln(greatest(coalesce(h.position, 1), 1)::float8))
                         * power(0.5, extract(epoch FROM (now() - h.created_at)) / 86400.0 / 90.0))
                FROM (SELECT DISTINCT ON (sh.session_hash) sh.session_hash, sh.position, sh.created_at
                        FROM source_hits sh
                       WHERE sh.user_id = u.id
                         AND ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
                       ORDER BY sh.session_hash, sh.created_at) h), 0)::float8 AS recent,
    COALESCE((SELECT array_agg(t.tag_slug ORDER BY t.tag_slug) FROM tags t WHERE t.user_id = u.id),
             '{{}}'::text[]) AS tags,
    COALESCE((SELECT array_agg(v.slug ORDER BY CASE WHEN uv2.role = 'primary' THEN 0 ELSE 1 END, v.sort)
                FROM user_verticals uv2 JOIN verticals v ON v.slug = uv2.vertical_slug
               WHERE uv2.user_id = u.id), '{{}}'::text[]) AS verticals,
    -- How strongly this member's tags are seconded: three endorsements on any of their tags reach
    -- the cap, so a member with many lightly-endorsed tags is not outranked by one very popular one.
    COALESCE((SELECT LEAST(1.0, max(cnt)::float8 / {full})
                FROM (SELECT count(*) AS cnt FROM tags t
                        JOIN tag_endorsements te ON te.tag_slug = t.tag_slug
                       WHERE t.user_id = u.id GROUP BY t.tag_slug) s), 0)::float8 AS endorse
"#,
        full = crate::governance::ENDORSE_FULL
    )
}

/// The same columns as `member_stats`, computed **once for the whole roster** instead of once per row.
///
/// `member_stats` is a correlated subquery block: eight probes per member, which is free at two members
/// and fatal at forty-five thousand (a category page measured 18 s on a 45k catalogue). This fragment
/// pre-aggregates each table by `user_id` and left-joins it, so a list query scans `source_hits`,
/// `tags`, `user_verticals` and `tag_endorsements` once each. It returns the FROM clause; the caller
/// writes the column list, which keeps one copy of the aggregate names.
///
/// `ctx` is the caller's bound parameter for the vertical context (or `NULL::text`): a click seen inside
/// a category counts for that category, and a click with no context counts everywhere (D1's rule,
/// unchanged — only the loop around it moved into SQL).
fn member_stats_from(ctx: &str) -> String {
    format!(
        r#"
      FROM users u
      LEFT JOIN (
          SELECT sh.user_id,
                 count(DISTINCT sh.session_hash)::bigint AS visitors,
                 count(DISTINCT sh.session_hash) FILTER (
                     WHERE sh.created_at > now() - interval '7 days')::bigint AS visitors_7d,
                 count(DISTINCT sh.session_hash) FILTER (
                     WHERE sh.created_at >= now() - interval '28 days'
                       AND sh.created_at <  now() - interval '7 days')::bigint AS visitors_prev,
                 count(*)::bigint AS clicks
            FROM source_hits sh
           WHERE ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
           GROUP BY sh.user_id
      ) st ON st.user_id = u.id
      LEFT JOIN (
          SELECT h.user_id,
                 sum(1.0 + ln(greatest(coalesce(h.position, 1), 1)::float8))::float8 AS attention,
                 sum((1.0 + ln(greatest(coalesce(h.position, 1), 1)::float8))
                     * power(0.5, extract(epoch FROM (now() - h.created_at)) / 86400.0 / 90.0))::float8 AS recent
            FROM (SELECT DISTINCT ON (sh.user_id, sh.session_hash)
                         sh.user_id, sh.session_hash, sh.position, sh.created_at
                    FROM source_hits sh
                   WHERE ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
                   ORDER BY sh.user_id, sh.session_hash, sh.created_at) h
           GROUP BY h.user_id
      ) at ON at.user_id = u.id
      LEFT JOIN (
          SELECT t.user_id, array_agg(t.tag_slug ORDER BY t.tag_slug) AS tags
            FROM tags t GROUP BY t.user_id
      ) tg ON tg.user_id = u.id
      LEFT JOIN (
          SELECT uv.user_id,
                 array_agg(v.slug ORDER BY CASE WHEN uv.role = 'primary' THEN 0 ELSE 1 END, v.sort) AS verticals
            FROM user_verticals uv JOIN verticals v ON v.slug = uv.vertical_slug
           GROUP BY uv.user_id
      ) vv ON vv.user_id = u.id
      LEFT JOIN (
          SELECT x.user_id, LEAST(1.0, max(x.cnt)::float8 / {full})::float8 AS endorse
            FROM (SELECT t2.user_id, count(*) AS cnt
                    FROM tags t2 JOIN tag_endorsements te ON te.tag_slug = t2.tag_slug
                   GROUP BY t2.user_id, t2.tag_slug) x
           GROUP BY x.user_id
      ) en ON en.user_id = u.id
"#,
        full = crate::governance::ENDORSE_FULL
    )
}

/// The column list that goes with `member_stats_from`, in the order `MemberStat` decodes.
const MEMBER_STAT_COLS: &str = "\
      u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, u.trust_level,
      u.bsky_handle, u.nsfw_enabled, u.created_at, u.last_active_at,
      coalesce(st.visitors, 0)::bigint AS visitors,
      coalesce(st.visitors_7d, 0)::bigint AS visitors_7d,
      coalesce(st.visitors_prev, 0)::bigint AS visitors_prev,
      coalesce(st.clicks, 0)::bigint AS clicks,
      coalesce(at.attention, 0)::float8 AS attention,
      coalesce(at.recent, 0)::float8 AS recent,
      coalesce(tg.tags, '{}'::text[]) AS tags,
      coalesce(vv.verticals, '{}'::text[]) AS verticals,
      coalesce(en.endorse, 0)::float8 AS endorse";

// ---------------------------------------------------------------- users

pub async fn user_by_handle(pool: &PgPool, handle: &str) -> Result<Option<User>, AppError> {
    let sql = format!("SELECT {USER_COLS} FROM users WHERE handle = $1");
    Ok(sqlx::query_as::<_, User>(&sql)
        .bind(handle)
        .fetch_optional(pool)
        .await?)
}

pub async fn user_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, AppError> {
    let sql = format!("SELECT {USER_COLS} FROM users WHERE id = $1");
    Ok(sqlx::query_as::<_, User>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn password_hash(pool: &PgPool, handle: &str) -> Result<Option<String>, AppError> {
    let row = sqlx::query("SELECT password_hash FROM users WHERE handle = $1")
        .bind(handle)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("password_hash")))
}

pub struct NewUser<'a> {
    pub handle: &'a str,
    pub display_name: &'a str,
    pub bio: &'a str,
    pub accent: &'a str,
    pub password_hash: &'a str,
    pub nsfw_enabled: bool,
}

pub async fn create_user(pool: &PgPool, new: NewUser<'_>) -> Result<User, AppError> {
    let sql = format!(
        "INSERT INTO users (handle, display_name, bio, accent, password_hash, nsfw_enabled)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {USER_COLS}"
    );
    let res = sqlx::query_as::<_, User>(&sql)
        .bind(new.handle)
        .bind(new.display_name)
        .bind(new.bio)
        .bind(new.accent)
        .bind(new.password_hash)
        .bind(new.nsfw_enabled)
        .fetch_one(pool)
        .await;
    match res {
        Ok(u) => Ok(u),
        Err(sqlx::Error::Database(db)) if db.constraint() == Some("users_handle_key") => Err(
            AppError::Conflict(format!("the handle `{}` is taken", new.handle)),
        ),
        Err(e) => Err(AppError::Db(e)),
    }
}

/// The reader's dial, as stored. `None` means they never chose, which is different from choosing 0
/// — one means "the surface decides", the other means "no lens at all".
pub async fn gravity_of(pool: &PgPool, visitor_hash: &str) -> Result<Option<f64>, AppError> {
    let row: Option<(Option<f64>,)> =
        sqlx::query_as("SELECT gravity FROM visitor_profiles WHERE visitor_hash = $1")
            .bind(visitor_hash)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(g,)| g))
}

/// Store the reader's dial. `None` clears it back to the surface's own ceiling.
pub async fn set_gravity(
    pool: &PgPool,
    visitor_hash: &str,
    value: Option<f64>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO visitor_profiles (visitor_hash, pages_visited, gravity)
         VALUES ($1, 0, $2)
         ON CONFLICT (visitor_hash) DO UPDATE SET gravity = EXCLUDED.gravity",
    )
    .bind(visitor_hash)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn touch_login(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE users SET last_login_at = now(), last_active_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_profile(
    pool: &PgPool,
    id: Uuid,
    patch: &ProfilePatch,
) -> Result<User, AppError> {
    let sql = format!(
        "UPDATE users SET
            display_name = COALESCE($2, display_name),
            bio          = COALESCE($3, bio),
            accent       = COALESCE($4, accent),
            avatar_url   = COALESCE($5, avatar_url),
            nsfw_enabled = COALESCE($6, nsfw_enabled),
            last_active_at = now()
         WHERE id = $1
         RETURNING {USER_COLS}"
    );
    Ok(sqlx::query_as::<_, User>(&sql)
        .bind(id)
        .bind(patch.display_name.as_deref())
        .bind(patch.bio.as_deref())
        .bind(patch.accent.as_deref())
        .bind(patch.avatar_url.as_deref())
        .bind(patch.nsfw_enabled)
        .fetch_one(pool)
        .await?)
}

pub async fn set_trust(pool: &PgPool, id: Uuid, level: i16, hidden: Option<bool>) -> Result<(), AppError> {
    sqlx::query("UPDATE users SET trust_level = $2, hidden = COALESCE($3, hidden) WHERE id = $1")
        .bind(id)
        .bind(level)
        .bind(hidden)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn zero_ranking(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    // The flagged member's click history stops counting: the rows are moved to
    // a null-ish surface and their trust is reset. Deleting them outright would
    // also be defensible; keeping them with a marker preserves the audit trail.
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE source_hits SET surface = 'voided', vertical_context = NULL WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM profile_views WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE users SET trust_level = 0 WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_verticals(
    pool: &PgPool,
    id: Uuid,
    primary: &str,
    secondaries: &[String],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM user_verticals WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, $2, 'primary')")
        .bind(id)
        .bind(primary)
        .execute(&mut *tx)
        .await?;
    for slug in secondaries.iter().filter(|s| s.as_str() != primary) {
        sqlx::query(
            "INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, $2, 'secondary')
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(slug)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn set_tags(pool: &PgPool, id: Uuid, tags: &[String]) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM tags WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for tag in tags.iter().take(10) {
        sqlx::query("INSERT INTO tags (user_id, tag_slug) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(tag)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn tags_of(pool: &PgPool, id: Uuid) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT tag_slug FROM tags WHERE user_id = $1 ORDER BY tag_slug")
        .bind(id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn verticals_of(pool: &PgPool, id: Uuid) -> Result<Vec<(String, String)>, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT vertical_slug, role FROM user_verticals WHERE user_id = $1 ORDER BY role, vertical_slug",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn set_bsky_claim(
    pool: &PgPool,
    id: Uuid,
    did: &str,
    handle: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE users SET bsky_did = $2, bsky_handle = $3, bsky_verified_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(did)
    .bind(handle)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn rotate_verify_token(pool: &PgPool, id: Uuid) -> Result<String, AppError> {
    let row = sqlx::query(
        "UPDATE users SET verify_token = encode(gen_random_bytes(12), 'hex') WHERE id = $1
         RETURNING verify_token",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<String, _>("verify_token"))
}

// ---------------------------------------------------------------- fixtures

/// A smoke test once wrote to the live database: hidden members with `example.com` links, still counted in the
/// verticals they joined. The pattern is deliberately narrow — **hidden and test-shaped** — so a real member who
/// hid their profile is never swept up, and a fixture that forgets to hide itself is left for a human to see.
const FIXTURE_HANDLE: &str = "^(test|smoke|e2e|dummy|fixture)";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FixtureRow {
    pub handle: String,
    pub links: i64,
}

/// What the cleanup would remove, so a deletion can be read before it is run.
pub async fn fixtures(pool: &PgPool) -> Result<Vec<FixtureRow>, AppError> {
    let sql = format!(
        "SELECT u.handle, count(l.id)::bigint AS links
           FROM users u LEFT JOIN links l ON l.user_id = u.id
          WHERE u.hidden AND u.handle ~ '{FIXTURE_HANDLE}'
          GROUP BY u.handle ORDER BY u.handle"
    );
    Ok(sqlx::query_as::<_, FixtureRow>(&sql).fetch_all(pool).await?)
}

/// Delete them. Links cascade from `users`, and the counts are read *before* the delete so the caller can report
/// what went. Idempotent: a second run finds nothing and says so.
pub async fn delete_fixtures(pool: &PgPool) -> Result<(i64, i64), AppError> {
    let links: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM links l JOIN users u ON u.id = l.user_id
          WHERE u.hidden AND u.handle ~ '{FIXTURE_HANDLE}'"
    ))
    .fetch_one(pool)
    .await?;
    let users: i64 = sqlx::query_scalar(&format!(
        "WITH gone AS (DELETE FROM users WHERE hidden AND handle ~ '{FIXTURE_HANDLE}' RETURNING 1)
         SELECT count(*) FROM gone"
    ))
    .fetch_one(pool)
    .await?;
    Ok((users, links))
}

// ---------------------------------------------------------------- verticals

pub async fn verticals(pool: &PgPool, include_nsfw: bool) -> Result<Vec<VerticalRow>, AppError> {
    let sql = format!(
        "SELECT {VERTICAL_COLS} FROM verticals WHERE ($1 OR nsfw = false) ORDER BY sort, slug"
    );
    Ok(sqlx::query_as::<_, VerticalRow>(&sql)
        .bind(include_nsfw)
        .fetch_all(pool)
        .await?)
}

/// The verticals that have somebody in them. This is what the navigation and the front page may offer, because a
/// vertical nobody is in is a page that exists and not yet a destination. The API keeps the full taxonomy
/// (`verticals`), since publishing a count of zero is honest; *offering* a door with nobody behind it is not.
pub async fn populated_verticals(pool: &PgPool, include_nsfw: bool) -> Result<Vec<VerticalRow>, AppError> {
    let sql = format!(
        "SELECT {VERTICAL_COLS} FROM verticals v
          WHERE ($1 OR v.nsfw = false)
            AND EXISTS (SELECT 1 FROM user_verticals uv JOIN users u ON u.id = uv.user_id
                         WHERE uv.vertical_slug = v.slug AND NOT u.hidden)
          ORDER BY v.sort, v.slug"
    );
    Ok(sqlx::query_as::<_, VerticalRow>(&sql)
        .bind(include_nsfw)
        .fetch_all(pool)
        .await?)
}

pub async fn vertical_by_slug(pool: &PgPool, slug: &str) -> Result<Option<VerticalRow>, AppError> {
    let sql = format!("SELECT {VERTICAL_COLS} FROM verticals WHERE slug = $1");
    Ok(sqlx::query_as::<_, VerticalRow>(&sql)
        .bind(slug)
        .fetch_optional(pool)
        .await?)
}

pub async fn vertical_counts(
    pool: &PgPool,
    include_nsfw: bool,
) -> Result<Vec<VerticalCount>, AppError> {
    let rows = sqlx::query_as::<_, VerticalCount>(
        "SELECT v.slug, v.label, v.description, v.icon, v.nsfw, v.sort, v.min_members,
                (SELECT count(*) FROM user_verticals uv
                   JOIN users u ON u.id = uv.user_id
                  WHERE uv.vertical_slug = v.slug AND u.hidden = false)::bigint AS members
           FROM verticals v
          WHERE ($1 OR v.nsfw = false)
          ORDER BY v.sort, v.slug",
    )
    .bind(include_nsfw)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Members of one vertical, with stats measured in that vertical's context.
pub async fn vertical_members(pool: &PgPool, slug: &str) -> Result<Vec<MemberStat>, AppError> {
    let sql = format!(
        "SELECT {MEMBER_STAT_COLS}, uv.role
           {from}
           JOIN user_verticals uv ON uv.user_id = u.id AND uv.vertical_slug = $1
          WHERE u.hidden = false",
        from = member_stats_from("$1")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind(slug)
        .fetch_all(pool)
        .await?)
}

/// The ids a vertical surface may decorate. This query never touches tags, verticals or endorsements:
/// it only joins the memberships to the small `source_hits` aggregate, so the sort is over simple
/// rows. The caller decorates this list and nothing else.
pub async fn vertical_candidate_ids(
    pool: &PgPool,
    slug: &str,
    limit: i64,
) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT u.id
           FROM users u
           JOIN user_verticals uv ON uv.user_id = u.id AND uv.vertical_slug = $1
           LEFT JOIN (
               SELECT sh.user_id,
                      count(DISTINCT sh.session_hash)::bigint AS visitors,
                      count(*)::bigint AS clicks
                 FROM source_hits sh
                WHERE sh.vertical_context = $1 OR sh.vertical_context IS NULL
                GROUP BY sh.user_id
           ) st ON st.user_id = u.id
          WHERE u.hidden = false
          ORDER BY coalesce(st.visitors, 0) DESC, coalesce(st.clicks, 0) DESC, u.created_at
          LIMIT $2",
    )
    .bind(slug)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn vertical_members_limited(
    pool: &PgPool,
    slug: &str,
    limit: i64,
) -> Result<Vec<MemberStat>, AppError> {
    let ids = vertical_candidate_ids(pool, slug, limit).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {MEMBER_STAT_COLS}, uv.role
           {from}
           JOIN user_verticals uv ON uv.user_id = u.id AND uv.vertical_slug = $1
          WHERE u.hidden = false AND u.id = ANY($2)
          ORDER BY array_position($2::uuid[], u.id)",
        from = member_stats_from("$1")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind(slug)
        .bind(&ids)
        .fetch_all(pool)
        .await?)
}

/// The pill count needs a number, not a roster. One indexed count instead of 48k decoded rows.
pub async fn vertical_member_count(pool: &PgPool, slug: &str) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT count(*)::bigint FROM user_verticals uv
           JOIN users u ON u.id = uv.user_id
          WHERE uv.vertical_slug = $1 AND u.hidden = false",
    )
    .bind(slug)
    .fetch_one(pool)
    .await?)
}

/// What a surface has shown in the last `hours`, per member (D45). One query, one shape: the ceiling needs
/// both halves of the ratio, so they are read together or not at all.
pub async fn impressions_window(
    pool: &PgPool,
    surface: &str,
    hours: i32,
) -> Result<std::collections::HashMap<Uuid, i64>, AppError> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT user_id, sum(count)::bigint FROM impressions
          WHERE surface = $1 AND bucket >= date_trunc('hour', now() - make_interval(hours => $2))
          GROUP BY user_id",
    )
    .bind(surface)
    .bind(hours)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Record what a render actually showed: one increment per member per hour, never per card and never per
/// visitor. An impression is "this card was on a page someone loaded", which is the whole measurement.
pub async fn record_impressions(pool: &PgPool, surface: &str, ids: &[Uuid]) -> Result<(), AppError> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO impressions (surface, user_id, bucket, count)
         SELECT $1, unnest($2::uuid[]), date_trunc('hour', now()), 1
         ON CONFLICT (surface, user_id, bucket) DO UPDATE SET count = impressions.count + 1",
    )
    .bind(surface)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every listed member, with stats over their whole click history.
pub async fn all_members(pool: &PgPool, include_nsfw: bool) -> Result<Vec<MemberStat>, AppError> {
    let sql = format!(
        "SELECT {MEMBER_STAT_COLS}, 'primary'::text AS role
           {from}
          WHERE u.hidden = false AND ($1 OR {SFW_PRIMARY_ONLY})
          ORDER BY u.created_at DESC",
        from = member_stats_from("NULL::text")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind(include_nsfw)
        .fetch_all(pool)
        .await?)
}

pub async fn member_stat(pool: &PgPool, id: Uuid, ctx: Option<&str>) -> Result<Option<MemberStat>, AppError> {
    let sql = format!(
        "SELECT u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, u.trust_level,
                u.bsky_handle, u.nsfw_enabled, u.created_at, u.last_active_at,
                COALESCE((SELECT role FROM user_verticals uv WHERE uv.user_id = u.id AND uv.role = 'primary'), 'primary')::text AS role,
                {}
           FROM users u WHERE u.id = $2",
        member_stats("$1")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind(ctx)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn create_vertical(
    pool: &PgPool,
    slug: &str,
    label: &str,
    description: &str,
    icon: &str,
    nsfw: bool,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO verticals (slug, label, description, icon, nsfw, sort) VALUES ($1,$2,$3,$4,$5,500)")
        .bind(slug)
        .bind(label)
        .bind(description)
        .bind(icon)
        .bind(nsfw)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn owner_id(pool: &PgPool) -> Result<Option<Uuid>, AppError> {
    let row = sqlx::query("SELECT id FROM users WHERE is_owner = true ORDER BY created_at LIMIT 1")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

// ---------------------------------------------------------------- links

pub async fn links_for(
    pool: &PgPool,
    user_id: Uuid,
    ctx: Option<&str>,
    allow_nsfw: bool,
) -> Result<Vec<LinkRow>, AppError> {
    let sql = format!(
        "SELECT {LINK_COLS_L} FROM links l
           LEFT JOIN verticals v ON v.slug = l.vertical_slug
          WHERE l.user_id = $1
            AND l.hidden = false
            AND (l.vertical_slug IS NULL OR l.vertical_slug = $2)
            AND ($3 OR v.nsfw IS NOT TRUE)
          ORDER BY l.position, l.created_at"
    );
    Ok(sqlx::query_as::<_, LinkRow>(&sql)
        .bind(user_id)
        .bind(ctx)
        .bind(allow_nsfw)
        .fetch_all(pool)
        .await?)
}

pub async fn all_links(pool: &PgPool, user_id: Uuid) -> Result<Vec<LinkRow>, AppError> {
    let sql = format!(
        "SELECT {LINK_COLS} FROM links WHERE user_id = $1 ORDER BY position, created_at"
    );
    Ok(sqlx::query_as::<_, LinkRow>(&sql)
        .bind(user_id)
        .fetch_all(pool)
        .await?)
}

pub async fn link_by_id(pool: &PgPool, id: Uuid) -> Result<Option<LinkRow>, AppError> {
    let sql = format!("SELECT {LINK_COLS} FROM links WHERE id = $1");
    Ok(sqlx::query_as::<_, LinkRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn count_links(pool: &PgPool, user_id: Uuid) -> Result<i64, AppError> {
    let row = sqlx::query("SELECT count(*)::bigint AS n FROM links WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("n"))
}

pub async fn create_link(
    pool: &PgPool,
    user_id: Uuid,
    url: &str,
    label: &str,
    icon: &str,
    vertical_slug: Option<&str>,
) -> Result<LinkRow, AppError> {
    let sql = format!(
        "INSERT INTO links (user_id, vertical_slug, url, label, icon, position)
         VALUES ($1, $2, $3, $4, $5,
                 COALESCE((SELECT max(position) + 1 FROM links WHERE user_id = $1), 0))
         RETURNING {LINK_COLS}"
    );
    Ok(sqlx::query_as::<_, LinkRow>(&sql)
        .bind(user_id)
        .bind(vertical_slug)
        .bind(url)
        .bind(label)
        .bind(icon)
        .fetch_one(pool)
        .await?)
}

pub async fn update_link(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    patch: &LinkPatch,
) -> Result<Option<LinkRow>, AppError> {
    // A nullable field that can be SET TO NULL needs a two-valued encoding:
    // outer None = leave alone, Some(None) = clear.
    let (set_vertical, vertical_value) = match &patch.vertical_slug {
        None => (false, None),
        Some(v) => (true, v.clone()),
    };
    let sql = format!(
        "UPDATE links SET
            url = COALESCE($3, url),
            label = COALESCE($4, label),
            icon = COALESCE($5, icon),
            hidden = COALESCE($6, hidden),
            vertical_slug = CASE WHEN $7 THEN $8::text ELSE vertical_slug END
         WHERE id = $2 AND user_id = $1
         RETURNING {LINK_COLS}"
    );
    Ok(sqlx::query_as::<_, LinkRow>(&sql)
        .bind(user_id)
        .bind(id)
        .bind(patch.url.as_deref())
        .bind(patch.label.as_deref())
        .bind(patch.icon.as_deref())
        .bind(patch.hidden)
        .bind(set_vertical)
        .bind(vertical_value)
        .fetch_optional(pool)
        .await?)
}

pub async fn delete_link(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, AppError> {
    let res = sqlx::query("DELETE FROM links WHERE id = $2 AND user_id = $1")
        .bind(user_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() == 1)
}

pub async fn reorder_links(pool: &PgPool, user_id: Uuid, order: &[Uuid]) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    for (i, id) in order.iter().enumerate() {
        sqlx::query("UPDATE links SET position = $3 WHERE id = $2 AND user_id = $1")
            .bind(user_id)
            .bind(id)
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------- tracking

pub async fn record_click(
    pool: &PgPool,
    user_id: Uuid,
    link_id: Option<Uuid>,
    surface: &str,
    ctx: Option<&str>,
    position: Option<i32>,
    session_hash: &str,
    referrer_host: Option<&str>,
    viewer_hash: Option<&str>,
    fingerprint: Option<&str>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO source_hits (user_id, link_id, surface, vertical_context, position, session_hash, referrer_host, viewer_hash, fingerprint)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(user_id)
    .bind(link_id)
    .bind(surface)
    .bind(ctx)
    .bind(position)
    .bind(session_hash)
    .bind(referrer_host)
    .bind(viewer_hash)
    .bind(fingerprint)
    .execute(&mut *tx)
    .await?;
    if let Some(link_id) = link_id {
        sqlx::query("UPDATE links SET click_count = click_count + 1 WHERE id = $1")
            .bind(link_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// One view per visitor per day. `ON CONFLICT DO NOTHING` makes a refresh
/// free and keeps a crawler's walk from inflating the denominator.
pub async fn record_view(
    pool: &PgPool,
    user_id: Uuid,
    ctx: Option<&str>,
    session_hash: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO profile_views (user_id, vertical_context, session_hash)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, session_hash, view_date) DO NOTHING",
    )
    .bind(user_id)
    .bind(ctx)
    .bind(session_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// Sessions per client fingerprint, newest week. One person has a handful; a farm has hundreds,
/// because rotating a cookie is free and changing a user agent is not.
pub async fn fingerprint_spread(pool: &PgPool, days: i32) -> Result<Vec<(String, i64, i64)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT fingerprint, count(DISTINCT session_hash)::bigint, count(*)::bigint
           FROM source_hits
          WHERE fingerprint IS NOT NULL AND created_at > now() - ($1::int * interval '1 day')
          GROUP BY fingerprint
         HAVING count(DISTINCT session_hash) > 20
          ORDER BY 2 DESC LIMIT 20",
    )
    .bind(days)
    .fetch_all(pool)
    .await?)
}

pub async fn analytics(
    pool: &PgPool,
    user_id: Uuid,
    days: i32,
) -> Result<(Vec<DayPoint>, Vec<LinkStat>, Vec<Bucket>, Vec<Bucket>, i64, i64), AppError> {
    let views = sqlx::query_as::<_, (NaiveDate, i64)>(
        "SELECT view_date AS day, count(*)::bigint AS n
           FROM profile_views
          WHERE user_id = $1 AND view_date > current_date - $2::int
          GROUP BY 1",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await?;

    let clicks = sqlx::query_as::<_, (NaiveDate, i64)>(
        "SELECT created_at::date AS day, count(*)::bigint AS n
           FROM source_hits
          WHERE user_id = $1 AND created_at > now() - ($2::int * interval '1 day')
            AND surface <> 'voided'
          GROUP BY 1",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await?;

    let mut series: BTreeMap<NaiveDate, DayPoint> = BTreeMap::new();
    for (day, n) in views {
        series
            .entry(day)
            .or_insert(DayPoint { day, views: 0, clicks: 0 })
            .views = n;
    }
    for (day, n) in clicks {
        series
            .entry(day)
            .or_insert(DayPoint { day, views: 0, clicks: 0 })
            .clicks = n;
    }

    let top_links = sqlx::query_as::<_, LinkStat>(
        "SELECT l.id, l.label, l.url,
                count(sh.id)::bigint AS clicks,
                count(DISTINCT sh.session_hash)::bigint AS visitors
           FROM links l
           LEFT JOIN source_hits sh
             ON sh.link_id = l.id
            AND sh.created_at > now() - ($2::int * interval '1 day')
            AND sh.surface <> 'voided'
          WHERE l.user_id = $1
          GROUP BY l.id, l.label, l.url, l.position
          ORDER BY clicks DESC, l.position
          LIMIT 12",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await?;

    let by_surface = sqlx::query_as::<_, Bucket>(
        "SELECT surface AS key, count(*)::bigint AS clicks,
                count(DISTINCT session_hash)::bigint AS visitors
           FROM source_hits
          WHERE user_id = $1 AND created_at > now() - ($2::int * interval '1 day')
          GROUP BY 1 ORDER BY clicks DESC",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await?;

    let by_vertical = sqlx::query_as::<_, Bucket>(
        "SELECT COALESCE(vertical_context, 'direct') AS key, count(*)::bigint AS clicks,
                count(DISTINCT session_hash)::bigint AS visitors
           FROM source_hits
          WHERE user_id = $1 AND created_at > now() - ($2::int * interval '1 day')
          GROUP BY 1 ORDER BY clicks DESC",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await?;

    let totals = sqlx::query(
        "SELECT
           (SELECT count(DISTINCT session_hash)::bigint FROM source_hits WHERE user_id = $1) AS visitors,
           (SELECT count(*)::bigint FROM source_hits WHERE user_id = $1) AS clicks",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok((
        series.into_values().collect(),
        top_links,
        by_surface,
        by_vertical,
        totals.get::<i64, _>("visitors"),
        totals.get::<i64, _>("clicks"),
    ))
}

// ---------------------------------------------------------------- search

pub async fn search(
    pool: &PgPool,
    q: &str,
    include_nsfw: bool,
) -> Result<(Vec<MemberStat>, Vec<(String, i64)>), AppError> {
    let like = format!("%{}%", q.replace('%', "").replace('_', ""));
    let sql = format!(
        "SELECT u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, u.trust_level,
                u.bsky_handle, u.nsfw_enabled, u.created_at, u.last_active_at, 'primary'::text AS role, {}
           FROM users u
          WHERE u.hidden = false
            AND ($2 OR {SFW_PRIMARY_ONLY})
            AND (u.handle ILIKE $1 OR u.display_name ILIKE $1 OR u.bio ILIKE $1
                 OR EXISTS (SELECT 1 FROM tags t WHERE t.user_id = u.id AND t.tag_slug ILIKE $1)
                 OR EXISTS (SELECT 1 FROM links l WHERE l.user_id = u.id AND l.url ILIKE $1))
          ORDER BY u.handle
          LIMIT 60",
        member_stats("NULL::text")
    );
    let members = sqlx::query_as::<_, MemberStat>(&sql)
        .bind(&like)
        .bind(include_nsfw)
        .fetch_all(pool)
        .await?;

    let tags = sqlx::query_as::<_, (String, i64)>(
        "SELECT t.tag_slug, count(*)::bigint AS n
           FROM tags t JOIN users u ON u.id = t.user_id
          WHERE t.tag_slug ILIKE $1 AND u.hidden = false
          GROUP BY 1 ORDER BY n DESC, 1 LIMIT 12",
    )
    .bind(&like)
    .fetch_all(pool)
    .await?;

    Ok((members, tags))
}

// ---------------------------------------------------------------- flags

pub async fn create_flag(
    pool: &PgPool,
    flagger: Option<Uuid>,
    target: Uuid,
    vertical: Option<&str>,
    reason: &str,
    detail: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO flags (flagger_id, target_user_id, vertical_slug, reason, detail)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(flagger)
    .bind(target)
    .bind(vertical)
    .bind(reason)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn flags(pool: &PgPool, open_only: bool) -> Result<Vec<FlagRow>, AppError> {
    let rows = sqlx::query_as::<_, FlagRow>(
        "SELECT f.id, f.target_user_id, u.handle AS target_handle, f.vertical_slug,
                f.reason, f.detail, f.status, f.created_at
           FROM flags f JOIN users u ON u.id = f.target_user_id
          WHERE (NOT $1 OR f.status = 'open')
          ORDER BY f.created_at DESC LIMIT 200",
    )
    .bind(open_only)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn resolve_flag(
    pool: &PgPool,
    id: Uuid,
    resolver: Uuid,
    status: &str,
    note: &str,
) -> Result<Option<(Uuid, String)>, AppError> {
    let row = sqlx::query(
        "UPDATE flags SET status = $2, note = $3, resolver_id = $4, resolved_at = now()
          WHERE id = $1 AND status = 'open'
          RETURNING target_user_id, reason",
    )
    .bind(id)
    .bind(status)
    .bind(note)
    .bind(resolver)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.get::<Uuid, _>("target_user_id"), r.get::<String, _>("reason"))))
}

// ---------------------------------------------------------------- sessions

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    token: &str,
    ttl_days: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at)
         VALUES ($1, $2, now() + ($3::bigint * interval '1 day'))",
    )
    .bind(token)
    .bind(user_id)
    .bind(ttl_days)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn user_by_session(pool: &PgPool, token: &str) -> Result<Option<User>, AppError> {
    let sql = format!(
        "SELECT {USER_COLS_U} FROM users u
           JOIN sessions s ON s.user_id = u.id
          WHERE s.id = $1 AND s.expires_at > now()"
    );
    Ok(sqlx::query_as::<_, User>(&sql)
        .bind(token)
        .fetch_optional(pool)
        .await?)
}

pub async fn delete_session(pool: &PgPool, token: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------- bot

pub async fn bot_recent_subjects(pool: &PgPool, kind: &str, days: i32) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT subject FROM bot_posts
          WHERE kind = $1 AND created_at > now() - ($2::int * interval '1 day')",
    )
    .bind(kind)
    .bind(days)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn record_bot_post(
    pool: &PgPool,
    kind: &str,
    subject: &str,
    uri: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO bot_posts (kind, subject, uri) VALUES ($1, $2, $3)")
        .bind(kind)
        .bind(subject)
        .bind(uri)
        .execute(pool)
        .await?;
    Ok(())
}

/// Promote TL0 members that have proven they exist: a link, a few days, and
/// five distinct visitors. Runs from `orbital bot trust` on a timer.
pub async fn promote_trust(pool: &PgPool) -> Result<u64, AppError> {
    let res = sqlx::query(
        "UPDATE users u SET trust_level = 1
          WHERE u.trust_level = 0
            AND u.created_at < now() - interval '2 days'
            AND (SELECT count(*) FROM links l WHERE l.user_id = u.id) >= 2
            AND (SELECT count(DISTINCT session_hash) FROM source_hits sh WHERE sh.user_id = u.id) >= 5",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn prune_old_events(pool: &PgPool) -> Result<u64, AppError> {
    let res = sqlx::query("DELETE FROM source_hits WHERE created_at < now() - interval '180 days'")
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

// ---------------------------------------------------------------- home + sitemap

/// (listed members, outbound clicks) for the front page's honesty line.
pub async fn totals(pool: &PgPool, include_nsfw: bool) -> Result<(i64, i64), AppError> {
    let sql = format!(
        "SELECT
           (SELECT count(*)::bigint FROM users u WHERE u.hidden = false
              AND ($1 OR {SFW_PRIMARY_ONLY})) AS members,
           (SELECT count(*)::bigint FROM source_hits sh
              JOIN users u ON u.id = sh.user_id
             WHERE u.hidden = false AND ($1 OR {SFW_PRIMARY_ONLY})
               AND sh.surface <> 'voided') AS clicks"
    );
    let row = sqlx::query(&sql)
    .bind(include_nsfw)
    .fetch_one(pool)
    .await?;
    Ok((row.get::<i64, _>("members"), row.get::<i64, _>("clicks")))
}

pub async fn sitemap_entries(
    pool: &PgPool,
    include_nsfw: bool,
) -> Result<Vec<(String, chrono::DateTime<chrono::Utc>)>, AppError> {
    let sql = format!(
        "SELECT handle, GREATEST(last_active_at, created_at) AS updated
           FROM users u
          WHERE hidden = false AND ($1 OR {SFW_PRIMARY_ONLY})
          ORDER BY created_at DESC"
    );
    let rows = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>)>(&sql)
    .bind(include_nsfw)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Ranks for the member's own verticals, computed with the same functions the
/// public leaderboards use — a dashboard that ranks differently from the page it
/// describes is a lie with a chart on top.
pub async fn standing(
    pool: &PgPool,
    user_id: Uuid,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<crate::models::Standing>, AppError> {
    let pairs = verticals_of(pool, user_id).await?;
    let catalogue = verticals(pool, true).await?;
    let mut out = Vec::new();
    for (slug, role) in pairs {
        let Some(v) = catalogue.iter().find(|v| v.slug == slug) else {
            continue;
        };
        let members: Vec<_> = vertical_members(pool, &slug)
            .await?
            .into_iter()
            .filter(|m| if v.nsfw { m.nsfw_enabled } else { m.verticals.first().map(|p| p != "nsfw").unwrap_or(true) })
            .collect();
        let total = members.len() as i64;
        let top = crate::ranking::rank_top(members.clone(), &[], 1.0, crate::attention::Config::raw(), crate::ranking::Window::Lifetime);
        let trending = crate::ranking::rank_trending(members.clone(), now, &[], 1.0);
        let me = members.iter().find(|m| m.id == user_id);
        out.push(crate::models::Standing {
            label: v.label.clone(),
            role: role.clone(),
            vertical: slug,
            members: total,
            top_rank: top.iter().position(|m| m.id == user_id).map(|i| i as i64 + 1).unwrap_or(0),
            trending_rank: trending
                .iter()
                .position(|m| m.id == user_id)
                .map(|i| i as i64 + 1)
                .unwrap_or(0),
            clicks_7d: me.map(|m| m.clicks).unwrap_or(0),
            visitors_7d: me.map(|m| m.visitors_7d).unwrap_or(0),
        });
    }
    Ok(out)
}

pub async fn member_count(pool: &PgPool, include_nsfw: bool) -> Result<i64, AppError> {
    let sql = format!(
        "SELECT count(*)::bigint AS n FROM users u WHERE hidden = false AND ($1 OR {SFW_PRIMARY_ONLY})"
    );
    let row = sqlx::query(&sql)
    .bind(include_nsfw)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("n"))
}

/// Links the checker has not looked at most recently. Oldest-checked first, so a run that is
/// interrupted still makes progress across the directory instead of re-checking the first page.
pub async fn links_to_check(pool: &PgPool, limit: i64) -> Result<Vec<(Uuid, String)>, AppError> {
    Ok(sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, url FROM links
          WHERE hidden = false
          ORDER BY last_checked NULLS FIRST, position, created_at
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Record one check. A success resets the counter, so a site that comes back stops being marked
/// without anyone touching the row.
pub async fn record_link_health(
    pool: &PgPool,
    link_id: Uuid,
    status: Option<i32>,
    ok: bool,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE links
            SET last_status = $2,
                last_checked = now(),
                fail_count = CASE WHEN $3 THEN 0 ELSE fail_count + 1 END
          WHERE id = $1",
    )
    .bind(link_id)
    .bind(status)
    .bind(ok)
    .execute(pool)
    .await?;
    Ok(())
}

/// Which networks send the clicks. `direct` is the null bucket: a visitor who pasted the URL or
/// arrived from an app that strips Referer.
pub async fn referrer_breakdown(
    pool: &PgPool,
    user_id: Uuid,
    days: i32,
) -> Result<Vec<Bucket>, AppError> {
    Ok(sqlx::query_as::<_, Bucket>(
        "SELECT coalesce(referrer_host, 'direct') AS key,
                count(*)::bigint AS clicks,
                count(DISTINCT session_hash)::bigint AS visitors
           FROM source_hits
          WHERE user_id = $1
            AND created_at > now() - ($2::int * interval '1 day')
            AND surface <> 'voided'
          GROUP BY 1
          ORDER BY clicks DESC
          LIMIT 12",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await?)
}

// ------------------------------------------------------------ preferences

/// A viewer's weights: explicit entries first, then passive ones decayed by age.
///
/// Explicit wins on collision rather than adding: a slider the person moved is a statement, and a
/// click they made three weeks ago must not outvote it.
pub async fn prefs(
    pool: &PgPool,
    owner_kind: &str,
    owner_id: &str,
) -> Result<Vec<(String, f64)>, AppError> {
    // Casts are not decoration: `weight` is a real (float4) and EXTRACT returns numeric, and sqlx
    // refuses to decode either into f64 at runtime. The failure mode is a ColumnDecode that a
    // caller is tempted to swallow, which turns every viewer into "no preferences".
    let rows: Vec<(String, f64, String, f64)> = sqlx::query_as(
        "SELECT tag_slug, weight::float8, source,
                (EXTRACT(EPOCH FROM (now() - updated_at)) / 86400.0)::float8
           FROM preference_weights
          WHERE owner_kind = $1 AND owner_id = $2",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    let mut out: std::collections::HashMap<String, (f64, bool)> = std::collections::HashMap::new();
    for (tag, weight, source, age_days) in rows {
        let explicit = source == "explicit";
        let w = if explicit {
            weight
        } else {
            crate::prefs::decayed(weight, age_days)
        };
        match out.get(&tag) {
            Some((_, true)) if !explicit => {}
            _ => {
                out.insert(tag, (w, explicit));
            }
        }
    }
    let mut weights: Vec<(String, f64)> = out
        .into_iter()
        .map(|(tag, (w, _))| (tag, w))
        .filter(|(_, w)| *w > 0.001)
        .collect();
    weights.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    Ok(weights)
}

/// Does this owner have any weight they set themselves?
///
/// `prefs()` returns explicit and passive weights merged, so "the viewer has weights" is not the
/// same question as "the viewer tuned something" — and only the second one lets personalisation
/// start before the warm-up. Conflating them made a single page view look like an explicit choice.
pub async fn has_explicit_prefs(
    pool: &PgPool,
    owner_kind: &str,
    owner_id: &str,
) -> Result<bool, AppError> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM preference_weights
          WHERE owner_kind = $1 AND owner_id = $2 AND source = 'explicit' LIMIT 1",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// Replace a viewer's explicit weights. An empty list clears them; weight 0 deletes one entry.
pub async fn set_explicit_prefs(
    pool: &PgPool,
    owner_kind: &str,
    owner_id: &str,
    entries: &[(String, f64)],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM preference_weights WHERE owner_kind = $1 AND owner_id = $2")
        .bind(owner_kind)
        .bind(owner_id)
        .execute(&mut *tx)
        .await?;
    for (tag, weight) in entries {
        if *weight <= 0.0 {
            continue;
        }
        sqlx::query(
            "INSERT INTO preference_weights (owner_kind, owner_id, tag_slug, weight, source, updated_at)
             VALUES ($1, $2, $3, $4, 'explicit', now())
             ON CONFLICT (owner_kind, owner_id, tag_slug)
             DO UPDATE SET weight = EXCLUDED.weight, source = 'explicit', updated_at = now()",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .bind(tag)
        .bind(*weight as f32)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Add passive signal, capped at 1.0, without touching tags the viewer set explicitly.
pub async fn accumulate_passive(
    pool: &PgPool,
    visitor_hash: &str,
    signal: &[(String, f64)],
) -> Result<(), AppError> {
    for (tag, delta) in signal {
        sqlx::query(
            "INSERT INTO preference_weights (owner_kind, owner_id, tag_slug, weight, source, updated_at)
             VALUES ('visitor', $1, $2, LEAST($3::real, 1.0), 'passive', now())
             ON CONFLICT (owner_kind, owner_id, tag_slug)
             DO UPDATE SET weight = LEAST(preference_weights.weight + EXCLUDED.weight, 1.0),
                           updated_at = now()
              WHERE preference_weights.source = 'passive'",
        )
        .bind(visitor_hash)
        .bind(tag)
        .bind(*delta as f32)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Count a page view. Returns the new total, which is what decides whether personalisation applies.
pub async fn touch_visitor(pool: &PgPool, visitor_hash: &str) -> Result<i64, AppError> {
    // `pages_visited` is an int4 in the schema; decoding it as i64 fails at runtime with a
    // ColumnDecode, which a caller that ignores errors turns into "no preferences, ever".
    let row: (i32,) = sqlx::query_as(
        "INSERT INTO visitor_profiles (visitor_hash, pages_visited) VALUES ($1, 1)
         ON CONFLICT (visitor_hash)
         DO UPDATE SET pages_visited = visitor_profiles.pages_visited + 1, last_seen_at = now()
         RETURNING pages_visited",
    )
    .bind(visitor_hash)
    .fetch_one(pool)
    .await?;
    Ok(row.0 as i64)
}

pub async fn visitor_pages(pool: &PgPool, visitor_hash: &str) -> Result<i64, AppError> {
    let row: Option<(i32,)> =
        sqlx::query_as("SELECT pages_visited FROM visitor_profiles WHERE visitor_hash = $1")
            .bind(visitor_hash)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0 as i64).unwrap_or(0))
}

/// What the people who clicked this member's links care about, strongest first.
///
/// Aggregated only: the query never returns a visitor id, and the mean is bucketed by the caller.
pub async fn audience_tags(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<(String, f64, i64)>, AppError> {
    Ok(sqlx::query_as(
        "WITH audience AS (
             SELECT DISTINCT viewer_hash FROM source_hits
              WHERE user_id = $1 AND viewer_hash IS NOT NULL AND surface <> 'voided'
         )
         SELECT pw.tag_slug, avg(pw.weight)::float8, count(*)::bigint
           FROM preference_weights pw
           JOIN audience a ON a.viewer_hash = pw.owner_id
          WHERE pw.owner_kind = 'visitor'
          GROUP BY 1
          ORDER BY 2 DESC, 3 DESC, 1
          LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Tags of the hub owner: the gravitational-pull term in the sidebar, now a light hand.
pub async fn hub_tags(pool: &PgPool) -> Result<Vec<String>, AppError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE is_owner = true ORDER BY created_at LIMIT 1")
            .fetch_optional(pool)
            .await?;
    match row {
        Some((id,)) => tags_of(pool, id).await,
        None => Ok(Vec::new()),
    }
}

/// The tags people actually carry, most used first: the tuner's suggestion list.
pub async fn popular_tags(pool: &PgPool, limit: i64) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT t.tag_slug FROM tags t JOIN users u ON u.id = t.user_id
          WHERE u.hidden = false
          GROUP BY 1 ORDER BY count(*) DESC, 1 LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Move an anonymous visitor's weights onto a new account, then drop the anonymous rows.
///
/// Called at registration: whatever the person told the tuner before signing up is what they meant
/// with an account, and a lens that evaporates on signup teaches them not to tune anything.
pub async fn adopt_visitor_prefs(
    pool: &PgPool,
    visitor_hash: &str,
    user_id: Uuid,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO preference_weights AS pw (owner_kind, owner_id, tag_slug, weight, source, updated_at)
         SELECT 'user', $1, tag_slug, weight, source, updated_at
           FROM preference_weights
          WHERE owner_kind = 'visitor' AND owner_id = $2
         ON CONFLICT (owner_kind, owner_id, tag_slug)
         DO UPDATE SET weight = EXCLUDED.weight, source = EXCLUDED.source, updated_at = EXCLUDED.updated_at",
    )
    .bind(user_id.to_string())
    .bind(visitor_hash)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM preference_weights WHERE owner_kind = 'visitor' AND owner_id = $1")
        .bind(visitor_hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Forget visitors nobody has heard from in `days`, and the passive weights that only they could
/// explain. Returns (visitors, weights) deleted.
///
/// Explicit weights are deleted with them: they belong to a hash, and a hash whose visitor is gone
/// is not a person we can keep personalising for. It is also the closest thing to a deletion
/// request this design can honour without collecting identities.
pub async fn prune_visitors(pool: &PgPool, days: i64) -> Result<(u64, u64), AppError> {
    let mut tx = pool.begin().await?;
    let stale: Vec<(String,)> = sqlx::query_as(
        "SELECT visitor_hash FROM visitor_profiles
          WHERE last_seen_at < now() - ($1::bigint * interval '1 day')",
    )
    .bind(days)
    .fetch_all(&mut *tx)
    .await?;

    let mut weights = 0u64;
    for (hash,) in &stale {
        let r = sqlx::query(
            "DELETE FROM preference_weights WHERE owner_kind = 'visitor' AND owner_id = $1",
        )
        .bind(hash)
        .execute(&mut *tx)
        .await?;
        weights += r.rows_affected();
    }
    let visitors = sqlx::query("DELETE FROM visitor_profiles WHERE last_seen_at < now() - ($1::bigint * interval '1 day')")
        .bind(days)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok((visitors, weights))
}

/// One read for the doctor and the operator. Every count is cast: `count(*)` is int4.
pub async fn catalogue_composition(pool: &PgPool) -> anyhow::Result<CatalogueComposition> {
    let row = sqlx::query_as::<_, CatalogueComposition>(
        "SELECT
           (SELECT count(*)::bigint FROM users
             WHERE claimed_at IS NULL AND password_hash IS NULL AND hidden = false) AS shells,
           (SELECT count(*)::bigint FROM users
             WHERE claimed_at IS NOT NULL OR password_hash IS NOT NULL) AS claimed,
           (SELECT count(*)::bigint FROM entities WHERE hidden = false) AS entities,
           (SELECT count(*)::bigint FROM entity_people WHERE role = 'subject') AS subjects,
           (SELECT count(*)::bigint FROM import_runs) AS receipts",
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Slugs are human-readable but uniqueness is not left to chance: an eight-hex suffix makes two
/// handles that normalise to the same slug different rows. A claim can give the entity a clean slug.
fn entity_slug(handle: &str) -> String {
    let raw: String = handle
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = raw.trim_matches('-');
    let base = if trimmed.len() < 2 {
        format!("m-{trimmed}")
    } else {
        trimmed.to_string()
    };
    let base: String = base.chars().take(51).collect();
    format!("{}-{}", base.trim_matches('-'), &Uuid::new_v4().simple().to_string()[..8])
}

/// Fill the substrate from shells that do not have a catalogue row yet. A shell is a user with no
/// password and no claim; an account is never backfilled as somebody else's subject. Tags that do not
/// satisfy `entity_tags`' own CHECK are counted out here instead of aborting the whole run.
pub async fn backfill_catalogue(pool: &PgPool, limit: Option<i64>) -> anyhow::Result<BackfillCounts> {
    sqlx::query(
        "INSERT INTO categories (slug, label, description, sort, nsfw)
         SELECT slug, label, description, sort, nsfw FROM verticals
         ON CONFLICT (slug) DO NOTHING",
    )
    .execute(pool)
    .await?;

    let rows: Vec<(Uuid, String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT u.id, u.handle, u.display_name, u.bio,
                (SELECT l.url FROM links l WHERE l.user_id = u.id AND l.hidden = false
                  ORDER BY l.position, l.created_at LIMIT 1) AS own_url,
                COALESCE((SELECT uv.vertical_slug FROM user_verticals uv
                  WHERE uv.user_id = u.id AND uv.role = 'primary' LIMIT 1), '') AS primary_vertical
         FROM users u
         WHERE u.hidden = false AND u.claimed_at IS NULL AND u.password_hash IS NULL
           AND NOT EXISTS (SELECT 1 FROM entity_people ep
                           WHERE ep.user_id = u.id AND ep.role = 'subject')
         ORDER BY u.created_at
         LIMIT $1",
    )
    .bind(limit.unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await?;

    let mut counts = BackfillCounts {
        seen: rows.len() as i64,
        ..Default::default()
    };
    for (user_id, handle, title, bio, own_url, primary_vertical) in rows {
        let key = own_url.unwrap_or_else(|| format!("handle:{handle}"));
        let nsfw = primary_vertical == "nsfw";
        let id: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO entities
               (slug, kind, canonical_key, title, description, category_slug, nsfw,
                origin, source, source_id, imported_at)
             VALUES ($1, 'person', $2, $3, $4, NULLIF($5, ''), $6,
                     'import', 'registry', $7, now())
             ON CONFLICT (canonical_key) DO NOTHING RETURNING id",
        )
        .bind(entity_slug(&handle))
        .bind(&key)
        .bind(title)
        .bind(bio)
        .bind(&primary_vertical)
        .bind(nsfw)
        .bind(&handle)
        .fetch_optional(pool)
        .await?;

        let Some(id) = id else {
            counts.conflicts += 1; // two shells sharing a first URL are one entity, not two
            // The shell still deserves a catalogue row and must not be picked up forever. Link it
            // to the entity that already owns this canonical key, without creating a second entity.
            if let Some(existing) = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM entities WHERE canonical_key = canonical_key($1)",
            )
            .bind(&key)
            .fetch_optional(pool)
            .await?
            {
                sqlx::query(
                    "INSERT INTO entity_people (entity_id, user_id, role, claimed_at)
                     VALUES ($1, $2, 'subject', NULL) ON CONFLICT DO NOTHING",
                )
                .bind(existing)
                .bind(user_id)
                .execute(pool)
                .await?;
            }
            continue;
        };
        counts.tags += sqlx::query(
            "INSERT INTO entity_tags (entity_id, tag_slug)
             SELECT $1, t.tag_slug FROM tags t
             WHERE t.user_id = $2
               AND t.tag_slug ~ '^[a-z0-9][a-z0-9 .-]{0,39}$'
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?
        .rows_affected() as i64;
        sqlx::query(
            "INSERT INTO entity_people (entity_id, user_id, role, claimed_at)
             VALUES ($1, $2, 'subject', NULL) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
        counts.inserted += 1;
    }
    Ok(counts)
}

/// A bounded read from the catalogue. The ORDER BY and LIMIT are part of the scale contract: this
/// query returns six rows whether the roster has two members or forty-eight thousand.
pub async fn catalogue_entries(
    pool: &PgPool,
    include_nsfw: bool,
    limit: i64,
) -> Result<Vec<EntitySummary>, AppError> {
    sqlx::query_as::<_, EntitySummary>(
        "SELECT e.id, e.slug, e.title, e.description,
                COALESCE(c.label, 'Catalogue') AS category_label,
                e.nsfw,
                EXISTS (SELECT 1 FROM entity_people ep JOIN users u ON u.id = ep.user_id
                        WHERE ep.entity_id = e.id
                          AND (u.claimed_at IS NOT NULL OR u.password_hash IS NOT NULL)) AS claimed,
                COALESCE((SELECT u.handle FROM entity_people ep JOIN users u ON u.id = ep.user_id
                          WHERE ep.entity_id = e.id AND ep.role = 'subject'
                          ORDER BY u.handle LIMIT 1), '') AS handle
         FROM entities e LEFT JOIN categories c ON c.slug = e.category_slug
         WHERE e.hidden = false AND e.merged_into IS NULL AND ($1 OR e.nsfw = false)
         ORDER BY e.created_at DESC
         LIMIT $2",
    )
    .bind(include_nsfw)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

/// Distinct visitors sent out by one vertical, without decoding the roster that produced them.
pub async fn vertical_visitors_total(pool: &PgPool, slug: &str) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(sum(visitors), 0)::bigint FROM (
           SELECT count(DISTINCT session_hash) AS visitors
             FROM source_hits WHERE vertical_context = $1 GROUP BY user_id
         ) v",
    )
    .bind(slug)
    .fetch_one(pool)
    .await?)
}

/// How many members have a visit in the last seven days — the Trending tab's own count, computed in
/// SQL instead of by ranking 48k decoded rows to throw most of them away.
pub async fn vertical_trending_count(pool: &PgPool, slug: &str) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT count(*)::bigint FROM (
           SELECT user_id FROM source_hits
            WHERE vertical_context = $1 AND created_at > now() - interval '7 days'
            GROUP BY user_id
         ) v",
    )
    .bind(slug)
    .fetch_one(pool)
    .await?)
}

/// The tag chips for a vertical, counted from the tags table rather than from decoded members.
pub async fn vertical_tag_counts(
    pool: &PgPool,
    slug: &str,
    limit: i64,
) -> Result<Vec<(String, i64)>, AppError> {
    Ok(sqlx::query_as(
        "SELECT t.tag_slug, count(DISTINCT t.user_id)::bigint AS n
           FROM tags t JOIN user_verticals uv ON uv.user_id = t.user_id
          WHERE uv.vertical_slug = $1
          GROUP BY t.tag_slug ORDER BY n DESC, t.tag_slug LIMIT $2",
    )
    .bind(slug)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// The site feeds emit 100 items. Decode 100 rows, not the whole roster, in the same order
/// `all_members` has always used.
pub async fn all_members_limited(
    pool: &PgPool,
    include_nsfw: bool,
    limit: i64,
) -> Result<Vec<MemberStat>, AppError> {
    let sql = format!(
        "SELECT {MEMBER_STAT_COLS}, 'primary'::text AS role
           {from}
          WHERE u.hidden = false AND ($1 OR {SFW_PRIMARY_ONLY})
          ORDER BY u.created_at DESC
          LIMIT $2",
        from = member_stats_from("NULL::text")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind(include_nsfw)
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

/// Catalogue matches for a search term. Twelve rows, whatever the size of `entities`; the LIKE
/// escapes the two characters that would otherwise turn a query into a wildcard.
pub async fn search_entities(
    pool: &PgPool,
    q: &str,
    include_nsfw: bool,
    limit: i64,
) -> Result<Vec<EntitySummary>, AppError> {
    let like = format!("%{}%", q.replace('%', "").replace('_', ""));
    sqlx::query_as::<_, EntitySummary>(
        "SELECT e.id, e.slug, e.title, e.description,
                COALESCE(c.label, 'Catalogue') AS category_label,
                e.nsfw,
                EXISTS (SELECT 1 FROM entity_people ep JOIN users u ON u.id = ep.user_id
                        WHERE ep.entity_id = e.id
                          AND (u.claimed_at IS NOT NULL OR u.password_hash IS NOT NULL)) AS claimed,
                COALESCE((SELECT u.handle FROM entity_people ep JOIN users u ON u.id = ep.user_id
                          WHERE ep.entity_id = e.id AND ep.role = 'subject'
                          ORDER BY u.handle LIMIT 1), '') AS handle
         FROM entities e LEFT JOIN categories c ON c.slug = e.category_slug
         WHERE e.hidden = false AND e.merged_into IS NULL AND ($2 OR e.nsfw = false)
           AND (e.title ILIKE $1 OR e.description ILIKE $1
                OR EXISTS (SELECT 1 FROM entity_tags et WHERE et.entity_id = e.id AND et.tag_slug ILIKE $1))
         ORDER BY e.title
         LIMIT $3",
    )
    .bind(like)
    .bind(include_nsfw)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

/// The one-time token a shell's claimant must place in their external profile. It is useful only
/// to somebody who controls that profile; publishing it does not prove the claim.
pub async fn claim_token(pool: &PgPool, handle: &str) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT verify_token FROM users
          WHERE handle = $1 AND claimed_at IS NULL AND password_hash IS NULL",
    )
    .bind(handle)
    .fetch_optional(pool)
    .await?)
}

/// Commit a claim only when the caller passes proof derived from the PDS. `None` is refused before
/// any write, so a route that forgot to verify cannot silently claim a shell.
pub async fn commit_claim(
    pool: &PgPool,
    handle: &str,
    proof: Option<crate::models::ClaimProof>,
    password_hash: &str,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let row: Option<(Uuid, Option<chrono::DateTime<chrono::Utc>>, Option<String>)> = sqlx::query_as(
        "SELECT id, claimed_at, password_hash FROM users WHERE handle = $1 FOR UPDATE",
    )
    .bind(handle)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, claimed_at, existing_password)) = row else {
        return Err(AppError::Validation(
            "no claimable shell with that handle".into(),
        ));
    };
    if claimed_at.is_some() || existing_password.is_some() {
        return Err(AppError::Conflict("that handle is already claimed".into()));
    }
    let Some(proof) = proof else {
        return Err(AppError::Validation("claim proof is missing".into()));
    };
    sqlx::query(
        "UPDATE users SET password_hash = $2, claimed_at = now(),
                          bsky_did = $3, bsky_handle = $4, bsky_verified_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(password_hash)
    .bind(&proof.did)
    .bind(&proof.handle)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE entity_people SET claimed_at = now() WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO import_runs (source, rows_seen, rows_inserted, note, finished_at)
         VALUES ('claim-proof', 1, 1, $1, now())",
    )
    .bind(format!(
        "entity-first refactor M3; handle={handle} did={}",
        proof.did
    ))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
