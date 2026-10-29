//! JSON API. Used by the Svelte dashboard and by anyone who wants to script
//! their profile. Every response is an explicit DTO: the database row is never
//! returned as-is, so a column added later cannot leak by accident.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{
    clear_session_cookie, hash_password, new_token, require_owner, session_cookie, verify_password,
    CurrentUser, MaybeUser,
};
use crate::error::{ApiError, ApiResult, AppError};
use crate::models::{LinkInput, LinkPatch, LoginInput, ProfilePatch, RegisterInput, ReorderInput};
use crate::render::{fmt_count, month_year, tracked_link};
use crate::store;
use crate::text::{accent_for, normalise_handle, normalise_slug, normalise_tag, validate_link_url};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/verticals", get(verticals))
        .route("/api/me", get(me).patch(update_me))
        .route("/api/me/verify-bsky", post(verify_bsky))
        .route("/api/me/verify-token", post(rotate_token))
        .route("/api/me/standing", get(standing))
        .route("/api/links", get(list_links).post(create_link))
        .route("/api/links/reorder", post(reorder))
        .route("/api/links/{id}", patch(update_link).delete(delete_link))
        .route("/api/analytics", get(analytics))
        .route("/api/flags", post(create_flag))
        .route("/api/admin/overview", get(admin_overview))
        .route("/api/admin/flags", get(admin_flags))
        .route("/api/admin/flags/{id}/resolve", post(admin_resolve))
        .route("/api/admin/users", get(admin_users))
        .route("/api/admin/users/{id}", post(admin_patch_user))
        .route("/api/admin/verticals", post(admin_create_vertical))
        .route("/api/admin/curation/{slug}", post(admin_curation))
}

fn json_with_cookie(value: Value, cookie: HeaderValue) -> Response {
    let mut res = Json(value).into_response();
    res.headers_mut().append(header::SET_COOKIE, cookie);
    res
}

fn user_json(u: &crate::models::User) -> Value {
    json!({
        "id": u.id,
        "handle": u.handle,
        "display_name": u.display_name,
        "bio": u.bio,
        "accent": u.accent,
        "avatar_url": u.avatar_url,
        "trust_level": u.trust_level,
        "nsfw_enabled": u.nsfw_enabled,
        "is_owner": u.is_owner,
        "bsky_handle": u.bsky_handle,
        "verified": u.bsky_handle.is_some(),
        "created_at": u.created_at.to_rfc3339(),
        "joined": month_year(u.created_at),
    })
}

// ------------------------------------------------------------------ auth

pub async fn register(
    State(st): State<AppState>,
    Json(input): Json<RegisterInput>,
) -> ApiResult<Response> {
    let handle = normalise_handle(&input.handle)?;
    let vertical_slug = normalise_slug(&input.vertical)?;
    let Some(vertical) = store::vertical_by_slug(&st.pool, &vertical_slug).await? else {
        return Err(ApiError(AppError::Validation("no such vertical".into())));
    };
    let display_name = if input.display_name.trim().is_empty() {
        handle.clone()
    } else {
        crate::text::truncate(input.display_name.trim(), 60)
    };
    let hash = hash_password(&input.password)?;
    let user = store::create_user(
        &st.pool,
        store::NewUser {
            handle: &handle,
            display_name: &display_name,
            bio: &crate::text::truncate(input.bio.trim(), 600),
            accent: &accent_for(&handle),
            password_hash: &hash,
            nsfw_enabled: vertical.nsfw,
        },
    )
    .await?;
    store::set_verticals(&st.pool, user.id, &vertical_slug, &[]).await?;
    let token = new_token();
    store::create_session(&st.pool, user.id, &token, 30).await?;
    store::touch_login(&st.pool, user.id).await?;
    Ok(json_with_cookie(
        json!({ "user": user_json(&user), "verify_token": user.verify_token }),
        session_cookie(&st.cfg, &token, 30),
    ))
}

