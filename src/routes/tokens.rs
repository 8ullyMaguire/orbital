//! API tokens, and the write API they unlock.
//!
//! Reading Orbital is anonymous: the public API needs no credential at all. Writing needs a member,
//! and requiring a browser session for that made the MCP server read-only — nobody can script their
//! own links. A token is the smallest thing that fixes it without inventing an OAuth server.
//!
//! Tokens are shown once and stored as a salted hash. A database dump cannot be replayed.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{Ctx, PageUser};
use crate::error::{ApiError, AppError};
use crate::AppState;

/// Prefix so a leaked token is recognisable in a log or a paste, the way `ghp_` is.
const PREFIX: &str = "orb_";

fn hash(salt: &str, token: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(token.as_bytes());
    format!("{:x}", h.finalize())
}

fn mint() -> String {
    // 32 bytes of entropy from the OS; `getrandom` is already in the tree via uuid.
    let bytes: [u8; 32] = rand_bytes();
    format!("{PREFIX}{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn rand_bytes() -> [u8; 32] {
    use std::io::Read;
    let mut buf = [0u8; 32];
    let mut f = std::fs::File::open("/dev/urandom").expect("urandom");
    f.read_exact(&mut buf).expect("entropy");
    buf
}

/// The member behind a `Bearer` token, if there is one.
///
/// This is an extractor rather than a middleware so a handler can be written that *requires* a
/// token and another that merely notices one.
pub struct ApiUser(pub crate::models::User);

impl axum::extract::FromRequestParts<AppState> for ApiUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        st: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|v| v.trim().to_string())
            .ok_or(AppError::Unauthorized)
        .map_err(ApiError)?;
        if !token.starts_with(PREFIX) {
            return Err(ApiError(AppError::Unauthorized));
        }
        let digest = hash(&st.cfg.salt, &token);
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT user_id FROM api_tokens WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(&digest)
        .fetch_optional(&st.pool)
        .await?;
        let (user_id,) = row.ok_or(AppError::Unauthorized)?;
        sqlx::query("UPDATE api_tokens SET last_used_at = now() WHERE token_hash = $1")
            .bind(&digest)
            .execute(&st.pool)
            .await?;
        let user = crate::store::user_by_id(&st.pool, user_id)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(ApiUser(user))
    }
}

/// `POST /api/me/tokens` — mint one. The plaintext is in the response and nowhere else.
async fn create(
    State(st): State<AppState>,
    PageUser(user): PageUser,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    let user = user.ok_or(AppError::Unauthorized)?;
    let label = body
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed")
        .chars()
        .take(60)
        .collect::<String>();
    let live: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM api_tokens WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user.id)
    .fetch_one(&st.pool)
    .await?;
    if live.0 >= 10 {
        return Err(ApiError(AppError::Validation(
            "ten live tokens is enough; revoke one first".into(),
        )));
    }
    let token = mint();
    sqlx::query("INSERT INTO api_tokens (user_id, token_hash, label) VALUES ($1, $2, $3)")
        .bind(user.id)
        .bind(hash(&st.cfg.salt, &token))
        .bind(&label)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({
        "token": token,
        "label": label,
        "note": "shown once; store it now"
    }))
    .into_response())
}

async fn list(State(st): State<AppState>, PageUser(user): PageUser) -> Result<Response, ApiError> {
    let user = user.ok_or(AppError::Unauthorized)?;
    let rows: Vec<(Uuid, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as(
            "SELECT id, label, created_at, last_used_at FROM api_tokens
              WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC",
        )
        .bind(user.id)
        .fetch_all(&st.pool)
        .await?;
    let out: Vec<_> = rows
        .into_iter()
        .map(|(id, label, created, used)| {
            json!({"id": id, "label": label, "created_at": created, "last_used_at": used})
        })
        .collect();
    Ok(Json(json!({"tokens": out})).into_response())
}

async fn revoke(
    State(st): State<AppState>,
    PageUser(user): PageUser,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let user = user.ok_or(AppError::Unauthorized)?;
    let done = sqlx::query(
        "UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user.id)
    .execute(&st.pool)
    .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError(AppError::NotFound));
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `GET /api/v1/me/links` — the member's own links, including hidden ones.
async fn my_links(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
) -> Result<Response, ApiError> {
    let rows: Vec<(String, String, bool, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT url, label, hidden, click_count, title, favicon FROM links
          WHERE user_id = $1 ORDER BY position, created_at",
    )
    .bind(user.id)
    .fetch_all(&st.pool)
    .await?;
    let out: Vec<_> = rows
        .into_iter()
        .map(|(url, label, hidden, clicks, title, favicon)| {
            json!({"url": url, "label": label, "hidden": hidden, "clicks": clicks,
                   "title": title, "favicon": favicon})
        })
        .collect();
    Ok(Json(json!({"handle": user.handle, "links": out})).into_response())
}

/// `POST /api/v1/me/links` — add one. `{"url": "...", "label": "...", "vertical": "dev"}`
async fn add_link(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    let url = body
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("url is required".into()))?;
    let url = crate::text::validate_link_url(url)?;
    let label = body
        .get("label")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::text::label_from_url(&url));
    let vertical = match body.get("vertical").and_then(|v| v.as_str()) {
        Some(slug) => {
            if crate::store::vertical_by_slug(&st.pool, slug).await?.is_none() {
                return Err(ApiError(AppError::Validation(format!("no vertical {slug}"))));
            }
            Some(slug.to_string())
        }
        None => None,
    };
    let limit: (i64,) = sqlx::query_as("SELECT count(*) FROM links WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&st.pool)
        .await?;
    if limit.0 >= 40 {
        return Err(ApiError(AppError::Validation("forty links is the ceiling".into())));
    }
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM links WHERE user_id = $1 AND url = $2")
            .bind(user.id)
            .bind(&url)
            .fetch_optional(&st.pool)
            .await?;
    if exists.is_some() {
        return Err(ApiError(AppError::Validation("that link is already on your page".into())));
    }
    let link = crate::store::create_link(
        &st.pool,
        user.id,
        &url,
        &label,
        "link",
        vertical.as_deref(),
    )
    .await?;
    // Fetch the title in the background: the API should not make the caller wait for a stranger's
    // web server, and the card looks better a moment later.
    let pool = st.pool.clone();
    let http = st.http.clone();
    let id = link.id;
    let fetch_url = url.clone();
    tokio::spawn(async move {
        if let Ok(meta) = crate::meta::page_metadata(&http, &fetch_url).await {
            let _ = sqlx::query(
                "UPDATE links SET title = $2, favicon = $3, metadata_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(meta.title)
            .bind(meta.favicon)
            .execute(&pool)
            .await;
        }
    });
    Ok((StatusCode::CREATED, Json(json!({"id": link.id, "url": url, "label": label}))).into_response())
}

/// `DELETE /api/v1/me/links?url=...` — remove one of your own.
async fn remove_link(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let url = q
        .get("url")
        .ok_or_else(|| AppError::Validation("url is required".into()))?;
    let url = crate::text::validate_link_url(url)?;
    let done = sqlx::query("DELETE FROM links WHERE user_id = $1 AND url = $2")
        .bind(user.id)
        .bind(&url)
        .execute(&st.pool)
        .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError(AppError::NotFound));
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Session-authenticated token management, for the dashboard.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/me/tokens", get(list).post(create))
        .route("/api/me/tokens/{id}/revoke", post(revoke))
}

/// Bearer-authenticated writes, for scripts and the MCP server.
pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/links", get(my_links).post(add_link))
        .route("/api/v1/me/links", delete(remove_link))
}
