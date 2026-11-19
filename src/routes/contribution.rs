//! Contribution intake, the trust-gated action log and user moderation (SPEC §9, D60).
//!
//! The log is a user surface, not an admin toy: TL2 can review and act, TL3/owner have the last
//! say. Every route here resolves a declared capability through `polaris-authz` before it touches
//! a row, and every decision writes a `polaris-ledger`-validated entry.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use polaris_ledger::{Actor, Cursor, Redaction, Subject as LedgerSubject};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, QueryBuilder, Row};
use uuid::Uuid;

use crate::auth::{CurrentUser, MaybeUser};
use crate::contribution::{self, ClientKey};
use crate::error::{ApiError, ApiResult, AppError};
use crate::text::{normalise_slug, validate_link_url};
use crate::AppState;

const ACTION_PROFILE: &str = "profile.create";
const ACTION_REVIEW: &str = "modlog.review";
const ACTION_ACT: &str = "moderation.act";
const ACTION_OVERRIDE: &str = "moderation.override";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/contribute/profile", axum::routing::post(contribute_profile))
        .route("/api/modlog", axum::routing::get(modlog))
        .route("/api/modlog/summary", axum::routing::get(modlog_summary))
        .route("/api/moderation/{id}", axum::routing::post(moderation_action))
}

#[derive(Debug, Deserialize)]
pub struct ProfileIntake {
    pub url: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default = "default_vertical")]
    pub vertical: String,
}

fn default_vertical() -> String {
    "nsfw".into()
}

/// The identity of a contribution is its canonical URL, not the name someone typed. Import shells
/// already carry most link-in-bio pages; a second profile for the same link is a duplicate, not a
/// new member.
async fn shell_for_url(pool: &PgPool, url: &str) -> Result<Option<(Uuid, String, String)>, AppError> {
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT u.id, u.handle, u.display_name
         FROM links l JOIN users u ON u.id = l.user_id
         WHERE canonical_key(l.url) = canonical_key($1)
         ORDER BY l.created_at LIMIT 1",
    )
    .bind(url)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;
    Ok(row)
}

fn derive_handle(url: &str, display_name: &str) -> String {
    let mut seed = display_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
        .collect::<String>()
        .to_lowercase();
    if seed.len() < 2 {
        seed = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or("new-member")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_lowercase();
    }
    if seed.len() < 2 {
        seed = "new-member".into();
    }
    seed.chars().take(30).collect()
}

async fn unique_handle(pool: &PgPool, seed: &str) -> Result<String, AppError> {
    for n in 0..100 {
        let candidate = if n == 0 { seed.to_string() } else { format!("{seed}-{n}") };
        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM users WHERE handle = $1")
                .bind(&candidate)
                .fetch_optional(pool)
                .await
                .map_err(AppError::from)?;
        if exists.is_none() {
            return Ok(candidate);
        }
    }
    Err(AppError::Conflict("no free handle for that profile".into()))
}