pub async fn login(
    State(st): State<AppState>,
    Json(input): Json<LoginInput>,
) -> ApiResult<Response> {
    let handle = normalise_handle(&input.handle)?;
    let Some(hash) = store::password_hash(&st.pool, &handle).await? else {
        // Same message and roughly the same work as a wrong password: an
        // enumeration oracle is a gift to a credential stuffer.
        let _ = verify_password(&input.password, "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$0000000000000000000000000000000000000000000");
        return Err(ApiError(AppError::Unauthorized));
    };
    if !verify_password(&input.password, &hash) {
        return Err(ApiError(AppError::Unauthorized));
    }
    let Some(user) = store::user_by_handle(&st.pool, &handle).await? else {
        return Err(ApiError(AppError::Unauthorized));
    };
    let token = new_token();
    store::create_session(&st.pool, user.id, &token, 30).await?;
    store::touch_login(&st.pool, user.id).await?;
    Ok(json_with_cookie(
        json!({ "user": user_json(&user) }),
        session_cookie(&st.cfg, &token, 30),
    ))
}

pub async fn logout(State(st): State<AppState>, user: MaybeUser) -> ApiResult<Response> {
    if let Some(u) = user.0 {
        let _ = u;
    }
    let mut res = Json(json!({ "ok": true })).into_response();
    res.headers_mut()
        .append(header::SET_COOKIE, clear_session_cookie(&st.cfg));
    Ok(res)
}

// ------------------------------------------------------------------ me

#[derive(Deserialize)]
pub struct VerticalsQuery {
    /// `all=1` includes the adult vertical. Its existence is not a secret — the
    /// listing a member publishes there is — so the catalogue is the same for
    /// everyone and only the membership data is gated by host.
    //
    // Taken as a string rather than a bool: `?all=1` is what people write, and
    // rejecting it with a 400 deserialize error is a trap for every future
    // caller. `true`, `1` and `yes` all mean yes.
    #[serde(default)]
    pub all: Option<String>,
}

pub async fn verticals(
    State(st): State<AppState>,
    Query(q): Query<VerticalsQuery>,
) -> ApiResult<Json<Value>> {
    let all = q
        .all
        .as_deref()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "all"))
        .unwrap_or(false);
    let rows = store::verticals(&st.pool, all).await?;
    Ok(Json(json!({
        "verticals": rows.iter().map(|v| json!({
            "slug": v.slug, "label": v.label, "description": v.description,
            "icon": v.icon, "nsfw": v.nsfw,
        })).collect::<Vec<_>>()
    })))
}

pub async fn me(State(st): State<AppState>, CurrentUser(user): CurrentUser) -> ApiResult<Json<Value>> {
    let pairs = store::verticals_of(&st.pool, user.id).await?;
    let tags = store::tags_of(&st.pool, user.id).await?;
    let links = store::all_links(&st.pool, user.id).await?;
    let verticals: Vec<Value> = store::verticals(&st.pool, true)
        .await?
        .into_iter()
        .filter(|v| pairs.iter().any(|(s, _)| s == &v.slug))
        .map(|v| {
            json!({
                "slug": v.slug, "label": v.label, "icon": v.icon, "nsfw": v.nsfw,
                "role": pairs.iter().find(|(s, _)| s == &v.slug).map(|(_, r)| r.clone()).unwrap_or_default(),
            })
        })
        .collect();
    Ok(Json(json!({
        "user": user_json(&user),
        "verticals": verticals,
        "tags": tags,
        "verify_token": user.verify_token,
        "links": links.iter().map(|l| json!({
            "id": l.id, "url": l.url, "label": l.label, "icon": l.icon,
            "position": l.position, "click_count": l.click_count,
            "clicks_label": fmt_count(l.click_count),
            "vertical_slug": l.vertical_slug, "hidden": l.hidden,
            // The member's own view of their health. The profile badge tells visitors; this
            // tells the person who can actually fix it.
            "last_status": l.last_status, "last_checked": l.last_checked,
            "fail_count": l.fail_count, "failing": l.fail_count >= crate::health::FAIL_THRESHOLD,
        })).collect::<Vec<_>>(),
        "public_url": format!("{}/@{}", st.cfg.public_base, user.handle),
    })))
}

