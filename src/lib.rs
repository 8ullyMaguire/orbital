pub mod auth;
pub mod bot;
pub mod config;
pub mod db;
pub mod error;
pub mod health;
pub mod governance;
pub mod html;
pub mod meta;
pub mod models;
pub mod og;
pub mod ops;
pub mod prefs;
pub mod ranking;
pub mod render;
pub mod routes;
pub mod seed;
pub mod store;
pub mod text;

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{HeaderValue, SET_COOKIE};
use axum::http::HeaderName;
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use sqlx::PgPool;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

use crate::auth::VisitorId;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    pub http: reqwest::Client,
}

pub async fn build_state(cfg: Config) -> anyhow::Result<AppState> {
    let pool = db::connect(&cfg).await?;
    let http = reqwest::Client::builder()
        .user_agent("orbital/0.1 (+https://orbital.polarisocial.xyz)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    Ok(AppState {
        pool,
        cfg: Arc::new(cfg),
        http,
    })
}

pub async fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::public::router())
        .merge(routes::estate::router())
        .merge(routes::api::router())
        .merge(routes::apiv1::router())
        .merge(routes::track::router())
        .merge(routes::prefs::router())
        .merge(routes::assets::router())
        .route("/healthz", axum::routing::get(healthz))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            visitor_middleware,
        ))
        .layer(axum::middleware::from_fn(headers_middleware))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

/// Mints the visitor id on a read but writes NO row: the id only becomes a
/// record when a view or a click records it. (A middleware that writes on
/// every read is how a crawler turns a page into a thousand sessions.)
async fn visitor_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let existing = auth::cookie_value(req.headers(), &state.cfg.visitor_cookie);
    let fresh = existing.is_none();
    let vid = existing.unwrap_or_else(auth::new_visitor_id);
    req.extensions_mut().insert(VisitorId(vid.clone()));

    // The read API answers the same bytes to everybody, so it must not hand out an identity: a
    // `Set-Cookie` makes a response uncacheable at nginx and at Cloudflare (measured: `cf-cache-
    // status: DYNAMIC` while the response said `max-age=60`), which turned every scrape into an
    // origin hit and made the advertised cache lifetime a lie. Nothing under /api/v1 counts a view
    // or a click, so there is nothing for the id to do there.
    let api_read = req.uri().path().starts_with("/api/v1/");

    let mut res = next.run(req).await;
    if fresh && !api_read {
        res.headers_mut()
            .append(SET_COOKIE, auth::visitor_cookie(&state.cfg, &vid));
    }
    res
}

/// Headers a page cannot forge and a segfault cannot skip.
///
/// There used to be an `X-Robots-Tag: noindex` branch here for the adult hostname. That surface is
/// retired (see DECISIONS D2): the directory holds links, not media, so there is no second
/// surface to keep out of an index. The remaining headers are unconditional, which is the point —
/// a header that depends on a host comparison is a header that can silently stop being sent.
async fn headers_middleware(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    res
}
