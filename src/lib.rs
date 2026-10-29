pub mod auth;
pub mod bot;
pub mod config;
pub mod db;
pub mod error;
pub mod html;
pub mod models;
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
use axum::response::{IntoResponse, Response};
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
        .merge(routes::api::router())
        .merge(routes::track::router())
        .merge(routes::assets::router())
        .route("/healthz", axum::routing::get(healthz))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            visitor_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            nsfw_gate_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            headers_middleware,
        ))
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
    let mut res = next.run(req).await;
    if fresh {
        res.headers_mut()
            .append(SET_COOKIE, auth::visitor_cookie(&state.cfg, &vid));
    }
    res
}

/// The adult host is gated at the edge of the router rather than in each
/// handler: a page added later cannot forget the check, because there is
/// nothing to forget.
async fn nsfw_gate_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let host = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let is_nsfw = host == state.cfg.nsfw_host.to_lowercase() || host == "nsfw.localhost";
    if is_nsfw {
        let path = req.uri().path().to_string();
        let exempt = path == "/gate"
            || path == "/robots.txt"
            || path == "/favicon.ico"
            || path.starts_with("/assets/");
        let ok = auth::cookie_value(req.headers(), "orbital_age").as_deref() == Some("1");
        if !exempt && !ok {
            let next_path = if path.starts_with('/') { path } else { "/".into() };
            let target = format!("/gate?next={}", crate::render::urlencode(&next_path));
            return axum::response::Redirect::to(&target).into_response();
        }
    }
    next.run(req).await
}

/// Headers a page cannot forge and a segfault cannot skip.
///
/// The adult check reads the configured host, not a `nsfw.` prefix. The prefix
/// version silently skipped every hostname that did not start with those five
/// characters, so `noindex` was never sent on the adult surface while the pages
/// looked gated — a crawler would have indexed the directory.
async fn headers_middleware(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let hostname = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let is_nsfw =
        hostname == state.cfg.nsfw_host.to_lowercase() || hostname == "nsfw.localhost";
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
    if is_nsfw {
        h.insert(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow, noarchive"),
        );
    }
    res
}