pub async fn update_me(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(patch): Json<ProfilePatch>,
) -> ApiResult<Json<Value>> {
    if let Some(name) = &patch.display_name {
        if name.trim().is_empty() {
            return Err(ApiError(AppError::Validation(
                "a display name cannot be empty".into(),
            )));
        }
    }
    if let Some(accent) = &patch.accent {
        let ok = accent.len() == 7
            && accent.starts_with('#')
            && accent[1..].chars().all(|c| c.is_ascii_hexdigit());
        if !ok {
            return Err(ApiError(AppError::Validation(
                "accent is a hex colour like #7c5cff".into(),
            )));
        }
    }
    if let Some(url) = &patch.avatar_url {
        if !url.trim().is_empty() {
            validate_link_url(url)?;
        }
    }
    let patch_tags = patch.tags.clone();
    let mut clean = ProfilePatch {
        display_name: patch.display_name.as_ref().map(|s| crate::text::truncate(s.trim(), 60)),
        bio: patch.bio.as_ref().map(|s| crate::text::truncate(s.trim(), 600)),
        tags: None,
        ..patch
    };

    if let Some(primary) = clean.primary_vertical.clone() {
        let slug = normalise_slug(&primary)?;
        let Some(v) = store::vertical_by_slug(&st.pool, &slug).await? else {
            return Err(ApiError(AppError::Validation("no such vertical".into())));
        };
        let secondaries: Vec<String> = clean
            .secondary_verticals
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|s| normalise_slug(s))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|s| s != &slug)
            .take(2)
            .collect();
        for s in &secondaries {
            if store::vertical_by_slug(&st.pool, s).await?.is_none() {
                return Err(ApiError(AppError::Validation(format!("no such vertical `{s}`"))));
            }
        }
        store::set_verticals(&st.pool, user.id, &slug, &secondaries).await?;
        // Choosing the adult vertical turns the adult flag on; the reverse is
        // left to the member, because removing it could hide a listing.
        if v.nsfw {
            clean.nsfw_enabled = Some(true);
        }
    }

    let user = store::update_profile(&st.pool, user.id, &clean).await?;
    if let Some(tags) = &patch_tags {
        let tags: Vec<String> = tags
            .iter()
            .map(|t| normalise_tag(t))
            .collect::<Result<Vec<_>, _>>()?;
        if tags.len() > 10 {
            return Err(ApiError(AppError::Validation("at most 10 tags".into())));
        }
        store::set_tags(&st.pool, user.id, &tags).await?;
    }
    Ok(Json(json!({ "user": user_json(&user) })))
}

pub async fn rotate_token(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Value>> {
    let token = store::rotate_verify_token(&st.pool, user.id).await?;
    Ok(Json(json!({ "verify_token": token })))
}

#[derive(Deserialize)]
pub struct BskyClaim {
    pub handle: String,
}

/// Verification is a claim the member makes about a handle they control: the
/// token has to appear in the public Bluesky profile description. The trusted
/// side is the PDS response, not the member's assertion about it.
pub async fn verify_bsky(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(claim): Json<BskyClaim>,
) -> ApiResult<Json<Value>> {
    let handle = claim.handle.trim().trim_start_matches('@').to_lowercase();
    if handle.len() < 3 || !handle.contains('.') {
        return Err(ApiError(AppError::Validation(
            "give the full Bluesky handle, like someone.bsky.social".into(),
        )));
    }
    let url = format!(
        "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
        st.cfg.bsky_pds,
        crate::render::urlencode(&handle)
    );
    let did: String = match st.http.get(&url).send().await {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(v) => v["did"].as_str().unwrap_or_default().to_string(),
            Err(_) => String::new(),
        },
        _ => String::new(),
    };
    if did.is_empty() {
        return Err(ApiError(AppError::Validation(
            "Bluesky does not know that handle".into(),
        )));
    }
    let profile_url = format!(
        "{}/xrpc/app.bsky.actor.getProfile?actor={}",
        st.cfg.bsky_pds,
        crate::render::urlencode(&did)
    );
    let description = match st.http.get(&profile_url).send().await {
        Ok(r) if r.status().is_success() => r
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v["description"].as_str().map(|s| s.to_string()))
            .unwrap_or_default(),
        _ => String::new(),
    };
    let needle = format!("orbital-verify:{}", user.verify_token);
    if !description.contains(&needle) {
        return Err(ApiError(AppError::Validation(format!(
            "put `{needle}` in your Bluesky description, then try again"
        ))));
    }
    store::set_bsky_claim(&st.pool, user.id, &did, &handle).await?;
    Ok(Json(json!({ "verified": true, "did": did, "handle": handle })))
}

