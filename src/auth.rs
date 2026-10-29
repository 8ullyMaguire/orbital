use std::sync::Arc;

use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::Config;
use crate::error::{ApiError, AppError, PageError};
use crate::models::User;
use crate::AppState;

pub fn hash_password(pw: &str) -> Result<String, AppError> {
    if pw.chars().count() < 8 {
        return Err(AppError::Validation(
            "a password is at least 8 characters".to_string(),
        ));
    }
    if pw.chars().count() > 200 {
        return Err(AppError::Validation("that password is too long".to_string()));
    }
    let mut raw = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut raw);
    // encode_b64 rather than SaltString::generate(OsRng): the latter needs the
    // `rand_core` feature on password-hash, and a build-time feature is not
    // where a login should fail.
    let salt = SaltString::encode_b64(&raw)
        .map_err(|e| AppError::Other(anyhow::anyhow!("salt: {e}")))?;
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Other(anyhow::anyhow!("hash: {e}")))
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(pw.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

pub fn new_token() -> String {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    hex::encode(raw)
}

pub fn new_visitor_id() -> String {
    let mut raw = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut raw);
    hex::encode(raw)
}

/// What is stored per click: a salted hash, never the cookie itself, so the
/// table cannot be turned back into browsing identities.
pub fn session_hash(salt: &str, visitor: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(visitor.as_bytes());
    hex::encode(h.finalize())
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let joined = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect::<Vec<_>>()
        .join("; ");
    for part in joined.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

pub fn session_cookie(cfg: &Config, token: &str, ttl_days: i64) -> HeaderValue {
    let secure = if cfg.secure_cookies { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}={}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{}",
        cfg.session_cookie,
        token,
        ttl_days * 86_400,
        secure
    ))
    .expect("cookie header is ascii")
}

pub fn clear_session_cookie(cfg: &Config) -> HeaderValue {
    let secure = if cfg.secure_cookies { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}",
        cfg.session_cookie, secure
    ))
    .expect("cookie header is ascii")
}

pub fn visitor_cookie(cfg: &Config, vid: &str) -> HeaderValue {
    let secure = if cfg.secure_cookies { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{}={}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax{}",
        cfg.visitor_cookie, vid, secure
    ))
    .expect("cookie header is ascii")
}

/// The visitor id, minted by the middleware for this request. No row is written
/// on a read; the id only becomes a row when a view or a click records it.
#[derive(Clone, Debug)]
pub struct VisitorId(pub String);

impl axum::extract::FromRequestParts<AppState> for VisitorId {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<VisitorId>().cloned().ok_or_else(|| {
            ApiError(AppError::Other(anyhow::anyhow!(
                "visitor middleware did not run; the layer is missing from the router"
            )))
        })
    }
}

#[derive(Clone, Debug)]
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match session_user(parts, state).await? {
            Some(u) => Ok(CurrentUser(u)),
            None => Err(ApiError(AppError::Unauthorized)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MaybeUser(pub Option<User>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(MaybeUser(session_user(parts, state).await?))
    }
}

async fn session_user(parts: &Parts, state: &AppState) -> Result<Option<User>, ApiError> {
    let Some(token) = cookie_value(&parts.headers, &state.cfg.session_cookie) else {
        return Ok(None);
    };
    Ok(crate::store::user_by_session(&state.pool, &token).await?)
}

/// Same as `MaybeUser`, for page handlers: an unauthorized rejection renders
/// HTML instead of JSON.
pub struct PageUser(pub Option<User>);

impl FromRequestParts<AppState> for PageUser {
    type Rejection = PageError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let Some(token) = cookie_value(&parts.headers, &state.cfg.session_cookie) else {
            return Ok(PageUser(None));
        };
        Ok(PageUser(
            crate::store::user_by_session(&state.pool, &token).await?,
        ))
    }
}

/// Request context. There used to be an `is_nsfw` here, derived from the Host header, because
/// adult listings answered on a second hostname. That surface is retired: the platform stores
/// links and labels, never adult media, so `nsfw` is a vertical and the destination site owns age
/// verification. `base` is the one public origin.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub base: String,
    pub hub_handle: String,
    /// Decided per request from the cookie and `Accept-Language`, so no handler has to think
    /// about it and no template can disagree with another on the same page.
    pub lang: crate::i18n::Lang,
}

impl Ctx {
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }
}

impl FromRequestParts<AppState> for Ctx {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        // The full Host header (with port, in dev) is what a browser used, so
        // it is what an absolute URL must carry; only the hostname decides
        // which surface answered.
        let host = parts
            .headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let scheme = parts
            .headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(if state.cfg.public_base.starts_with("https") {
                "https"
            } else {
                "http"
            });
        let base = if host.is_empty() {
            state.cfg.public_base.clone()
        } else {
            format!("{scheme}://{host}")
        };
        Ok(Ctx {
            base,
            hub_handle: state.cfg.hub_handle.clone(),
            lang: crate::i18n::Lang::from_headers(&parts.headers),
        })
    }
}

pub fn require_owner(user: &User) -> Result<(), AppError> {
    if user.is_owner || user.trust_level >= 3 {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub type SharedConfig = Arc<Config>;
pub type UserId = Uuid;
