//! The estate's own front door: a status page for the network, and the social cards.
//!
//! Both are things a visitor asks for and neither needs a session: "is this network alive" and "what
//! does a link to it look like".

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::PageError;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct HostStatus {
    pub host: String,
    pub label: String,
    pub up: bool,
    pub status: Option<u16>,
    pub millis: u128,
}

/// A hostname the estate publishes, as listed in the estate file the deploy writes from nginx.
fn estate_file() -> std::path::PathBuf {
    std::env::var("ORBITAL_ESTATE_FILE")
        .map(Into::into)
        .unwrap_or_else(|_| "static/estate.txt".into())
}

fn read_estate() -> Vec<(String, String)> {
    let Ok(raw) = std::fs::read_to_string(estate_file()) else {
        return Vec::new();
    };
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_once('|') {
            Some((host, label)) => (host.trim().to_string(), label.trim().to_string()),
            None => (l.to_string(), l.to_string()),
        })
        .collect()
}

/// Probing nineteen hosts on every page view would be a self-inflicted load test, so the answer is
/// cached for a minute. A status page that is a minute stale is still a status page.
static CACHE: Mutex<Option<(Instant, Vec<HostStatus>)>> = Mutex::new(None);

async fn probe_all() -> Vec<HostStatus> {
    if let Ok(guard) = CACHE.lock() {
        if let Some((at, rows)) = guard.as_ref() {
            if at.elapsed() < Duration::from_secs(60) {
                return rows.clone();
            }
        }
    }
    let hosts = read_estate();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .user_agent("orbital-status/1")
        .build()
        .unwrap_or_default();

    let mut set = tokio::task::JoinSet::new();
    for (host, label) in hosts {
        let client = client.clone();
        set.spawn(async move {
            let started = Instant::now();
            let res = client
                .get(format!("https://{host}/"))
                .send()
                .await
                .map(|r| r.status().as_u16())
                .ok();
            HostStatus {
                up: res.map(|s| s < 500).unwrap_or(false),
                status: res,
                millis: started.elapsed().as_millis(),
                host,
                label,
            }
        });
    }
    let mut rows = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(row) = joined {
            rows.push(row);
        }
    }
    rows.sort_by_key(|a| (!a.up, a.host.clone()));
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((Instant::now(), rows.clone()));
    }
    rows
}

pub async fn status(State(st): State<AppState>) -> Result<Response, PageError> {
    use askama::Template;
    let hosts = probe_all().await;
    let up = hosts.iter().filter(|h| h.up).count();
    let verticals = crate::store::verticals(&st.pool, true).await.unwrap_or_default();
    let page = crate::render::StatusTemplate {
        layout: crate::render::Layout::new(&st.cfg.public_base, false, &verticals, None),
        total: hosts.len(),
        up,
        generated: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        hosts: hosts
            .into_iter()
            .map(|h| crate::render::HostStatusRow {
                status: h
                    .status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                host: h.host,
                label: h.label,
                up: h.up,
                millis: h.millis,
            })
            .collect(),
    };
    let html = page.render().map_err(|e| PageError(crate::error::AppError::Other(anyhow::anyhow!(e.to_string()))))?;
    let mut res = html.into_response();
    // A status page is the one page that must never be cached by a proxy.
    res.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(res)
}

/// `/og/{file}` — the generated social card, from disk when it exists and from the database when it
/// does not. A card that 404s is worse than no card at all.
pub async fn og(State(st): State<AppState>, Path(file): Path<String>) -> Result<Response, PageError> {
    let handle = file
        .trim_end_matches(".png")
        .trim_end_matches(".svg")
        .to_string();
    if handle.is_empty()
        || !handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(PageError(crate::error::AppError::NotFound));
    }
    let dir = std::env::var("ORBITAL_OG_DIR").unwrap_or_else(|_| "static/og".into());
    let want_png = file.ends_with(".png");
    let path = format!("{dir}/{handle}.{}", if want_png { "png" } else { "svg" });
    if let Ok(bytes) = std::fs::read(&path) {
        let ctype = if want_png { "image/png" } else { "image/svg+xml" };
        let mut res = bytes.into_response();
        res.headers_mut()
            .insert(header::CONTENT_TYPE, ctype.parse().unwrap());
        res.headers_mut().insert(
            header::CACHE_CONTROL,
            "public, max-age=3600".parse().unwrap(),
        );
        return Ok(res);
    }

    // Not generated yet: render it now so the first share is not the broken one.
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT handle, COALESCE(display_name, handle), (SELECT count(*) FROM links l
            WHERE l.user_id = users.id AND l.hidden = false)
           FROM users WHERE handle = $1 AND hidden = false",
    )
    .bind(&handle)
    .fetch_optional(&st.pool)
    .await?;
    let Some((handle, name, links)) = row else {
        return Err(PageError(crate::error::AppError::NotFound));
    };
    let svg = crate::og::card(&name, &handle, links, &st.cfg.public_base);
    let mut res = svg.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        if want_png {
            // The bytes are SVG: say so rather than serving an image/png that is not one.
            "image/svg+xml"
        } else {
            "image/svg+xml"
        }
        .parse()
        .unwrap(),
    );
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        "public, max-age=300".parse().unwrap(),
    );
    Ok(res)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/og/{file}", get(og))
}