// ------------------------------------------------------------------ links

/// The member's own rank, in the same order the public pages show.
pub async fn standing(State(st): State<AppState>, CurrentUser(user): CurrentUser) -> ApiResult<Json<Value>> {
    let rows = store::standing(&st.pool, user.id, chrono::Utc::now()).await?;
    Ok(Json(json!({ "standing": rows })))
}

pub async fn list_links(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Value>> {
    let links = store::all_links(&st.pool, user.id).await?;
    Ok(Json(json!({
        "links": links.iter().map(|l| json!({
            "id": l.id, "url": l.url, "label": l.label, "icon": l.icon,
            "position": l.position, "click_count": l.click_count,
            "clicks_label": fmt_count(l.click_count),
            "vertical_slug": l.vertical_slug, "hidden": l.hidden,
            "tracked": tracked_link(&st.cfg.public_base, l, "profile", None, l.position as usize),
        })).collect::<Vec<_>>()
    })))
}

pub async fn create_link(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<LinkInput>,
) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let url = validate_link_url(&input.url)?;
    let count = store::count_links(&st.pool, user.id).await?;
    if count >= 40 {
        return Err(ApiError(AppError::Validation(
            "40 links is the limit; a profile is a page, not a sitemap".into(),
        )));
    }
    let label = if input.label.trim().is_empty() {
        crate::text::pretty_url(&url)
    } else {
        crate::text::truncate(input.label.trim(), 60)
    };
    let icon = match input.icon.as_deref() {
        Some(name) if crate::html::is_known_icon(name) => name.to_string(),
        _ => crate::html::icon_for_url(&url).to_string(),
    };
    let vertical = match input.vertical_slug.as_deref() {
        Some(s) if !s.trim().is_empty() => {
            let slug = normalise_slug(s)?;
            if store::vertical_by_slug(&st.pool, &slug).await?.is_none() {
                return Err(ApiError(AppError::Validation("no such vertical".into())));
            }
            Some(slug)
        }
        _ => None,
    };
    let link = store::create_link(
        &st.pool,
        user.id,
        &url,
        &label,
        &icon,
        vertical.as_deref(),
    )
    .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(json!({
            "link": {
                "id": link.id, "url": link.url, "label": link.label, "icon": link.icon,
                "position": link.position, "vertical_slug": link.vertical_slug,
            }
        })),
    ))
}

pub async fn update_link(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(patch): Json<LinkPatch>,
) -> ApiResult<Json<Value>> {
    let mut clean = patch;
    if let Some(url) = &clean.url {
        clean.url = Some(validate_link_url(url)?);
    }
    if let Some(label) = &clean.label {
        clean.label = Some(crate::text::truncate(label.trim(), 60));
    }
    if let Some(icon) = &clean.icon {
        if !crate::html::is_known_icon(icon) {
            return Err(ApiError(AppError::Validation("unknown icon".into())));
        }
    }
    if let Some(Some(slug)) = &clean.vertical_slug {
        if slug.trim().is_empty() {
            clean.vertical_slug = Some(None);
        } else {
            let s = normalise_slug(slug)?;
            if store::vertical_by_slug(&st.pool, &s).await?.is_none() {
                return Err(ApiError(AppError::Validation("no such vertical".into())));
            }
            clean.vertical_slug = Some(Some(s));
        }
    }
    let updated = store::update_link(&st.pool, user.id, id, &clean).await?;
    match updated {
        Some(l) => Ok(Json(json!({
            "link": {
                "id": l.id, "url": l.url, "label": l.label, "icon": l.icon,
                "position": l.position, "vertical_slug": l.vertical_slug, "hidden": l.hidden,
            }
        }))),
        None => Err(ApiError(AppError::NotFound)),
    }
}

pub async fn delete_link(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    if store::delete_link(&st.pool, user.id, id).await? {
        Ok(Json(json!({ "deleted": true })))
    } else {
        Err(ApiError(AppError::NotFound))
    }
}

