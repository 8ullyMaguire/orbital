//! The click recorder. Every outbound link on a profile goes through here, so
//! the source of a click is a fact recorded at the moment it happens rather
//! than a number a dashboard guesses at.

use axum::extract::{Path, Query, State};
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

pub async fn click(
    State(st): State<AppState>,
    VisitorId(vid): VisitorId,
    Path(id): Path<Uuid>,
    Query(q): Query<ClickQuery>,
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
    )
    .await
    {
        tracing::warn!(error = %e, link = %link.id, "click not recorded");
    }
    Ok(Redirect::to(&link.url).into_response())
}
