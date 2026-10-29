//! Static assets and the dashboard bundle, embedded in the binary at compile
//! time. One artefact to deploy, and no "the build directory is stale" class of
//! bug: the bytes the process serves are the bytes it was compiled from.

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;

use crate::AppState;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct Spa;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/assets/{*path}", get(asset))
        .route("/favicon.ico", get(favicon))
        .route("/app", get(|| async { axum::response::Redirect::permanent("/app/") }))
        .route("/app/", get(spa_index))
        .route("/app/{*path}", get(spa_file))
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn serve(bytes: rust_embed::EmbeddedFile, path: &str) -> Response {
    let immutable = path.starts_with("assets/") && path.len() > 12 && path.contains('-');
    let cache = if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300"
    };
    (
        [
            (header::CONTENT_TYPE, content_type(path)),
            (header::CACHE_CONTROL, cache),
        ],
        bytes.data.into_owned(),
    )
        .into_response()
}

async fn asset(Path(path): Path<String>) -> Response {
    match Assets::get(&path) {
        Some(f) => serve(f, &path),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn favicon() -> Response {
    match Assets::get("favicon.svg") {
        Some(f) => serve(f, "favicon.svg"),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn spa_index() -> Response {
    match Spa::get("index.html") {
        Some(f) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            f.data.into_owned(),
        )
            .into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "dashboard bundle missing: run `node node_modules/vite/bin/vite.js build` in web/",
        )
            .into_response(),
    }
}

async fn spa_file(Path(path): Path<String>) -> Response {
    if let Some(f) = Spa::get(&path) {
        let immutable = path.starts_with("assets/");
        let cache = if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "public, max-age=60"
        };
        return (
            [
                (header::CONTENT_TYPE, content_type(&path)),
                (header::CACHE_CONTROL, cache),
            ],
            f.data.into_owned(),
        )
            .into_response();
    }
    // A missing chunk must be a 404, not the shell: a browser handed HTML for a
    // .js request fails with a syntax error that names nothing useful.
    if path.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    spa_index().await
}
