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
     hidden, created_at";

/// Same list, qualified: `links_for` joins `verticals`, which also has `label`
/// and `icon` columns, so an unqualified list is an ambiguous-reference error.
const LINK_COLS_L: &str = "l.id, l.user_id, l.vertical_slug, l.url, l.label, l.icon, \
     l.position, l.click_count, l.hidden, l.created_at";

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
    (SELECT count(*) FROM source_hits sh
       WHERE sh.user_id = u.id
         AND ({ctx} IS NULL OR sh.vertical_context = {ctx} OR sh.vertical_context IS NULL)
    )::bigint AS clicks,
    COALESCE((SELECT array_agg(t.tag_slug ORDER BY t.tag_slug) FROM tags t WHERE t.user_id = u.id),
             '{{}}'::text[]) AS tags,
    COALESCE((SELECT array_agg(v.slug ORDER BY CASE WHEN uv2.role = 'primary' THEN 0 ELSE 1 END, v.sort)
                FROM user_verticals uv2 JOIN verticals v ON v.slug = uv2.vertical_slug
               WHERE uv2.user_id = u.id), '{{}}'::text[]) AS verticals
"#
    )
}

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
        "SELECT u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, u.trust_level,
                u.bsky_handle, u.nsfw_enabled, u.created_at, u.last_active_at, uv.role, {}
           FROM users u
           JOIN user_verticals uv ON uv.user_id = u.id AND uv.vertical_slug = $1
          WHERE u.hidden = false",
        member_stats("$1")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind(slug)
        .fetch_all(pool)
        .await?)
}

/// Every listed member, with stats over their whole click history.
pub async fn all_members(pool: &PgPool, include_nsfw: bool) -> Result<Vec<MemberStat>, AppError> {
    let sql = format!(
        "SELECT u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, u.trust_level,
                u.bsky_handle, u.nsfw_enabled, u.created_at, u.last_active_at, 'primary'::text AS role, {}
           FROM users u
          WHERE u.hidden = false AND ($1 OR {SFW_PRIMARY_ONLY})
          ORDER BY u.created_at DESC",
        member_stats("NULL::text")
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

pub async fn curated(pool: &PgPool, slug: &str) -> Result<Vec<MemberStat>, AppError> {
    let sql = format!(
        "SELECT u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, u.trust_level,
                u.bsky_handle, u.nsfw_enabled, u.created_at, u.last_active_at, 'primary'::text AS role, {}
           FROM curation c
           JOIN users u ON u.id = c.user_id
          WHERE c.vertical_slug = $1 AND u.hidden = false
          ORDER BY c.position, u.display_name",
        member_stats("$1")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind(slug)
        .fetch_all(pool)
        .await?)
}

pub async fn set_curation(pool: &PgPool, slug: &str, users: &[Uuid]) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM curation WHERE vertical_slug = $1")
        .bind(slug)
        .execute(&mut *tx)
        .await?;
    for (i, uid) in users.iter().enumerate() {
        sqlx::query("INSERT INTO curation (vertical_slug, user_id, position) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(slug)
            .bind(uid)
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
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
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO source_hits (user_id, link_id, surface, vertical_context, position, session_hash)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(link_id)
    .bind(surface)
    .bind(ctx)
    .bind(position)
    .bind(session_hash)
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

/// Curator's picks across every SFW vertical, in curation order.
pub async fn curated_any(
    pool: &PgPool,
    include_nsfw: bool,
    limit: i64,
) -> Result<Vec<MemberStat>, AppError> {
    let sql = format!(
        "SELECT u.id, u.handle, u.display_name, u.bio, u.accent, u.avatar_url, u.trust_level,
                u.bsky_handle, u.nsfw_enabled, u.created_at, u.last_active_at, 'primary'::text AS role, {}
           FROM (SELECT DISTINCT ON (c.user_id) c.user_id, c.position
                   FROM curation c JOIN verticals v ON v.slug = c.vertical_slug
                  WHERE ($2 OR v.nsfw = false)
                  ORDER BY c.user_id, c.position) c
           JOIN users u ON u.id = c.user_id
          WHERE u.hidden = false
          ORDER BY c.position, u.display_name
          LIMIT $3",
        member_stats("NULL::text")
    );
    Ok(sqlx::query_as::<_, MemberStat>(&sql)
        .bind::<Option<&str>>(None)
        .bind(include_nsfw)
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

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