pub async fn contribute_profile(
    State(st): State<AppState>,
    MaybeUser(user): MaybeUser,
    ClientKey(client): ClientKey,
    Json(input): Json<ProfileIntake>,
) -> ApiResult<Response> {
    // Guard, then budget, then write. A refused contribution must still appear in the log.
    if let Err(e) = contribution::gate(&st, user.as_ref(), ACTION_PROFILE) {
        let _ = contribution::record(&st.pool, contribution::Entry {
            action: ACTION_PROFILE, decision: "refused", actor: user.as_ref(),
            actor_handle: user.as_ref().map(|u| u.handle.as_str()),
            visitor_hash: None, subject_type: Some("link"), subject_id: Some(&input.url),
            before: None, after: None, reason: "gate refused", gate: contribution::gate_mode(&st, ACTION_PROFILE),
            client_hash: Some(&client), request_id: None,
        }).await;
        return Err(e);
    }
    if let Err(e) = contribution::spend(&st, &ClientKey(client.clone()), ACTION_PROFILE, user.as_ref()) {
        let _ = contribution::record(&st.pool, contribution::Entry {
            action: ACTION_PROFILE, decision: "refused", actor: user.as_ref(),
            actor_handle: user.as_ref().map(|u| u.handle.as_str()),
            visitor_hash: None, subject_type: Some("link"), subject_id: Some(&input.url),
            before: None, after: None, reason: "rate limited", gate: contribution::gate_mode(&st, ACTION_PROFILE),
            client_hash: Some(&client), request_id: None,
        }).await;
        return Err(e);
    }

    let url = validate_link_url(&input.url)?;
    let vertical = normalise_slug(&input.vertical)?;
    if sqlx::query_as::<_, (String,)>("SELECT slug FROM verticals WHERE slug = $1")
        .bind(&vertical)
        .fetch_optional(&st.pool)
        .await
        .map_err(AppError::from)?
        .is_none()
    {
        return Err(ApiError(AppError::Validation("no such vertical".into())));
    }

    if let Some((_id, handle, old_name)) = shell_for_url(&st.pool, &url).await? {
        let new_name = if input.display_name.trim().is_empty() { old_name.clone() } else { input.display_name.trim().to_string() };
        if new_name != old_name {
            sqlx::query("UPDATE users SET display_name = $2 WHERE id = $1")
                .bind(_id).bind(&new_name).execute(&st.pool).await.map_err(AppError::from)?;
        }
        contribution::record(&st.pool, contribution::Entry {
            action: ACTION_PROFILE, decision: "applied", actor: user.as_ref(),
            actor_handle: user.as_ref().map(|u| u.handle.as_str()),
            visitor_hash: None, subject_type: Some("profile"), subject_id: Some(&handle),
            before: Some(json!({"display_name": old_name, "url": url})),
            after: Some(json!({"display_name": new_name, "url": url})),
            reason: "existing canonical URL; updated in place",
            gate: contribution::gate_mode(&st, ACTION_PROFILE), client_hash: Some(&client), request_id: None,
        }).await?;
        return Ok((StatusCode::OK, Json(json!({
            "already_existing": true, "handle": handle, "display_name": new_name, "url": url,
        }))).into_response());
    }

    let seed = derive_handle(&url, &input.display_name);
    let handle = unique_handle(&st.pool, &seed).await?;
    let display = if input.display_name.trim().is_empty() { handle.clone() } else { crate::text::truncate(input.display_name.trim(), 60) };
    let accent = crate::text::accent_for(&handle);
    let nsfw = sqlx::query_as::<_, (bool,)>("SELECT nsfw FROM verticals WHERE slug = $1")
        .bind(&vertical).fetch_one(&st.pool).await.map_err(AppError::from)?.0;
    let bio = crate::text::truncate(input.bio.trim(), 600);

    let mut tx = st.pool.begin().await.map_err(AppError::from)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name, bio, accent, nsfw_enabled)
         VALUES ($1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(&handle).bind(&display).bind(&bio).bind(&accent).bind(nsfw)
    .fetch_one(&mut *tx).await.map_err(AppError::from)?;
    sqlx::query(
        "INSERT INTO links (user_id, url, label, vertical_slug, position)
         VALUES ($1,$2,$3,$4,0)",
    )
    .bind(id).bind(&url).bind(crate::text::pretty_url(&url)).bind(&vertical)
    .execute(&mut *tx).await.map_err(AppError::from)?;
    sqlx::query(
        "INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1,$2,'primary')",
    )
    .bind(id).bind(&vertical).execute(&mut *tx).await.map_err(AppError::from)?;
    sqlx::query(
        "INSERT INTO contribution_log (action, decision, actor_id, actor_handle, subject_type,
            subject_id, after_state, reason, gate_mode, client_hash)
         VALUES ($1,'applied',$2,$3,'profile',$4,$5,$6,$7,$8)",
    )
    .bind(ACTION_PROFILE)
    .bind(user.as_ref().map(|u| u.id))
    .bind(user.as_ref().map(|u| u.handle.as_str()))
    .bind(&handle)
    .bind(json!({"display_name": display, "url": url, "vertical": vertical}))
    .bind("profile created from a link")
    .bind(contribution::gate_mode(&st, ACTION_PROFILE))
    .bind(&client)
    .execute(&mut *tx).await.map_err(AppError::from)?;
    tx.commit().await.map_err(AppError::from)?;

    Ok((StatusCode::CREATED, Json(json!({
        "handle": handle, "display_name": display, "url": url, "already_existing": false,
    }))).into_response())
}

