//! The public read API, versioned under `/api/v1`.
//!
//! The point is not "we have an API": it is that the rankings and the directory are worth building
//! on only if someone else can read them. Everything here is read-only, needs no account, and
//! returns the same facts the HTML pages show — no hidden fields, no extra rows the site would not
//! render. That constraint is deliberate: an API that returns more than the page is a leak with a
//! version number on it.
//!
//! Write access stays on the session-cookie API under `/api/` until tokens exist (they do not yet).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, PageError, PageResult};
use crate::models::MemberStat;
use crate::store;
use crate::AppState;

pub fn router() -> Router<AppState> {
    // Cached at the edge for a minute: a directory that changes hourly does not need to change
    // per request, and a read API is the thing strangers will hammer.
    Router::new()
        .route("/api/v1/stats", get(stats))
        .route("/api/v1/verticals", get(verticals))
        .route("/api/v1/members", get(members))
        .route("/api/v1/members/{handle}", get(member))
        .route("/api/v1/members/{handle}/links", get(member_links))
        .route("/api/v1/links/health", get(link_health))
        .route("/api/v1/ring/{slug}", get(ring))
}

/// Serialise once, then answer with a validator the client can use.
///
/// The ETag is weak (`W/`) on purpose: these responses are assembled from live queries, so two
/// bodies that hash the same are *equivalent*, not byte-identical by construction. Weak is the
/// honest tag, and it is what makes a poller's `If-None-Match` cost a 304 instead of the payload.
///
/// CORS is `*` because the API is public and read-only: the alternative is that nobody can build a
/// browser client, including a widget on our own blog. It is on every response, including the 304,
/// because a cached response without it is a response the browser discards.
fn cached(headers: &HeaderMap, value: Value) -> Response {
    let body = serde_json::to_vec(&value).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    body.hash(&mut hasher);
    let etag = format!("W/\"{:016x}\"", hasher.finish());

    let matched = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|inm| {
            inm.split(',')
                .map(|t| t.trim().trim_start_matches("W/"))
                .any(|t| t == etag.trim_start_matches("W/"))
        })
        .unwrap_or(false);

    if matched {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag),
                (header::CACHE_CONTROL, "public, max-age=60".to_string()),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".to_string()),
            ],
        )
            .into_response();
    }

    (
        [
            (header::CACHE_CONTROL, "public, max-age=60".to_string()),
            (header::CONTENT_TYPE, "application/json; charset=utf-8".to_string()),
            (header::ETAG, etag),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".to_string()),
            (header::ACCESS_CONTROL_EXPOSE_HEADERS, "etag".to_string()),
        ],
        Json(value),
    )
        .into_response()
}

/// The numbers on the front door, in one request.
async fn stats(State(st): State<AppState>, headers: HeaderMap) -> PageResult<Response> {
    let members = store::all_members(&st.pool, true).await?;
    let verticals = store::verticals(&st.pool, true).await?;
    let clicks: i64 = members.iter().map(|m| m.clicks).sum();
    let visitors: i64 = members.iter().map(|m| m.visitors).sum();
    Ok(cached(&headers, json!({
        "members": members.len(),
        "verticals": verticals.len(),
        "clicks": clicks,
        "visitors": visitors,
        // A link the checker has never seen is counted separately: "0 dead" means something
        // different when nothing has been checked.
        "links": link_counts(&st).await?,
    })))
}

async fn link_counts(st: &AppState) -> PageResult<Value> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE fail_count >= 3)::bigint,
                count(*) FILTER (WHERE last_checked IS NULL)::bigint
           FROM links WHERE hidden = false",
    )
    .fetch_one(&st.pool)
    .await?;
    Ok(json!({ "total": row.0, "failing": row.1, "unchecked": row.2 }))
}

async fn verticals(State(st): State<AppState>, headers: HeaderMap) -> PageResult<Response> {
    let all = store::verticals(&st.pool, true).await?;
    let mut out = Vec::new();
    for v in all {
        let members = store::vertical_members(&st.pool, &v.slug).await?;
        out.push(json!({
            "slug": v.slug,
            "label": v.label,
            "description": v.description,
            "members": members.len(),
            "url": format!("{}/v/{}", st.cfg.public_base, v.slug),
        }));
    }
    Ok(cached(&headers, json!({ "verticals": out })))
}

#[derive(Deserialize)]
pub struct MembersQuery {
    #[serde(default)]
    pub vertical: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

async fn members(
    State(st): State<AppState>,
    Query(q): Query<MembersQuery>,
    headers: HeaderMap,
) -> PageResult<Response> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);

    let list: Vec<MemberStat> = match (&q.vertical, &q.q) {
        (Some(slug), _) => store::vertical_members(&st.pool, slug).await?,
        (None, Some(term)) if !term.trim().is_empty() => {
            // `search` also returns the tag counts the HTML page renders; the API needs the
            // members only.
            store::search(&st.pool, term.trim(), true).await?.0
        }
        _ => store::all_members(&st.pool, true).await?,
    };
    let total = list.len() as i64;
    let rows: Vec<Value> = list
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|m| member_json(m, &st.cfg.public_base))
        .collect();
    Ok(cached(&headers, json!({
        "total": total,
        "limit": limit,
        "offset": offset,
        "members": rows,
    })))
}

