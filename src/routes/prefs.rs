//! `POST /prefs` — the tuner.
//!
//! The widget is an ordinary HTML form (no JavaScript, no fetch), so this is a form post that ends
//! in a redirect back to the page the visitor was reading. Two shapes are accepted: repeated
//! `tag`/`w` pairs from the sliders, and a single `new_tag`/`new_w` from the "add a tag" row.
//!
//! The redirect target comes from `Referer` and is only honoured as a path — an absolute URL from a
//! header is an open redirect waiting to be found.

use axum::extract::State;
use axum::routing::post;
use axum::http::header;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};

use crate::auth::{Ctx, PageUser, VisitorId};
use crate::error::PageError;
use crate::prefs;
use crate::AppState;

fn back_to(headers: &HeaderMap, fallback: &str) -> String {
    headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| r.split("://").nth(1))
        .and_then(|rest| rest.find('/').map(|i| &rest[i..]))
        .filter(|p| p.starts_with('/') && !p.starts_with("//"))
        .map(|p| p.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn pairs(body: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(body.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

pub async fn apply(
    State(st): State<AppState>,
    _ctx: Ctx,
    PageUser(user): PageUser,
    VisitorId(vid): VisitorId,
    headers: HeaderMap,
    body: String,
) -> Result<Response, PageError> {
    let form = pairs(&body);
    let mut tags: Vec<String> = Vec::new();
    let mut weights: Vec<f64> = Vec::new();
    let mut new_tag = String::new();
    let mut new_w = String::new();

    for (k, v) in &form {
        match k.as_str() {
            "tag" => tags.push(v.clone()),
            "w" => weights.push(v.parse().unwrap_or(0.0)),
            "new_tag" => new_tag = v.clone(),
            "new_w" => new_w = v.clone(),
            _ => {}
        }
    }
    // The free-text row is a slider too: a tag typed without a weight is not a preference.
    if !new_tag.trim().is_empty() {
        tags.push(new_tag.clone());
        weights.push(new_w.parse().unwrap_or(5.0));
    }

    let entries: Vec<(String, f64)> = tags
        .iter()
        .zip(weights.iter())
        .map(|(t, w)| (t.clone(), *w))
        .collect();

    match &user {
        Some(u) => {
            prefs::set_explicit(&st.pool, "user", &u.id.to_string(), &entries).await?;
        }
        None => {
            let hash = prefs::visitor_hash(&vid);
            prefs::set_explicit(&st.pool, "visitor", &hash, &entries).await?;
        }
    }
    Ok(Redirect::to(&back_to(&headers, "/")).into_response())
}

pub async fn reset(
    State(st): State<AppState>,
    _ctx: Ctx,
    PageUser(user): PageUser,
    VisitorId(vid): VisitorId,
    headers: HeaderMap,
) -> Result<Response, PageError> {
    match &user {
        Some(u) => prefs::set_explicit(&st.pool, "user", &u.id.to_string(), &[]).await?,
        None => {
            let hash = prefs::visitor_hash(&vid);
            prefs::set_explicit(&st.pool, "visitor", &hash, &[]).await?
        }
    }
    Ok(Redirect::to(&back_to(&headers, "/")).into_response())
}

/// The widget's data: the viewer's current weights (as 0..10 for the sliders) and the tag corpus to
/// suggest. The corpus is the tags people actually have, so the box never proposes a tag that
/// matches nothing.
pub async fn view(st: &AppState, vid: &str, user: Option<&crate::models::User>) -> prefs::TunerView {
    let loaded = match user {
        Some(u) => prefs::load_member(&st.pool, u.id).await,
        None => prefs::load(&st.pool, vid).await,
    }
    .unwrap_or_default();
    let suggestions = crate::store::popular_tags(&st.pool, 24).await.unwrap_or_default();
    prefs::TunerView::new(&loaded, suggestions)
}

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/prefs", post(apply))
        .route("/prefs/reset", post(reset))
}