pub async fn reorder(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ReorderInput>,
) -> ApiResult<Json<Value>> {
    store::reorder_links(&st.pool, user.id, &input.order).await?;
    Ok(Json(json!({ "ok": true })))
}

// ------------------------------------------------------------------ analytics

#[derive(Deserialize)]
pub struct AnalyticsQuery {
    #[serde(default)]
    pub days: Option<i32>,
}

pub async fn analytics(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<AnalyticsQuery>,
) -> ApiResult<Json<Value>> {
    let days = q.days.unwrap_or(30).clamp(1, 180);
    let (series, top_links, by_surface, by_vertical, visitors, clicks) =
        store::analytics(&st.pool, user.id, days).await?;
    let by_referrer = store::referrer_breakdown(&st.pool, user.id, days).await?;
    // What the people who actually clicked care about, bucketed so a member reads a shape rather
    // than a decimal. Aggregates only: no visitor id leaves this query.
    let audience: Vec<serde_json::Value> = store::audience_tags(&st.pool, user.id, 12)
        .await?
        .into_iter()
        .map(|(tag, mean, n)| {
            let strength = if mean >= 0.6 {
                "strong"
            } else if mean >= 0.3 {
                "moderate"
            } else {
                "weak"
            };
            serde_json::json!({ "tag": tag, "strength": strength, "mean": (mean * 100.0).round() / 100.0, "visitors": n })
        })
        .collect();
    let views: i64 = series.iter().map(|d| d.views).sum();
    Ok(Json(json!({
        "days": days,
        "totals": {
            "views": views,
            "visitors": visitors,
            "clicks": clicks,
            "clicks_per_visitor": if visitors > 0 { (clicks as f64 / visitors as f64 * 100.0).round() / 100.0 } else { 0.0 },
        },
        "series": series,
        "top_links": top_links,
        "by_surface": by_surface,
        "by_vertical": by_vertical,
        // Which network sent the visitor. The surface says which of our pages earned the click;
        // this says whether the announcement on Bluesky or the one on Mastodon actually worked.
        "by_referrer": by_referrer,
        "audience_tags": audience,
    })))
}

// ------------------------------------------------------------------ flags

#[derive(Deserialize)]
pub struct FlagInput {
    pub handle: String,
    #[serde(default)]
    pub vertical: Option<String>,
    pub reason: String,
    #[serde(default)]
    pub detail: String,
}

pub async fn create_flag(
    State(st): State<AppState>,
    MaybeUser(user): MaybeUser,
    Json(input): Json<FlagInput>,
) -> ApiResult<Json<Value>> {
    let handle = normalise_handle(&input.handle)?;
    let Some(target) = store::user_by_handle(&st.pool, &handle).await? else {
        return Err(ApiError(AppError::NotFound));
    };
    store::create_flag(
        &st.pool,
        user.map(|u| u.id),
        target.id,
        input.vertical.as_deref(),
        &crate::text::truncate(&input.reason, 60),
        &crate::text::truncate(&input.detail, 2000),
    )
    .await?;
    Ok(Json(json!({ "reported": true })))
}

// ------------------------------------------------------------------ admin

fn guard(user: &crate::models::User) -> ApiResult<()> {
    require_owner(user).map_err(ApiError)
}