async fn member(
    State(st): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> PageResult<Response> {
    let handle = handle.trim_start_matches('@').to_ascii_lowercase();
    let Some(u) = store::user_by_handle(&st.pool, &handle).await? else {
        return Err(PageError(AppError::NotFound));
    };
    if u.hidden {
        return Err(PageError(AppError::NotFound));
    }
    let stat = store::member_stat(&st.pool, u.id, None)
        .await?
        .ok_or(PageError(AppError::NotFound))?;
    let tags = store::tags_of(&st.pool, u.id).await?;
    Ok(cached(&headers, json!({
        "member": member_json(&stat, &st.cfg.public_base),
        "bio": u.bio,
        "tags": tags,
        // The profile's own claim of an external identity; verification state included, because
        // "claims to be" and "proved it" are different facts and conflating them is how a
        // directory starts publishing lies.
        "bluesky": u.bsky_handle.map(|h| json!({
            "handle": h,
            "verified": u.bsky_verified_at.is_some(),
        })),
        "links": link_rows(&st, u.id).await?,
    })))
}

async fn member_links(
    State(st): State<AppState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> PageResult<Response> {
    let handle = handle.trim_start_matches('@').to_ascii_lowercase();
    let Some(u) = store::user_by_handle(&st.pool, &handle).await? else {
        return Err(PageError(AppError::NotFound));
    };
    if u.hidden {
        return Err(PageError(AppError::NotFound));
    }
    Ok(cached(&headers, json!({ "handle": handle, "links": link_rows(&st, u.id).await? })))
}

async fn link_rows(st: &AppState, user_id: uuid::Uuid) -> PageResult<Vec<Value>> {
    let links = store::links_for(&st.pool, user_id, None, true).await?;
    Ok(links
        .iter()
        .map(|l| {
            json!({
                "label": l.label,
                "url": l.url,
                "vertical": l.vertical_slug,
                "clicks": l.click_count,
                // Health is part of the fact, not a dashboard secret: a consumer that reads this
                // API should not send people to a link the checker has watched fail.
                "last_status": l.last_status,
                "last_checked": l.last_checked,
                "failing": l.fail_count >= crate::health::FAIL_THRESHOLD,
                "fail_count": l.fail_count,
            })
        })
        .collect())
}

/// The dead links, worst first. This is the endpoint a member's own monitoring would use.
async fn link_health(State(st): State<AppState>, headers: HeaderMap) -> PageResult<Response> {
    let rows: Vec<(String, String, String, Option<i32>, i32, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT u.handle, l.label, l.url, l.last_status, l.fail_count, l.last_checked
               FROM links l JOIN users u ON u.id = l.user_id
              WHERE l.hidden = false AND l.fail_count >= 3
              ORDER BY l.fail_count DESC, l.last_checked DESC
              LIMIT 100",
        )
        .fetch_all(&st.pool)
        .await?;
    Ok(cached(&headers, json!({
        "threshold": crate::health::FAIL_THRESHOLD,
        "counts": link_counts(&st).await?,
        "failing": rows.iter().map(|(handle, label, url, status, fails, checked)| json!({
            "handle": handle,
            "label": label,
            "url": url,
            "last_status": status,
            "fail_count": fails,
            "last_checked": checked,
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct RingQuery {
    #[serde(default)]
    pub from: Option<String>,
}

/// The same ring `/v/{slug}/next` walks, as JSON — so a client can render the walk without
/// following redirects.
async fn ring(
    State(st): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<RingQuery>,
    headers: HeaderMap,
) -> PageResult<Response> {
    let Some(v) = store::vertical_by_slug(&st.pool, &slug).await? else {
        return Err(PageError(AppError::NotFound));
    };
    let mut members = store::vertical_members(&st.pool, &v.slug).await?;
    if members.is_empty() {
        return Err(PageError(AppError::NotFound));
    }
    let hub_tags = match store::owner_id(&st.pool).await? {
        Some(id) => store::tags_of(&st.pool, id).await?,
        None => Vec::new(),
    };
    members.sort_by(|a, b| a.handle.cmp(&b.handle));
    let day = chrono::Utc::now().timestamp() / 86_400;
    let order = crate::ranking::ring(&members, &hub_tags, &[v.slug.clone()], day);
    let from = q
        .from
        .as_deref()
        .map(|h| h.trim().trim_start_matches('@').to_ascii_lowercase());
    let pos = from.and_then(|f| order.iter().position(|m| m.handle.eq_ignore_ascii_case(&f)));
    let next = match pos {
        Some(i) => order[(i + 1) % order.len()],
        None => order[0],
    };
    Ok(cached(&headers, json!({
        "vertical": v.slug,
        "from": q.from,
        "next": { "handle": next.handle, "display_name": next.display_name },
        "url": format!("{}/@{}", st.cfg.public_base, next.handle),
        "size": order.len(),
    })))
}

fn member_json(m: &MemberStat, base: &str) -> Value {
    json!({
        "handle": m.handle,
        "display_name": m.display_name,
        "url": format!("{base}/@{}", m.handle),
        "verticals": m.verticals,
        "tags": m.tags,
        "trust_level": m.trust_level,
        "visitors": m.visitors,
        "visitors_7d": m.visitors_7d,
        "clicks": m.clicks,
        "joined": m.created_at,
    })
}