#[derive(Debug, Deserialize)]
pub struct ModlogQuery {
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub subject_type: Option<String>,
    #[serde(default)]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 { 50 }

fn where_clause(qb: &mut QueryBuilder<'_, sqlx::Postgres>, q: &ModlogQuery) {
    qb.push(" WHERE true");
    if let Some(v) = &q.action { qb.push(" AND action = ").push_bind(v.clone()); }
    if let Some(v) = &q.actor { qb.push(" AND actor_handle = ").push_bind(v.clone()); }
    if let Some(v) = &q.subject_type { qb.push(" AND subject_type = ").push_bind(v.clone()); }
    if let Some(v) = &q.subject_id { qb.push(" AND subject_id = ").push_bind(v.clone()); }
    if let Some(v) = &q.decision { qb.push(" AND decision = ").push_bind(v.clone()); }
    if let Some(v) = q.from { qb.push(" AND created_at >= ").push_bind(v); }
    if let Some(v) = q.to { qb.push(" AND created_at <= ").push_bind(v); }
    if let Some(v) = &q.q {
        let like = format!("%{}%", v.replace('%', "\\%").replace('_', "\\_"));
        qb.push(" AND (reason ILIKE ").push_bind(like.clone()).push(" ESCAPE '\\' OR subject_id ILIKE ").push_bind(like).push(" ESCAPE '\\')");
    }
    if let Some(raw) = &q.before {
        // Validated in the handler: a malformed cursor is a 400, never a silent first page.
        let c = Cursor::decode(raw).expect("cursor validated before the query");
        let id = c.id.parse::<i64>().expect("cursor id validated before the query");
        qb.push(" AND (created_at, id) < (").push_bind(c.at).push(", ").push_bind(id).push(")");
    }
}

pub async fn modlog(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<ModlogQuery>,
) -> ApiResult<Json<Value>> {
    contribution::gate(&st, Some(&user), ACTION_REVIEW)?;
    if let Some(raw) = &q.before {
        let cursor = Cursor::decode(raw).map_err(|e| ApiError(AppError::Validation(e.to_string())))?;
        cursor.id.parse::<i64>().map_err(|_| ApiError(AppError::Validation(
            "this cursor's id is not a log position".into(),
        )))?;
    }
    let limit = q.limit.clamp(1, 200);
    let mut count = QueryBuilder::new("SELECT count(*)::bigint FROM contribution_log");
    where_clause(&mut count, &q);
    let total: i64 = count.build().fetch_one(&st.pool).await
        .map_err(AppError::from)?
        .try_get("count").map_err(AppError::from)?;

    let mut rows_q = QueryBuilder::new(
        "SELECT id, created_at, action, decision, actor_id, actor_handle, subject_type,
                subject_id, reason, gate_mode, before_state, after_state, client_hash
         FROM contribution_log",
    );
    where_clause(&mut rows_q, &q);
    rows_q.push(" ORDER BY created_at DESC, id DESC LIMIT ").push_bind(limit);
    let rows = rows_q.build().fetch_all(&st.pool).await.map_err(AppError::from)?;

    let entries = rows.into_iter().map(|r| {
        let action: String = r.get("action");
        let at: DateTime<Utc> = r.get("created_at");
        let handle: Option<String> = r.get("actor_handle");
        let kind: Option<String> = r.get("subject_type");
        let sid: Option<String> = r.get("subject_id");
        let subject = if kind.as_deref() == Some("profile") {
            LedgerSubject::Account { account: String::new(), public: sid.clone().unwrap_or_default() }
        } else {
            LedgerSubject::Object { kind: kind.clone().unwrap_or_else(|| "unknown".into()), id: sid.clone().unwrap_or_default() }
        };
        let actor = match handle {
            Some(public) => Actor::Person { account: None, public },
            None => Actor::System,
        };
        let act = polaris_ledger::Act {
            action: action.clone(), subject, actor,
            reason: r.get::<Option<String>, _>("reason"), expires_at: None, at,
        };
        let projected = polaris_ledger::project(&act, contribution::redaction_for(&action));
        let mut entry = json!({
            "id": r.get::<i64, _>("id"),
            "at": at,
            "action": projected.action,
            "decision": r.get::<String, _>("decision"),
            "subject": projected.subject,
            "subject_kind": projected.subject_kind,
            "actor": projected.actor,
            "panel": projected.panel,
            "reason": projected.reason,
            "gate_mode": r.get::<String, _>("gate_mode"),
            "before": r.get::<Option<Value>, _>("before_state"),
            "after": r.get::<Option<Value>, _>("after_state"),
        });
        if user.is_owner || user.trust_level >= 3 {
            entry["actor_id"] = json!(r.get::<Option<Uuid>, _>("actor_id"));
            entry["client_hash"] = json!(r.get::<Option<String>, _>("client_hash"));
        }
        entry
    }).collect::<Vec<_>>();

    Ok(Json(json!({ "entries": entries, "total": total, "limit": limit })))
}

#[derive(Debug, Deserialize)]
pub struct SummaryQuery {
    #[serde(default = "default_days")]
    pub days: i64,
}
fn default_days() -> i64 { 30 }

pub async fn modlog_summary(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<SummaryQuery>,
) -> ApiResult<Json<Value>> {
    contribution::gate(&st, Some(&user), ACTION_REVIEW)?;
    Ok(Json(json!({
        "days": q.days.clamp(1, 365),
        "decisions": contribution::summary(&st.pool, q.days.clamp(1, 365)).await?,
        "population_signal": contribution::population_signal(&st.pool, q.days.clamp(1, 365)).await?,
        "declaration": st.cfg.contribution.declaration,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ModerationInput {
    pub action: String,
    pub reason: String,
}

/// A moderation decision is another append-only row. A revert targets the original decision by id;
/// the row it reverses is never edited, so the history cannot be quietly rewritten.
pub async fn moderation_action(
    State(st): State<AppState>,
    CurrentUser(user): CurrentUser,
    ClientKey(client): ClientKey,
    Path(id): Path<i64>,
    Json(input): Json<ModerationInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let action = if input.action == ACTION_OVERRIDE { ACTION_OVERRIDE } else { ACTION_ACT };
    contribution::gate(&st, Some(&user), action)?;
    contribution::spend(&st, &ClientKey(client.clone()), action, Some(&user))?;

    let target: Option<(Option<Uuid>, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT actor_id, action, decision, subject_type, subject_id FROM contribution_log WHERE id = $1",
    ).bind(id).fetch_optional(&st.pool).await.map_err(AppError::from)?;
    let Some((actor_id, target_action, decision, kind, sid)) = target else {
        return Err(ApiError(AppError::NotFound));
    };
    let target_trust: i16 = match actor_id {
        Some(uid) => sqlx::query_as::<_, (i16,)>("SELECT trust_level FROM users WHERE id = $1")
            .bind(uid).fetch_optional(&st.pool).await.map_err(AppError::from)?
            .map(|t| t.0).unwrap_or(0),
        None => 0,
    };
    if !contribution::can_moderate(Some(&user), target_trust) {
        return Err(ApiError(AppError::ForbiddenWithReason(
            "a moderator may only act on contributions below their trust level".into(),
        )));
    }
    if actor_id == Some(user.id) {
        return Err(ApiError(AppError::ForbiddenWithReason(
            "you cannot moderate your own contribution".into(),
        )));
    }

    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(ApiError(AppError::Validation(
            "a moderation decision needs a reason".into(),
        )));
    }
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO contribution_log
         (action, decision, actor_id, actor_handle, subject_type, subject_id, reason,
          gate_mode, client_hash, after_state)
         VALUES ($1,'reverted',$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(action)
    .bind(user.id).bind(&user.handle)
    .bind(kind.as_deref()).bind(sid.as_deref())
    .bind(reason)
    .bind(contribution::gate_mode(&st, action))
    .bind(&client)
    .bind(json!({"reverses_id": id, "target_action": target_action, "target_decision": decision}))
    .fetch_one(&st.pool).await.map_err(AppError::from)?;
    // The ledger's projection must be constructible from what we stored. If it is not, the row is
    // still written but the public surface would lie about the actor; refuse before returning 201.
    let act = polaris_ledger::Act {
        action: action.to_string(),
        subject: LedgerSubject::Object {
            kind: kind.unwrap_or_else(|| "unknown".into()),
            id: sid.unwrap_or_default(),
        },
        actor: Actor::Person { account: Some(user.id.to_string()), public: user.handle.clone() },
        reason: Some(reason.to_string()),
        expires_at: None,
        at: Utc::now(),
    };
    let projected = polaris_ledger::project(&act, Redaction::PublishHandle);
    Ok((StatusCode::CREATED, Json(json!({
        "id": new_id, "reverses": id,
        "actor": projected.actor, "reason": projected.reason,
    }))))
}