pub async fn admin_overview(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Value>> {
    guard(&user)?;
    let (members, clicks) = store::totals(&st.pool, true).await?;
    let open = store::flags(&st.pool, true).await?;
    let verticals = store::vertical_counts(&st.pool, true).await?;
    Ok(Json(json!({
        "members": members,
        "clicks": clicks,
        "open_flags": open.len(),
        "verticals": verticals.iter().map(|v| json!({
            "slug": v.slug, "label": v.label, "members": v.members, "nsfw": v.nsfw,
        })).collect::<Vec<_>>(),
    })))
}

pub async fn admin_flags(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Value>> {
    guard(&user)?;
    let rows = store::flags(&st.pool, false).await?;
    Ok(Json(json!({
        "flags": rows.iter().map(|f| json!({
            "id": f.id, "target": f.target_handle, "vertical": f.vertical_slug,
            "reason": f.reason, "detail": f.detail, "status": f.status,
            "created_at": f.created_at.to_rfc3339(),
        })).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
pub struct ResolveInput {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub zero_ranking: Option<bool>,
}

pub async fn admin_resolve(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ResolveInput>,
) -> ApiResult<Json<Value>> {
    guard(&user)?;
    let status = input.status.unwrap_or_else(|| "resolved".to_string());
    if !matches!(status.as_str(), "resolved" | "dismissed") {
        return Err(ApiError(AppError::Validation(
            "status is resolved or dismissed".into(),
        )));
    }
    let note = input.note.unwrap_or_default();
    let Some((target, reason)) = store::resolve_flag(&st.pool, id, user.id, &status, &note).await?
    else {
        return Err(ApiError(AppError::NotFound));
    };
    if input.zero_ranking.unwrap_or(false) {
        store::zero_ranking(&st.pool, target).await?;
        store::record_bot_post(&st.pool, "moderation", &target.to_string(), Some(&reason)).await?;
    }
    Ok(Json(json!({ "ok": true, "target": target, "status": status })))
}

pub async fn admin_users(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Value>> {
    guard(&user)?;
    let members = store::all_members(&st.pool, true).await?;
    Ok(Json(json!({
        "users": members.iter().map(|m| json!({
            "id": m.id, "handle": m.handle, "display_name": m.display_name,
            "trust_level": m.trust_level, "nsfw": m.nsfw_enabled,
            "visitors": m.visitors, "clicks": m.clicks,
            "created_at": m.created_at.to_rfc3339(),
        })).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
pub struct AdminUserPatch {
    #[serde(default)]
    pub trust_level: Option<i16>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

pub async fn admin_patch_user(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(patch): Json<AdminUserPatch>,
) -> ApiResult<Json<Value>> {
    guard(&user)?;
    if let Some(level) = patch.trust_level {
        if !(0..=3).contains(&level) {
            return Err(ApiError(AppError::Validation("trust level is 0–3".into())));
        }
        store::set_trust(&st.pool, id, level, patch.hidden).await?;
    } else if let Some(hidden) = patch.hidden {
        let current = store::user_by_id(&st.pool, id)
            .await?
            .ok_or(ApiError(AppError::NotFound))?;
        store::set_trust(&st.pool, id, current.trust_level, Some(hidden)).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct NewVertical {
    pub slug: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub nsfw: Option<bool>,
}

pub async fn admin_create_vertical(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<NewVertical>,
) -> ApiResult<Json<Value>> {
    guard(&user)?;
    let slug = normalise_slug(&input.slug)?;
    if input.label.trim().is_empty() {
        return Err(ApiError(AppError::Validation("a label is required".into())));
    }
    if store::vertical_by_slug(&st.pool, &slug).await?.is_some() {
        return Err(ApiError(AppError::Conflict("that vertical exists".into())));
    }
    store::create_vertical(
        &st.pool,
        &slug,
        &crate::text::truncate(input.label.trim(), 40),
        &crate::text::truncate(input.description.trim(), 200),
        input.icon.as_deref().unwrap_or("dot"),
        input.nsfw.unwrap_or(false),
    )
    .await?;
    Ok(Json(json!({ "ok": true, "slug": slug })))
}

#[derive(Deserialize)]
pub struct CurationInput {
    pub users: Vec<Uuid>,
}

pub async fn admin_curation(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(slug): Path<String>,
    Json(input): Json<CurationInput>,
) -> ApiResult<Json<Value>> {
    guard(&user)?;
    let slug = normalise_slug(&slug)?;
    if store::vertical_by_slug(&st.pool, &slug).await?.is_none() {
        return Err(ApiError(AppError::NotFound));
    }
    store::set_curation(&st.pool, &slug, &input.users).await?;
    Ok(Json(json!({ "ok": true, "count": input.users.len() })))
}

#[allow(dead_code)]
pub async fn not_found() -> ApiError {
    ApiError(AppError::NotFound)
}

#[allow(dead_code)]
pub(crate) fn now() -> chrono::DateTime<chrono::Utc> {
    Utc::now()
}
