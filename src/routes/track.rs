//! The click recorder. Every outbound link on a profile goes through here, so
//! the source of a click is a fact recorded at the moment it happens rather
//! than a number a dashboard guesses at.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::VisitorId;
use crate::error::{AppError, PageError, PageResult};
use crate::store;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/l/{id}", get(click))
}

#[derive(Deserialize)]
pub struct ClickQuery {
    #[serde(default)]
    pub s: Option<String>,
    #[serde(default)]
    pub c: Option<String>,
    #[serde(default)]
    pub p: Option<i32>,
}

fn clean_surface(s: Option<String>) -> String {
    match s.as_deref() {
        Some("vertical") => "vertical".to_string(),
        Some("embed") => "embed".to_string(),
        Some("search") => "search".to_string(),
        Some("direct") => "direct".to_string(),
        _ => "profile".to_string(),
    }
}

/// Only the host of the referrer is kept. A full Referer URL is a person's browsing history
/// attached to a click; the host is the part that answers "which network sent this", and it is
/// still a coarse fact. Ourselves are dropped: an internal navigation is not an attribution.
/// What a script cannot cheaply vary per request: its user agent and language header. Not an
/// identity and never shown as one - it groups sessions that look like one client, which is the
/// shape of a click farm rather than of a person.
fn fingerprint(salt: &str, headers: &HeaderMap) -> String {
    use sha2::{Digest, Sha256};
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let lang = headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b"|fp|");
    h.update(ua.as_bytes());
    h.update(b"|");
    h.update(lang.as_bytes());
    format!("{:x}", h.finalize())[..32].to_string()
}

fn referrer_host(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::REFERER)?.to_str().ok()?;
    let after_scheme = raw.split_once("://").map(|(_, rest)| rest).unwrap_or(raw);
    let host_port = after_scheme.split('/').next().unwrap_or("");
    let host = host_port.rsplit_once('@').map(|(_, h)| h).unwrap_or(host_port);
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    if host.is_empty() || host.len() > 100 {
        return None;
    }
    // An identifier, not a path: keep letters, digits, dots and hyphens only.
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return None;
    }
    Some(host)
}

pub async fn click(
    State(st): State<AppState>,
    VisitorId(vid): VisitorId,
    Path(id): Path<Uuid>,
    Query(q): Query<ClickQuery>,
    headers: HeaderMap,
) -> PageResult<Response> {
    let Some(link) = store::link_by_id(&st.pool, id).await? else {
        return Err(PageError(AppError::NotFound));
    };
    if link.hidden {
        return Err(PageError(AppError::NotFound));
    }
    let ctx = match q.c.as_deref() {
        Some(slug) if !slug.is_empty() => {
            // Only a vertical that exists is recorded as context; anything else
            // would be a free-text value from a URL in an analytics column.
            match store::vertical_by_slug(&st.pool, slug).await? {
                Some(v) => Some(v.slug),
                None => None,
            }
        }
        _ => None,
    };
    let hash = crate::auth::session_hash(&st.cfg.salt, &vid);
    let surface = clean_surface(q.s);
    // A failed insert must not swallow the click: the visitor still gets to the
    // destination, and the error is logged by the layer above.
    if let Err(e) = store::record_click(
        &st.pool,
        link.user_id,
        Some(link.id),
        &surface,
        ctx.as_deref(),
        q.p,
        &hash,
        referrer_host(&headers).as_deref().filter(|h| !h.ends_with("polarisocial.xyz")),
        // The hashed visitor id: this is what lets a member be told what their audience cares
        // about without any visitor id being stored in the clear.
        Some(&crate::prefs::visitor_hash(&vid)),
        Some(&fingerprint(&st.cfg.salt, &headers)),
    )
    .await
    {
        tracing::warn!(error = %e, link = %link.id, "click not recorded");
    }
    Ok(Redirect::to(&link.url).into_response())
}
