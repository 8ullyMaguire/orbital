//! Integration tests over the real HTTP surface against a real Postgres.
//!
//! `sqlx::test` creates a fresh database per test and applies `migrations/`,
//! so these exercise the schema the deployed binary will meet — not a fixture
//! shaped like it.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::routes;
use orbital::AppState;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn test_config() -> Config {
    Config::for_test("http://orbital.test")
}

async fn app(pool: PgPool) -> Router {
    let cfg = test_config();
    let http = reqwest::Client::new();
    let state = AppState {
        pool,
        cfg: std::sync::Arc::new(cfg),
        http,
    };
    orbital::build_router(state).await
}

/// The same router, with a config an operator could have written.
async fn app_with(pool: PgPool, cfg: Config) -> Router {
    let http = reqwest::Client::new();
    let state = AppState {
        pool,
        cfg: std::sync::Arc::new(cfg),
        http,
    };
    orbital::build_router(state).await
}

struct Res {
    status: StatusCode,
    body: String,
    cookie: Option<String>,
    robots: Option<String>,
    /// Where a redirect points. A webring is a promise about where the next click goes, so the
    /// test has to read the header rather than the (empty) body.
    location: Option<String>,
    etag: Option<String>,
    cors: Option<String>,
    /// The anonymous visitor id: the thing a preference profile is keyed to.
    vcookie: Option<String>,
    /// The `orbital_off` flag, when the response sets one. Browsers keep this; a test must too.
    offcookie: Option<String>,
}

impl Res {
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

async fn call(app: &Router, req: Request<Body>) -> Res {
    let res = app.clone().oneshot(req).await.expect("router responded");
    let status = res.status();
    let cookie = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|c| c.starts_with("orbital_s="))
        .map(|c| c.split(';').next().unwrap_or("").to_string());
    let location = res
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let etag = res
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let cors = res
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let robots = res
        .headers()
        .get("x-robots-tag")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let vcookie = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|c| c.starts_with("orbital_v="))
        .map(|c| c.split(';').next().unwrap_or("").to_string());
    let offcookie = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|c| c.starts_with("orbital_off"))
        .map(|c| c.split(';').next().unwrap_or("").to_string());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Res {
        status,
        body: String::from_utf8_lossy(&bytes).to_string(),
        cookie,
        robots,
        location,
        etag,
        cors,
        vcookie,
        offcookie,
    }
}

fn get(path: &str) -> Request<Body> {
    Request::builder().uri(path).body(Body::empty()).unwrap()
}

fn host(mut req: Request<Body>, h: &str) -> Request<Body> {
    req.headers_mut().insert("host", h.parse().unwrap());
    req
}

fn cookie(mut req: Request<Body>, c: &str) -> Request<Body> {
    req.headers_mut().insert("cookie", c.parse().unwrap());
    req
}

fn json_req(method: &str, path: &str, body: serde_json::Value, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    b.body(Body::from(body.to_string())).unwrap()
}

async fn register(app: &Router, handle: &str, vertical: &str) -> Res {
    call(
        app,
        json_req(
            "POST",
            "/api/auth/register",
            serde_json::json!({
                "handle": handle,
                "password": "a long enough password",
                "display_name": handle,
                "vertical": vertical,
                "bio": format!("{handle} makes things."),
            }),
            None,
        ),
    )
    .await
}

#[sqlx::test]
async fn the_front_door_renders_with_its_verticals(pool: PgPool) {
    let app = app(pool).await;
    let res = call(&app, get("/")).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert!(res.body.contains("Verticals"), "the nav must list the verticals");
    assert!(res.body.contains("/v/dev"), "the seeded verticals must be linked");
    assert!(res.body.contains("og:image"), "social cards need an og:image");
}

#[sqlx::test]
async fn register_then_login_yields_a_working_session(pool: PgPool) {
    let app = app(pool).await;
    let reg = register(&app, "alice", "dev").await;
    assert_eq!(reg.status, StatusCode::OK, "{}", reg.body);
    let sess = reg.cookie.expect("register must set a session cookie");
    assert!(sess.starts_with("orbital_s="));

    let me = call(&app, cookie(get("/api/me"), &sess)).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    assert_eq!(me.json()["user"]["handle"], "alice");

    // The same account can sign in fresh, and a wrong password is refused
    // without revealing whether the handle exists.
    let bad = call(
        &app,
        json_req(
            "POST",
            "/api/auth/login",
            serde_json::json!({"handle": "alice", "password": "wrong password"}),
            None,
        ),
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    let good = call(
        &app,
        json_req(
            "POST",
            "/api/auth/login",
            serde_json::json!({"handle": "alice", "password": "a long enough password"}),
            None,
        ),
    )
    .await;
    assert_eq!(good.status, StatusCode::OK, "{}", good.body);
    assert!(good.cookie.is_some());
}

#[sqlx::test]
async fn a_click_is_attributed_and_then_ranked(pool: PgPool) {
    let app = app(pool).await;
    let reg = register(&app, "worker", "dev").await;
    let sess = reg.cookie.clone().unwrap();

    let created = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "https://example.com/work", "label": "My work"}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let link_id = created.json()["link"]["id"].as_str().unwrap().to_string();

    let profile = call(&app, get("/@worker")).await;
    assert_eq!(profile.status, StatusCode::OK);
    // Source-tagged: the outbound URL must be the tracker, never the target.
    assert!(profile.body.contains(&format!("/l/{link_id}?")), "link must be tracked");
    assert!(!profile.body.contains("href=\"https://example.com/work\""), "raw target leaked");

    let click = call(&app, cookie(get(&format!("/l/{link_id}?s=vertical&c=dev&p=1")), "orbital_v=v1")).await;
    assert_eq!(click.status, StatusCode::SEE_OTHER, "{}", click.body);

    let an = call(&app, cookie(get("/api/analytics?days=7"), &sess)).await;
    assert_eq!(an.status, StatusCode::OK, "{}", an.body);
    assert_eq!(an.json()["totals"]["clicks"], 1, "{}", an.body);

    let board = call(&app, get("/v/dev?tab=top")).await;
    assert!(board.body.contains("worker"), "the member must be on the leaderboard");
    let trending = call(&app, get("/v/dev?tab=trending")).await;
    assert!(trending.body.contains("worker"), "one recent visitor is enough to trend");
}

#[sqlx::test]
async fn one_browser_counts_once_and_two_browsers_count_twice(pool: PgPool) {
    let app = app(pool).await;
    let reg = register(&app, "counter", "dev").await;
    let sess = reg.cookie.clone().unwrap();
    let created = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "https://example.com/one"}),
            Some(&sess),
        ),
    )
    .await;
    let id = created.json()["link"]["id"].as_str().unwrap().to_string();

    for _ in 0..5 {
        let r = call(&app, cookie(get(&format!("/l/{id}?s=profile")), "orbital_v=same-browser")).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER);
    }
    let one = call(&app, cookie(get("/api/analytics?days=7"), &sess)).await;
    assert_eq!(one.json()["totals"]["clicks"], 5, "every click is recorded");
    assert_eq!(one.json()["totals"]["visitors"], 1, "one browser is one visitor");

    let _ = call(&app, cookie(get(&format!("/l/{id}?s=profile")), "orbital_v=other-browser")).await;
    let two = call(&app, cookie(get("/api/analytics?days=7"), &sess)).await;
    assert_eq!(two.json()["totals"]["visitors"], 2, "a second browser is a second visitor");
}

#[sqlx::test]
async fn trending_is_hidden_rather_than_padded_when_nobody_has_traffic(pool: PgPool) {
    let app = app(pool).await;
    let _ = register(&app, "quiet", "dev").await;
    let res = call(&app, get("/v/dev?tab=trending")).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(
        res.body.contains("No traffic measured yet"),
        "an empty trending tab must say so: {}",
        res.body
    );
    assert!(!res.body.contains("class=\"row\""), "no zero rows behind a rising label");
}

#[sqlx::test]
async fn an_adult_vertical_is_an_ordinary_vertical(pool: PgPool) {
    // The platform stores links and labels, never adult media, so there is no second surface and
    // no gate: `nsfw` is a category like `gaming`, and the destination site owns age verification.
    // This test used to assert the opposite (404 on the "SFW host", a gate redirect, an
    // X-Robots-Tag), so it is the guard against the wall being rebuilt by accident.
    let app = app(pool).await;
    let reg = register(&app, "adult", "nsfw").await;
    assert_eq!(reg.status, StatusCode::OK, "{}", reg.body);
    let sess = reg.cookie.clone().unwrap();
    let created = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "https://example.org/adult-link"}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);

    // The profile renders, on the one host, with its links - a TL0 account included, because
    // trust no longer decides who the directory admits.
    let profile = call(&app, get("/@adult")).await;
    assert_eq!(
        profile.status,
        StatusCode::OK,
        "an adult profile must render like any other: {}",
        profile.body
    );
    assert!(profile.body.contains("example.org/adult-link"), "its links render too");
    assert!(
        profile.robots.is_none(),
        "no page carries X-Robots-Tag any more, got {:?}",
        profile.robots
    );

    // The vertical has a page, and it is listed in the index.
    let vertical = call(&app, get("/v/nsfw")).await;
    assert_eq!(vertical.status, StatusCode::OK, "{}", vertical.body);

    // There is nothing to acknowledge, so the gate is gone rather than merely unlinked.
    let gate = call(&app, host(get("/gate?next=%2F%40adult"), "orbital.test")).await;
    assert_eq!(gate.status, StatusCode::NOT_FOUND, "the gate route is retired");

    // A second hostname is not a second surface: the old adult hostname serves the same page.
    let old_host = call(&app, host(get("/@adult"), "orbitalnsfw.test")).await;
    assert_eq!(
        old_host.status,
        StatusCode::OK,
        "the retired hostname must not behave differently"
    );

    // And the sitemap lists it, because the directory is one directory.
    let map = call(&app, get("/sitemap.xml")).await;
    assert!(map.body.contains("/@adult"), "{}", map.body);
}

#[sqlx::test]
async fn a_link_may_not_carry_credentials_or_a_foreign_scheme(pool: PgPool) {
    let app = app(pool).await;
    let sess = register(&app, "guard", "dev").await.cookie.unwrap();
    for bad in [
        "https://user:pw@example.com/x",
        "javascript:alert(1)",
        "ftp://example.com/x",
    ] {
        let res = call(
            &app,
            json_req("POST", "/api/links", serde_json::json!({"url": bad}), Some(&sess)),
        )
        .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{bad} was accepted");
    }
    let ok = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "example.com/no-scheme"}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.body);
    assert_eq!(ok.json()["link"]["url"], "https://example.com/no-scheme");
}

#[sqlx::test]
async fn a_profile_cannot_belong_to_three_secondaries(pool: PgPool) {
    let app = app(pool).await;
    let sess = register(&app, "greedy", "dev").await.cookie.unwrap();
    let res = call(
        &app,
        json_req(
            "PATCH",
            "/api/me",
            serde_json::json!({
                "primary_vertical": "dev",
                "secondary_verticals": ["art", "music", "writers", "gaming"]
            }),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let me = call(&app, cookie(get("/api/me"), &sess)).await;
    let n = me.json()["verticals"].as_array().unwrap().len();
    assert_eq!(n, 3, "one primary plus at most two secondaries, got {n}");
}

#[sqlx::test]
async fn the_embed_widget_is_self_contained_and_links_back(pool: PgPool) {
    let app = app(pool).await;
    let _ = register(&app, "embedded", "dev").await;
    let res = call(&app, get("/v/dev/embed?view=top&count=5")).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.contains("embedded"), "{}", res.body);
    assert!(res.body.contains("app/#/join"), "the widget must carry the join CTA");
    assert!(!res.body.contains("class=\"top\""), "an embed is not the site chrome");
}

#[sqlx::test]
async fn every_vertical_is_listed_on_one_surface(pool: PgPool) {
    // Two members that used to be told apart: one SFW-primary with an adult secondary, one
    // adult-primary with a SFW secondary. Both belong to the same directory now, and the
    // profile shows every vertical a member chose, because choosing one is not a secret.
    let app = app(pool.clone()).await;

    let sess = register(&app, "dual", "dev").await.cookie.unwrap();
    let patched = call(
        &app,
        json_req(
            "PATCH",
            "/api/me",
            serde_json::json!({"primary_vertical": "dev", "secondary_verticals": ["nsfw"]}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);

    let board = call(&app, get("/v/dev")).await;
    assert!(board.body.contains("dual"), "a dev-primary member ranks in dev");

    let card = call(&app, get("/@dual")).await;
    assert!(
        card.body.contains("/v/nsfw"),
        "the profile states every vertical it holds, adult secondary included: {}",
        card.body
    );

    let _s2 = register(&app, "shadow", "nsfw").await.cookie.unwrap();
    let people = call(&app, get("/")).await;
    assert!(people.body.contains("shadow"), "the front door lists every profile");
    let search = call(&app, get("/search?q=shadow")).await;
    assert!(search.body.contains("shadow"), "and search finds them");
}

#[sqlx::test]
async fn every_route_in_the_inventory_answers(pool: PgPool) {
    let app = app(pool.clone()).await;
    let sess = register(&app, "router", "dev").await.cookie.unwrap();
    // Privilege is a column plus a flag in this schema; promote the fixture the
    // way the operator would, so the admin routes are exercised for real.
    sqlx::query("UPDATE users SET is_owner = true, trust_level = 3 WHERE handle = 'router'")
        .execute(&pool)
        .await
        .unwrap();
    let link = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "https://example.com/r"}),
            Some(&sess),
        ),
    )
    .await;
    let id = link.json()["link"]["id"].as_str().unwrap().to_string();

    let routes = vec![
        "/", "/about", "/search?q=router", "/v/dev", "/v/dev/embed",
        "/v/dev/feed.json", "/feed.json", "/feed.xml", "/sitemap.xml",
        "/robots.txt", "/app/", "/assets/site.css", "/healthz", "/@router",
        "/api/me", "/api/verticals", "/api/links", "/api/analytics",
        "/api/admin/overview", "/api/admin/flags", "/api/admin/users",
    ];
    for r in routes {
        let res = call(&app, cookie(get(r), &sess)).await;
        assert!(
            res.status.is_success(),
            "{r} answered {}: {}",
            res.status,
            &res.body[..res.body.len().min(200)]
        );
    }
    let click = call(&app, cookie(get(&format!("/l/{id}?s=profile")), "orbital_v=x")).await;
    assert!(click.status.is_redirection(), "click route: {}", click.status);
}

#[sqlx::test]
async fn the_dashboard_bundle_is_served_from_the_binary(pool: PgPool) {
    let app = app(pool).await;
    let shell = call(&app, get("/app/")).await;
    assert_eq!(shell.status, StatusCode::OK);
    assert!(shell.body.contains("app"), "{}", shell.body);
    // A missing chunk is a 404, never the shell with a JavaScript content type.
    let missing = call(&app, get("/app/nope.js")).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn the_router_registers_every_handlers_path(pool: PgPool) {
    // Guard against a handler that exists but was never wired: the inventory is
    // built from real requests, so a missing registration is a 404 here.
    let app = app(pool).await;
    let res = call(&app, get("/api/verticals")).await;
    assert_eq!(res.status, StatusCode::OK);
    let verticals = res.json()["verticals"].as_array().unwrap().len();
    assert_eq!(verticals, 9, "nine SFW starter verticals, got {verticals}");
    let _ = routes::public::router;
}

#[sqlx::test]
async fn a_click_remembers_which_network_sent_it(pool: PgPool) {
    // The dashboard could say which of our pages earned a click and nothing about where the
    // visitor came from, so "did Bluesky or Mastodon work" was unanswerable.
    let app = app(pool).await;
    let sess = register(&app, "networker", "dev").await.cookie.unwrap();
    let created = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "https://example.org/from-bsky"}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let id = created.json()["link"]["id"].as_str().unwrap().to_string();

    // A click that arrives from a timeline: the href is ours, the referrer is theirs.
    let mut req = get(&format!("/l/{id}?s=profile"));
    req.headers_mut()
        .insert("referer", "https://bsky.app/profile/criticalnexus.bsky.social/post/3k".parse().unwrap());
    let click = call(&app, req).await;
    assert!(click.status.is_redirection(), "{}", click.status);

    // A click from one of our own pages is not an attribution: it must not appear as a network.
    let mut internal = get(&format!("/l/{id}?s=vertical"));
    internal.headers_mut()
        .insert("referer", "https://orbital.polarisocial.xyz/v/dev".parse().unwrap());
    call(&app, internal).await;

    let an = call(&app, cookie(get("/api/analytics?days=7"), &sess)).await;
    assert_eq!(an.status, StatusCode::OK, "{}", an.body);
    let refs = an.json()["by_referrer"].as_array().cloned().unwrap_or_default();
    let keys: Vec<String> = refs.iter().filter_map(|r| r["key"].as_str().map(String::from)).collect();
    assert!(keys.contains(&"bsky.app".to_string()), "by_referrer = {keys:?}");
    assert!(
        !keys.iter().any(|k| k.ends_with("polarisocial.xyz")),
        "an internal navigation must not count as a network: {keys:?}"
    );
}

#[sqlx::test]
async fn the_vertical_ring_is_walkable_by_url(pool: PgPool) {
    // The profile page has had prev/next for a while, but only while you are on a profile. The
    // addressable form is what makes the ring linkable - and what gives the members ranked 40+ a
    // route to a visitor that does not depend on being near the top of a leaderboard.
    let app = app(pool).await;
    for h in ["alpha", "bravo", "charlie"] {
        register(&app, h, "dev").await;
    }

    let first = call(&app, get("/v/dev/next")).await;
    assert!(first.status.is_redirection(), "{}", first.status);
    let location = first.location.clone().unwrap_or_default();
    assert!(
        location.starts_with("/@"),
        "must redirect to a profile, got {location:?}"
    );

    // Walking from any member lands on a different member, and the walk wraps.
    let from_alpha = call(&app, get("/v/dev/next?from=alpha")).await;
    let from_bravo = call(&app, get("/v/dev/next?from=bravo")).await;
    assert!(from_alpha.status.is_redirection() && from_bravo.status.is_redirection());
    assert_ne!(
        from_alpha.location, from_bravo.location,
        "walking from two different members must not land on the same profile"
    );
    assert_ne!(
        from_alpha.location.as_deref(),
        Some("/@alpha"),
        "the ring must move: a next that returns you to yourself is not a walk"
    );

    let unknown = call(&app, get("/v/dev/next?from=nobody")).await;
    assert!(unknown.status.is_redirection(), "an unknown `from` starts at the top");

    let missing = call(&app, get("/v/nope/next")).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND, "no such vertical is a 404");
}

#[sqlx::test]
async fn the_public_api_shows_what_the_page_shows(pool: PgPool) {
    // An API that answers more than the page is a leak with a version number on it. This asserts
    // the parity that matters: the same member, the same links, the same health verdict the card
    // on the profile carries.
    let app = app(pool.clone()).await;
    let sess = register(&app, "apiuser", "dev").await.cookie.unwrap();
    let created = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "https://example.org/api-visible"}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let id = created.json()["link"]["id"].as_str().unwrap().to_string();

    let stats = call(&app, get("/api/v1/stats")).await;
    assert_eq!(stats.status, StatusCode::OK, "{}", stats.body);
    assert!(stats.json()["members"].as_i64().unwrap() >= 1);
    assert!(stats.json()["links"]["total"].as_i64().unwrap() >= 1);

    let verticals = call(&app, get("/api/v1/verticals")).await;
    assert!(verticals.body.contains("\"slug\":\"dev\"") || verticals.body.contains("\"slug\": \"dev\""));

    let one = call(&app, get("/api/v1/members/apiuser")).await;
    assert_eq!(one.status, StatusCode::OK, "{}", one.body);
    let links = one.json()["links"].as_array().cloned().unwrap_or_default();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["url"], "https://example.org/api-visible");
    assert!(links[0]["failing"].is_boolean(), "health is part of the fact");

    // The handle is accepted with or without the `@` a profile URL carries.
    let at = call(&app, get("/api/v1/members/@apiuser")).await;
    assert_eq!(at.status, StatusCode::OK);

    // A hidden member is a 404 here exactly as it is on the site.
    orbital::store::set_trust(&pool, user_id(&pool, "apiuser").await, 0, Some(true))
        .await
        .unwrap();
    let hidden = call(&app, get("/api/v1/members/apiuser")).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{}", hidden.body);

    let missing = call(&app, get("/api/v1/members/nobody")).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND, "{}", missing.body);
}

async fn user_id(pool: &PgPool, handle: &str) -> uuid::Uuid {
    orbital::store::user_by_handle(pool, handle)
        .await
        .unwrap()
        .expect("member exists")
        .id
}

#[sqlx::test]
async fn the_health_endpoint_names_the_worst_links(pool: PgPool) {
    let app = app(pool.clone()).await;
    let sess = register(&app, "rotter", "dev").await.cookie.unwrap();
    let created = call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({"url": "https://example.org/gone"}),
            Some(&sess),
        ),
    )
    .await;
    let id = created.json()["link"]["id"].as_str().unwrap().to_string();
    let link_id: uuid::Uuid = id.parse().unwrap();

    // What the checker would record after three failed nights.
    for _ in 0..3 {
        orbital::store::record_link_health(&pool, link_id, Some(410), false)
            .await
            .unwrap();
    }

    let health = call(&app, get("/api/v1/links/health")).await;
    assert_eq!(health.status, StatusCode::OK, "{}", health.body);
    assert!(health.body.contains("https://example.org/gone"), "{}", health.body);
    assert_eq!(health.json()["threshold"].as_i64().unwrap(), 3);

    // And the ring endpoint answers the same walk the redirect performs.
    let ring = call(&app, get("/api/v1/ring/dev")).await;
    assert_eq!(ring.status, StatusCode::OK, "{}", ring.body);
    assert_eq!(ring.json()["vertical"], "dev");
    assert!(ring.json()["next"]["handle"].is_string());

    let nope = call(&app, get("/api/v1/ring/no-such-vertical")).await;
    assert_eq!(nope.status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn the_read_api_validates_and_speaks_cors(pool: PgPool) {
    // Two properties a public, unauthenticated endpoint owes its callers: a validator so a poller
    // costs a 304 instead of the payload, and a CORS header so a browser client can exist at all.
    let app = app(pool).await;
    // The ring answers 404 for an empty category, and a 404 correctly carries no validator - so
    // the category needs a member before it can be used to prove the headers are universal.
    register(&app, "etagger", "dev").await;

    let first = call(&app, get("/api/v1/stats")).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let etag = first.etag.clone().expect("the API must send an ETag");
    assert!(etag.starts_with("W/\""), "a weak validator, got {etag}");
    assert_eq!(first.cors.as_deref(), Some("*"), "CORS on the body response");

    let mut cond = get("/api/v1/stats");
    cond.headers_mut()
        .insert("if-none-match", etag.parse().unwrap());
    let second = call(&app, cond).await;
    assert_eq!(
        second.status,
        StatusCode::NOT_MODIFIED,
        "If-None-Match must be honoured: {}",
        second.body
    );
    assert_eq!(
        second.cors.as_deref(),
        Some("*"),
        "a 304 without CORS is a response the browser throws away"
    );

    // A validator from another representation must not suppress the body.
    let mut stale = get("/api/v1/stats");
    stale
        .headers_mut()
        .insert("if-none-match", "W/\"0000000000000000\"".parse().unwrap());
    let third = call(&app, stale).await;
    assert_eq!(third.status, StatusCode::OK);
    assert!(third.body.contains("members"), "{}", third.body);

    // And every endpoint carries it, not just the one that was tested.
    for path in [
        "/api/v1/verticals",
        "/api/v1/members",
        "/api/v1/links/health",
        "/api/v1/ring/dev",
    ] {
        let r = call(&app, get(path)).await;
        assert!(r.etag.is_some(), "{path} has no ETag");
        assert_eq!(r.cors.as_deref(), Some("*"), "{path} has no CORS header");
    }
}

#[sqlx::test]
async fn the_read_api_does_not_hand_out_a_cookie(pool: PgPool) {
    // A `Set-Cookie` makes a response uncacheable: measured in production, Cloudflare answered
    // `cf-cache-status: DYNAMIC` for a response that advertised `max-age=60`. Nothing under
    // /api/v1 counts a view, so the visitor id has no work to do there - but every HTML page still
    // needs one, or view deduplication stops working.
    let app = app(pool).await;

    let api = app.clone().oneshot(get("/api/v1/stats")).await.unwrap();
    assert_eq!(api.status(), StatusCode::OK);
    assert!(
        api.headers().get("set-cookie").is_none(),
        "the read API must stay cacheable"
    );
    assert_eq!(
        api.headers().get("cache-control").and_then(|v| v.to_str().ok()),
        Some("public, max-age=60")
    );

    let page = app.clone().oneshot(get("/")).await.unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert!(
        page.headers().get_all("set-cookie").iter().any(|v| v
            .to_str()
            .map(|c| c.starts_with("orbital_v="))
            .unwrap_or(false)),
        "a page still mints the visitor id"
    );
}

fn post_form(path: &str, body: &str, c: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("cookie", c)
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Tags are set through the store, not an endpoint: there is no tag API, and a test that needs a
/// member with a tag should say so directly rather than route it through a product feature.
async fn give_tags(pool: &PgPool, handle: &str, tags: &[&str]) {
    for t in tags {
        sqlx::query(
            "INSERT INTO tags (user_id, tag_slug) SELECT id, $2 FROM users WHERE handle = $1
             ON CONFLICT DO NOTHING",
        )
        .bind(handle)
        .bind(t)
        .execute(pool)
        .await
        .unwrap();
    }
}

fn order_of(body: &str, handles: &[&str]) -> Vec<String> {
    let mut found: Vec<(usize, String)> = handles
        .iter()
        .filter_map(|h| body.find(&format!("/@{h}")).map(|i| (i, h.to_string())))
        .collect();
    found.sort_by_key(|(i, _)| *i);
    found.into_iter().map(|(_, h)| h).collect()
}

#[sqlx::test]
async fn the_tuner_reorders_the_list_for_one_viewer_only(pool: PgPool) {
    let app = app(pool.clone()).await;
    register(&app, "robi", "music").await;
    register(&app, "zara", "music").await;
    give_tags(&pool, "robi", &["chiptune"]).await;
    give_tags(&pool, "zara", &["podcast"]).await;

    // A cold visitor: no weights, so the ordering is the honest one (handle order, no traffic).
    let cold = call(&app, get("/v/music")).await;
    let viewer = cold.vcookie.clone().expect("the page mints a visitor id");
    let before = order_of(&cold.body, &["robi", "zara"]);
    assert_eq!(before, vec!["robi", "zara"], "cold order");

    // Tune: chiptune at full weight.
    let applied = call(&app, post_form("/prefs", "tag=chiptune&w=10", &viewer)).await;
    assert_eq!(applied.status, StatusCode::SEE_OTHER, "{}", applied.body);

    let tuned = call(&app, cookie(get("/v/music"), &viewer)).await;
    assert_eq!(
        order_of(&tuned.body, &["robi", "zara"]),
        vec!["robi", "zara"],
        "robi already led; the point is that the weight changed the score, checked below"
    );

    // The same list for a *different* visitor is unchanged: a lens is not a global re-sort.
    let other = call(&app, get("/v/music")).await;
    let other_viewer = other.vcookie.expect("second visitor");
    assert_ne!(other_viewer, viewer);
    assert_eq!(order_of(&other.body, &["robi", "zara"]), before);
}

#[sqlx::test]
async fn the_tuner_can_change_which_member_leads(pool: PgPool) {
    // Two members with no traffic: the cold order is the handle order, and the tuner has to be
    // able to invert it, or the weights are decorative.
    let app = app(pool.clone()).await;
    register(&app, "aaron", "music").await;
    register(&app, "zoe", "music").await;
    give_tags(&pool, "aaron", &["podcast"]).await;
    give_tags(&pool, "zoe", &["chiptune"]).await;

    let cold = call(&app, get("/v/music")).await;
    let viewer = cold.vcookie.expect("visitor");
    assert_eq!(order_of(&cold.body, &["aaron", "zoe"]), vec!["aaron", "zoe"]);

    let raw = viewer.trim_start_matches("orbital_v=").to_string();
    let loaded = orbital::prefs::load(&pool, &raw).await.expect("load prefs");
    println!("DEBUG before applied={} weights={:?}", loaded.applied, loaded.weights);
    let posted = call(&app, post_form("/prefs", "tag=chiptune&w=10", &viewer)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "post /prefs: {}", posted.body);
    let stored: Vec<(String, f32, String)> =
        sqlx::query_as("SELECT tag_slug, weight, source FROM preference_weights")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!stored.is_empty(), "the tuner must store something: {stored:?}");
    let after = orbital::prefs::load(&pool, &raw).await.expect("load prefs");
    println!("DEBUG after applied={} weights={:?}", after.applied, after.weights);
    let tuned = call(&app, cookie(get("/v/music"), &viewer)).await;
    assert_eq!(
        order_of(&tuned.body, &["aaron", "zoe"]),
        vec!["zoe", "aaron"],
        "a full weight on chiptune lifts zoe above the alphabetically first member"
    );
}

#[sqlx::test]
async fn reading_a_profile_leaves_a_passive_preference(pool: PgPool) {
    let app = app(pool.clone()).await;
    register(&app, "robi", "music").await;
    give_tags(&pool, "robi", &["chiptune"]).await;

    let first = call(&app, get("/@robi")).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let viewer = first.vcookie.expect("visitor");

    let pages: (i32,) =
        sqlx::query_as("SELECT pages_visited FROM visitor_profiles")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pages.0, 1, "the page view is counted");

    let row: (String, f32, String) = sqlx::query_as(
        "SELECT tag_slug, weight, source FROM preference_weights
          WHERE owner_kind = 'visitor' AND source = 'passive'",
    )
    .fetch_one(&pool)
    .await
    .expect("reading a profile leaves passive signal");
    assert_eq!(row.0, "chiptune");
    assert!((row.1 - 0.15).abs() < 1e-6);
    assert_eq!(row.2, "passive");
    assert!(viewer.starts_with("orbital_v="));
}

#[sqlx::test]
async fn a_profile_link_carries_where_the_visitor_found_it(pool: PgPool) {
    let app = app(pool.clone()).await;
    let reg = register(&app, "robi", "music").await;
    let sess = reg.cookie.expect("session");
    call(
        &app,
        json_req(
            "POST",
            "/api/links",
            serde_json::json!({ "url": "https://example.com/from-the-ring", "label": "Ring test" }),
            Some(&sess),
        ),
    )
    .await;

    let direct = call(&app, get("/@robi")).await;
    assert!(direct.body.contains("s=profile"), "a direct visit says so");

    let from_ring = call(&app, get("/@robi?s=ring")).await;
    assert!(
        from_ring.body.contains("s=ring"),
        "the ring's arrival must label the click, or the dashboard cannot tell the surfaces apart"
    );

    // A crafted value is not an analytics column: anything unknown falls back to `profile`, and the
    // value never reaches the page. (The page has legitimate inline scripts, so the assertion is
    // that the *surface* is the fallback, not that the word "script" is absent.)
    let junk = call(&app, get("/@robi?s=%3Cscript%3Ealert(1)%3C/script%3E")).await;
    assert_eq!(junk.status, StatusCode::OK);
    assert!(junk.body.contains("s=profile"));
    assert!(!junk.body.contains("alert(1)"), "the query value must not be reflected");
}

#[sqlx::test]
async fn the_ring_follows_what_the_visitor_asked_for(pool: PgPool) {
    let app = app(pool.clone()).await;
    register(&app, "start", "music").await;
    register(&app, "podcaster", "music").await;
    register(&app, "chiptuneer", "music").await;
    give_tags(&pool, "start", &["chiptune"]).await;
    give_tags(&pool, "podcaster", &["podcast"]).await;
    give_tags(&pool, "chiptuneer", &["chiptune"]).await;

    // Cold: with no weights the walk stays on the ring order, which is not preference-shaped.
    let cold = call(&app, get("/v/music/next?from=start")).await;
    assert_eq!(cold.status, StatusCode::SEE_OTHER);
    let viewer = cold.vcookie.clone().expect("visitor");

    call(&app, post_form("/prefs", "tag=chiptune&w=10", &viewer)).await;
    let tuned = call(&app, cookie(get("/v/music/next?from=start"), &viewer)).await;
    assert_eq!(tuned.status, StatusCode::SEE_OTHER);
    assert_eq!(
        tuned.location.as_deref(),
        Some("/@chiptuneer?s=ring"),
        "the walk should go to the member the viewer asked for, and say it came from the ring"
    );
}

#[sqlx::test]
async fn the_lens_page_shows_what_the_visitor_asked_for(pool: PgPool) {
    let app = app(pool.clone()).await;
    let cold = call(&app, get("/lens")).await;
    assert_eq!(cold.status, StatusCode::OK, "{}", cold.body);
    assert!(cold.body.contains("No weights set"));
    let viewer = cold.vcookie.clone().expect("visitor");

    call(&app, post_form("/prefs", "tag=chiptune&w=8", &viewer)).await;
    let tuned = call(&app, cookie(get("/lens"), &viewer)).await;
    assert_eq!(tuned.status, StatusCode::OK);
    assert!(tuned.body.contains("chiptune"), "{}", tuned.body);
    assert!(tuned.body.contains("this browser"), "a visitor's lens says where it is saved");

    // Clearing it puts the page back to the empty state.
    call(&app, post_form("/prefs/reset", "", &viewer)).await;
    let cleared = call(&app, cookie(get("/lens"), &viewer)).await;
    assert!(cleared.body.contains("No weights set"));
}

#[sqlx::test]
async fn signing_up_keeps_the_lens_built_while_anonymous(pool: PgPool) {
    let app = app(pool.clone()).await;
    let mint = call(&app, get("/lens")).await;
    let viewer = mint.vcookie.expect("visitor");
    call(&app, post_form("/prefs", "tag=chiptune&w=9", &viewer)).await;

    // Register with the same cookie the tuner was used with.
    let reg = call(
        &app,
        {
            let mut r = json_req(
                "POST",
                "/api/auth/register",
                serde_json::json!({
                    "handle": "tuner",
                    "password": "a long enough password",
                    "display_name": "Tuner",
                    "vertical": "music",
                    "bio": "I tuned before I joined.",
                }),
                None,
            );
            r.headers_mut().insert("cookie", viewer.parse().unwrap());
            r
        },
    )
    .await;
    assert_eq!(reg.status, StatusCode::OK, "{}", reg.body);
    let sess = reg.cookie.expect("session");

    let lens = call(&app, cookie(get("/lens"), &sess)).await;
    assert!(lens.body.contains("your account"), "the lens moved to the account");
    assert!(
        !lens.body.contains("No weights set"),
        "the page must show the adopted weight, not the empty state"
    );
    // The placeholder text mentions "chiptune" too, so the page alone cannot prove adoption.
    let owned: Vec<(String, f64)> = sqlx::query_as(
        "SELECT tag_slug, weight::float8 FROM preference_weights WHERE owner_kind = 'user'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(owned.len(), 1, "one weight, stored on the account");
    assert_eq!(owned[0].0, "chiptune");
}

#[sqlx::test]
async fn the_tuner_arrives_folded_for_everyone_including_a_tuned_visitor(pool: PgPool) {
    // The block used to be rendered `open` whenever the viewer held any weight — and `store::prefs`
    // merges passive weights into the same list, so every returning reader got an unfolded panel
    // they had collapsed. The fold is the visitor's choice; the server never renders it.
    let app = app(pool.clone()).await;
    register(&app, "muso", "music").await;

    let cold = call(&app, get("/v/music")).await;
    let viewer = cold.vcookie.clone().expect("visitor");
    assert!(
        cold.body.contains("<details class=\"tuner\">"),
        "a cold page ships the tuner folded"
    );

    let posted = call(&app, post_form("/prefs", "tag=chiptune&w=8", &viewer)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);

    let tuned = call(&app, cookie(get("/v/music"), &viewer)).await;
    assert!(
        tuned.body.contains("#chiptune"),
        "the stored weight is still rendered as a row"
    );
    assert!(
        tuned.body.contains("<details class=\"tuner\">"),
        "a tuned visitor still gets the block"
    );
    assert!(
        !tuned.body.contains("<details class=\"tuner\" open>"),
        "and it is not unfolded on arrival: nothing may reopen what the visitor folded"
    );
}

#[sqlx::test]
async fn the_tuner_never_shows_internal_keys(pool: PgPool) {
    // `v:music` is how a vertical weight is stored. Showing the key is how this shipped once.
    let app = app(pool.clone()).await;
    register(&app, "muso", "music").await;
    let first = call(&app, get("/v/music")).await;
    let viewer = first.vcookie.expect("visitor");
    // Two more views: passive accumulation writes a `v:music` weight.
    call(&app, cookie(get("/v/music"), &viewer)).await;
    let third = call(&app, cookie(get("/v/music"), &viewer)).await;

    assert!(
        !third.body.contains("#v:"),
        "a stored key leaked into the widget"
    );
    if third.body.contains("in music") {
        // The label is the human form of the same weight.
        assert!(third.body.contains("in music"));
    }
}

#[sqlx::test]
async fn a_tuned_visitor_is_told_the_order_is_theirs(pool: PgPool) {
    // Personalisation nobody can see is indistinguishable from a coincidence.
    let app = app(pool.clone()).await;
    register(&app, "muso", "music").await;

    let cold = call(&app, get("/v/music")).await;
    assert!(!cold.body.contains("Ranked with your lens"));
    let viewer = cold.vcookie.expect("visitor");

    call(&app, post_form("/prefs", "tag=chiptune&w=8", &viewer)).await;
    let tuned = call(&app, cookie(get("/v/music"), &viewer)).await;
    assert!(
        tuned.body.contains("Ranked with your lens"),
        "a tuned viewer must be told the list is theirs"
    );
    assert!(tuned.body.contains("href=\"/lens\""), "and where to change it");
}

#[sqlx::test]
async fn passive_weights_wait_for_the_warm_up(pool: PgPool) {
    // A vertical view writes a passive `v:music` weight immediately. If that counted as a tuned
    // lens, one page view would reorder the site under a first-time visitor.
    let app = app(pool.clone()).await;
    register(&app, "muso", "music").await;
    register(&app, "other", "music").await;

    let first = call(&app, get("/v/music")).await;
    let viewer = first.vcookie.expect("visitor");
    assert!(first.body.contains("in music"), "the passive weight is visible in the tuner");
    assert!(
        !first.body.contains("Ranked with your lens"),
        "but it is not yet a lens"
    );

    // Four more views: still inside the warm-up.
    for _ in 0..4 {
        call(&app, cookie(get("/v/music"), &viewer)).await;
    }
    let fifth = call(&app, cookie(get("/v/music"), &viewer)).await;
    assert!(
        fifth.body.contains("Ranked with your lens"),
        "after five pages personalisation is allowed to apply"
    );
}

#[sqlx::test]
async fn a_category_is_a_weight_too(pool: PgPool) {
    // /lens offers "add a category (v:music)"; normalise_tag refuses the colon, so this row used to
    // store nothing at all. A category is validated against the real taxonomy instead.
    let app = app(pool.clone()).await;
    let first = call(&app, get("/lens")).await;
    let viewer = first.vcookie.expect("visitor");

    // Apply replaces the whole lens, so both entries go in one post.
    call(&app, post_form("/prefs", "tag=v:music&w=7&tag=v:nonsense&w=7", &viewer)).await;
    let lens = call(&app, cookie(get("/lens"), &viewer)).await;
    assert!(lens.body.contains("name=\"tag\" value=\"v:music\""), "a real category sticks");
    assert!(
        !lens.body.contains("v:nonsense"),
        "an invented one is refused, not stored"
    );
}

#[sqlx::test]
async fn a_lens_has_a_ceiling(pool: PgPool) {
    // The rate limit bounds requests, not size: one loop could post two hundred tags.
    let app = app(pool.clone()).await;
    let first = call(&app, get("/lens")).await;
    let viewer = first.vcookie.expect("visitor");

    // One form post carries the whole lens: posting one tag per request would replace the set each
    // time (that is what Apply means), and the cap would never be exercised.
    let body: Vec<String> = (0..40)
        .map(|i| format!("tag=tag{i}&w={}", i % 10 + 1))
        .collect();
    let body = body.join("&");
    call(&app, post_form("/prefs", &body, &viewer)).await;
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM preference_weights WHERE source = 'explicit'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 24, "the cap keeps the strongest 24");
}

#[sqlx::test]
async fn personalisation_can_be_turned_off(pool: PgPool) {
    let app = app(pool.clone()).await;
    register(&app, "muso", "music").await;

    let first = call(&app, get("/v/music")).await;
    let viewer = first.vcookie.expect("visitor");
    call(&app, post_form("/prefs", "tag=chiptune&w=9", &viewer)).await;
    let on = call(&app, cookie(get("/v/music"), &viewer)).await;
    assert!(on.body.contains("Ranked with your lens"));

    let off = call(&app, cookie(get("/prefs/off"), &viewer)).await;
    assert_eq!(off.status, 303);
    let flag = off.offcookie.expect("the off flag is set as a cookie");
    assert!(flag.contains("=1"), "got {flag}");
    let back = call(&app, cookie(get("/v/music"), &format!("{viewer}; {flag}"))).await;
    assert!(!back.body.contains("Ranked with your lens"), "off means off");
    assert!(back.body.contains("Personalisation is off"), "and it says so");

    let on = call(&app, cookie(get("/prefs/on"), &format!("{viewer}; {flag}"))).await;
    let cleared = on.offcookie.expect("clearing sets an empty value");
    assert!(cleared.ends_with("orbital_off="), "got {cleared}");
    let again = call(&app, cookie(get("/v/music"), &viewer)).await;
    assert!(again.body.contains("Ranked with your lens"), "and on means on");
}

// ---------------------------------------------------------------- platform (2026-10-29)

#[sqlx::test]
async fn a_token_can_add_a_link_without_a_browser(pool: PgPool) {
    // The whole point: a script, or the MCP server, can write without a session cookie.
    let app = app(pool.clone()).await;
    let reg = register(&app, "writer", "dev").await;
    let sess = reg.cookie.expect("session");

    let created = call(
        &app,
        json_req("POST", "/api/me/tokens", json!({"label": "cli"}), Some(&sess)),
    )
    .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let token = created.json()["token"].as_str().expect("token").to_string();
    assert!(token.starts_with("orb_"));

    // Not in the listing: the plaintext exists only in that one response.
    let listing = call(&app, cookie(get("/api/me/tokens"), &sess)).await;
    assert!(!listing.body.contains(&token), "the token must not be listed back");

    let added = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/v1/me/links")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(
                json!({"url": "https://example.com/writing", "label": "Writing"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);

    let mine = call(
        &app,
        Request::builder()
            .uri("/api/v1/me/links")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(mine.status, StatusCode::OK);
    assert!(mine.body.contains("example.com/writing"));

    // And a token that is not one is refused rather than treated as anonymous.
    let bad = call(
        &app,
        Request::builder()
            .uri("/api/v1/me/links")
            .header("authorization", "Bearer orb_deadbeef")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn the_status_page_lists_the_estate_and_says_when_it_does_not_know(pool: PgPool) {
    // The page reads `static/estate.txt`, which the deploy writes from nginx's server_names. So the
    // test must not assume either state: a clean checkout has no file (the page must say so rather
    // than pretend the network is empty), and a host that has deployed has one (the page must list
    // what it names). Measured 2026-11-10: the previous version asserted only the empty state and
    // therefore passed in CI and failed on the deploy host — a gate that lies in the one direction
    // that costs nothing to get right.
    let app = app(pool).await;
    let res = call(&app, get("/status")).await;
    assert_eq!(res.status, StatusCode::OK);

    let listed: Vec<String> = std::fs::read_to_string("static/estate.txt")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('|').map(|(host, _)| host.trim().to_string()))
        .filter(|h| !h.is_empty())
        .collect();

    if listed.is_empty() {
        assert!(
            res.body.contains("No hosts are configured yet"),
            "an honest empty state"
        );
    } else {
        for host in listed.iter().take(5) {
            assert!(
                res.body.contains(&format!("https://{host}/")),
                "the page must link {host}, because the estate file names it"
            );
        }
    }
}

#[sqlx::test]
async fn a_card_is_rendered_even_before_the_file_exists(pool: PgPool) {
    let app = app(pool).await;
    register(&app, "carder", "dev").await;
    let res = call(&app, get("/og/carder.png")).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.contains("<svg"), "the fallback is a generated card");
    assert!(res.body.contains("@carder"));

    let missing = call(&app, get("/og/nobody.png")).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn an_alias_merges_two_tags_on_the_next_retag(pool: PgPool) {
    let app = app(pool.clone()).await;
    let reg = register(&app, "boss", "dev").await;
    let sess = reg.cookie.expect("session");
    sqlx::query("UPDATE users SET is_owner = true, trust_level = 3 WHERE handle = 'boss'")
        .execute(&pool)
        .await
        .unwrap();
    let res = call(
        &app,
        json_req(
            "POST",
            "/api/admin/tag-alias",
            json!({"from": "Rust Lang", "to": "rust"}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let (from, to): (String, String) =
        sqlx::query_as("SELECT from_tag, to_tag FROM tag_aliases")
            .fetch_one(&pool)
            .await
            .unwrap();
    // Both sides are normalised on the way in, and the pair must still be two different tags.
    assert_eq!(from, "rust-lang");
    assert_eq!(to, "rust");
}

#[sqlx::test]
async fn rank_delta_reports_where_you_were(pool: PgPool) {
    let app = app(pool.clone()).await;
    let reg = register(&app, "ranker", "dev").await;
    let sess = reg.cookie.expect("session");
    let (id,): (Uuid,) = sqlx::query_as("SELECT id FROM users WHERE handle = 'ranker'")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Last week ninth, today third: +6.
    sqlx::query("INSERT INTO rank_snapshots (user_id, vertical_slug, rank, taken_at) VALUES ($1, 'dev', 9, now() - interval '7 days')")
        .bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO rank_snapshots (user_id, vertical_slug, rank, taken_at) VALUES ($1, 'dev', 3, now())")
        .bind(id).execute(&pool).await.unwrap();

    let res = call(&app, cookie(get("/api/me/rank-delta"), &sess)).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let body = res.json();
    let first = &body["ranks"][0];
    assert_eq!(first["rank"], 3);
    assert_eq!(first["previous"], 9);
    assert_eq!(first["delta"], 6);
}

// ---------------------------------------------------------------- governance (v4 remainder)

#[sqlx::test]
async fn three_votes_create_a_vertical(pool: PgPool) {
    let app = app(pool.clone()).await;
    let mut voters = Vec::new();
    for handle in ["v1", "v2", "v3"] {
        let s = register(&app, handle, "dev").await.cookie.expect("session");
        sqlx::query("UPDATE users SET trust_level = 3 WHERE handle = $1")
            .bind(handle)
            .execute(&pool)
            .await
            .unwrap();
        voters.push(s);
    }
    let made = call(
        &app,
        json_req(
            "POST",
            "/api/verticals/proposals",
            json!({"slug": "radio", "label": "Radio", "description": "People who broadcast."}),
            Some(&voters[0]),
        ),
    )
    .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.body);
    let id = made.json()["id"].as_str().unwrap().to_string();

    // Two votes are not a decision.
    for s in &voters[..2] {
        let r = call(&app, json_req("POST", &format!("/api/verticals/proposals/{id}/vote"), json!({}), Some(s))).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        assert_eq!(r.json()["accepted"], false);
    }
    let third = call(&app, json_req("POST", &format!("/api/verticals/proposals/{id}/vote"), json!({}), Some(&voters[2]))).await;
    assert_eq!(third.json()["accepted"], true, "{}", third.body);
    assert!(sqlx::query_as::<_, (String,)>("SELECT slug FROM verticals WHERE slug = 'radio'")
        .fetch_optional(&pool)
        .await
        .unwrap()
        .is_some());
}

#[sqlx::test]
async fn a_merge_moves_the_members_and_their_tags(pool: PgPool) {
    // The decision: a merge is a rename of the room, not an eviction.
    let app = app(pool.clone()).await;
    let boss = register(&app, "boss", "dev").await.cookie.expect("session");
    sqlx::query("UPDATE users SET is_owner = true, trust_level = 3 WHERE handle = 'boss'")
        .execute(&pool).await.unwrap();
    let muso = register(&app, "muso", "music").await.cookie.expect("session");
    let muso_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE handle = 'muso'")
        .fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO tags (user_id, tag_slug) VALUES ($1, 'chiptune')")
        .bind(muso_id).execute(&pool).await.unwrap();
    let _ = muso;

    let made = call(&app, json_req("POST", "/api/verticals/proposals",
        json!({"slug": "music", "label": "Music", "merged_into": "art"}), Some(&boss))).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.body);
    let id = made.json()["id"].as_str().unwrap().to_string();
    let voted = call(&app, json_req("POST", &format!("/api/verticals/proposals/{id}/vote"), json!({}), Some(&boss))).await;
    // One owner vote is not quorum, so decide by hand the way an operator would.
    assert_eq!(voted.json()["accepted"], false);
    let decided = call(&app, json_req("POST", &format!("/api/admin/proposals/{id}/decide"),
        json!({"accept": true}), Some(&boss))).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.body);

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT vertical_slug, role FROM user_verticals WHERE user_id = $1")
        .bind(muso_id).fetch_all(&pool).await.unwrap();
    assert!(rows.iter().any(|(s, r)| s == "art" && r == "primary"), "moved, still primary: {rows:?}");
    assert!(!rows.iter().any(|(s, _)| s == "music"), "the retired room is gone");
    let tags: (i64,) = sqlx::query_as("SELECT count(*) FROM tags WHERE user_id = $1")
        .bind(muso_id).fetch_one(&pool).await.unwrap();
    assert_eq!(tags.0, 1, "their tags came with them");
    // The room itself is gone: leaving it would list an empty vertical forever.
    let old: Option<(String,)> = sqlx::query_as("SELECT slug FROM verticals WHERE slug = 'music'")
        .fetch_optional(&pool).await.unwrap();
    assert!(old.is_none(), "the retired vertical must not survive the merge");
}

#[sqlx::test]
async fn an_endorsed_tag_lifts_a_member_but_never_swamps_clicks(pool: PgPool) {
    let app = app(pool.clone()).await;
    let mut endorsers = Vec::new();
    for handle in ["e1", "e2", "e3"] {
        let s = register(&app, handle, "dev").await.cookie.expect("session");
        sqlx::query("UPDATE users SET trust_level = 2 WHERE handle = $1")
            .bind(handle).execute(&pool).await.unwrap();
        endorsers.push(s);
    }
    // A TL1 member may not endorse.
    let low = register(&app, "low", "dev").await.cookie.expect("session");
    let refused = call(&app, json_req("POST", "/api/tags/chiptune/endorse", json!({}), Some(&low))).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    for s in &endorsers {
        let r = call(&app, json_req("POST", "/api/tags/chiptune/endorse", json!({}), Some(s))).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }
    let last = call(&app, json_req("POST", "/api/tags/chiptune/endorse", json!({}), Some(&endorsers[0]))).await;
    // Toggling: the same person endorsing twice takes it back.
    assert_eq!(last.json()["endorsements"], 2, "{}", last.body);
    let again = call(&app, json_req("POST", "/api/tags/chiptune/endorse", json!({}), Some(&endorsers[0]))).await;
    assert_eq!(again.json()["endorsements"], 3, "{}", again.body);
    let boost = again.json()["boost"].as_f64().unwrap();
    assert!((boost - 0.2).abs() < 1e-9, "three endorsements reach the cap, got {boost}");
}

#[sqlx::test]
async fn the_site_reads_in_spanish_when_asked(pool: PgPool) {
    let app = app(pool).await;

    let english = call(&app, get("/")).await;
    assert!(english.body.contains("Skip to content"));
    assert!(english.body.contains(">Directory<"));

    // An explicit choice, which must outrank the browser.
    let chosen = call(&app, cookie(get("/"), "orbital_lang=es")).await;
    assert_eq!(chosen.status, StatusCode::OK);
    assert!(chosen.body.contains("Ir al contenido"), "the skip link");
    assert!(chosen.body.contains(">Directorio<"), "the nav");
    assert!(
        chosen.body.contains("/lang/en"),
        "the switch points back to English"
    );
    assert!(
        !chosen.body.contains("(missing translation)"),
        "every key used by the chrome exists in the table"
    );

    // And the browser's own preference, for a visitor who has not chosen.
    let negotiated = call(
        &app,
        Request::builder()
            .uri("/")
            .header("accept-language", "es-ES,es;q=0.9,en;q=0.8")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(negotiated.body.contains("Ir al contenido"), "Accept-Language is honoured");
}

#[sqlx::test]
async fn the_read_api_search_answers_the_route_the_capability_document_declares(pool: PgPool) {
    // The estate's capabilities document declares `search: local` with a route. A declaration with
    // no route behind it is what the W0 contract exists to prevent, so this test asks the route the
    // same question the HTML page answers: does it find the member, and does it name the tags?
    let app = app(pool).await;
    register(&app, "searcher", "dev").await;

    let hit = call(&app, get("/api/v1/search?q=searcher")).await;
    assert_eq!(hit.status, StatusCode::OK, "{}", hit.body);
    let body: serde_json::Value = serde_json::from_str(&hit.body).expect("the route must answer JSON");
    assert_eq!(body["q"], "searcher", "{}", hit.body);
    assert!(
        body["total"].as_i64().unwrap_or(0) >= 1,
        "the member the page finds must be found here: {}",
        hit.body
    );
    assert!(
        body["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["handle"] == "searcher"),
        "{}",
        hit.body
    );
    assert!(hit.etag.is_some(), "the contract asks for a validator");
    assert_eq!(hit.cors.as_deref(), Some("*"), "a client must be able to call it");

    // An empty query is not an error, and it must not dump the directory: a client that forgets `q`
    // would otherwise receive every member on the site.
    let empty = call(&app, get("/api/v1/search?q=")).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.body);
    let eb: serde_json::Value = serde_json::from_str(&empty.body).unwrap();
    assert_eq!(eb["total"], 0, "{}", empty.body);
    assert!(eb["members"].as_array().unwrap().is_empty(), "{}", empty.body);
}

#[sqlx::test]
async fn the_home_page_carries_three_shelves_and_says_why_each_card_is_there(pool: PgPool) {
    // The step's gate, on the rendered page: three named shelves for a stranger, the same three for a
    // tuned visitor, and a reason on every card that cites only something the reader can observe.
    // Before this change the page carried one "Trending this week" list and an untruncated "New" list:
    // `data-shelf` appeared zero times on the live home page (measured 2026-11-10).
    let app = app(pool.clone()).await;
    for (handle, vertical) in [("alpha", "dev"), ("beta", "dev"), ("gamma", "music")] {
        register(&app, handle, vertical).await;
    }
    // `Rising` is `rank_trending`, which drops every member with zero visitors in the last seven days
    // (`trending_score` returns 0.0 when `max_7d` is 0). So the fixture has to produce the thing the
    // shelf is a claim about: real `source_hits` rows. A test that skipped this would be asserting an
    // empty shelf renders, which is not what the step says.
    for (i, handle) in ["alpha", "beta", "gamma"].iter().enumerate() {
        sqlx::query(
            "INSERT INTO source_hits (user_id, session_hash, created_at)
             SELECT id, $2, now() - make_interval(hours => $3::int) FROM users WHERE handle = $1",
        )
        .bind(*handle)
        .bind(format!("sess-{i}"))
        .bind(i as i32)
        .execute(&pool)
        .await
        .expect("a source hit is what makes a member rise");
    }

    let cold = call(&app, get("/")).await;
    assert_eq!(cold.status, StatusCode::OK, "{}", cold.body);
    for key in ["for_you", "rising", "new"] {
        assert!(
            cold.body.contains(&format!("data-shelf=\"{key}\"")),
            "the {key} shelf must render for a stranger: {}",
            cold.body
        );
    }
    assert!(
        cold.body.contains("class=\"mcard-w\""),
        "every shelf card carries a why line: {}",
        cold.body
    );

    // A tuned visitor: the shelves must still be three, and their own tag must be the reason shown.
    let viewer = cold.vcookie.clone().expect("the page mints a visitor id");
    let applied = call(&app, post_form("/prefs", "tag=chiptune&w=10", &viewer)).await;
    assert_eq!(applied.status, StatusCode::SEE_OTHER, "{}", applied.body);

    let tuned = call(&app, cookie(get("/"), &viewer)).await;
    assert_eq!(tuned.status, StatusCode::OK);
    for key in ["for_you", "rising", "new"] {
        assert!(
            tuned.body.contains(&format!("data-shelf=\"{key}\"")),
            "the {key} shelf must render for a tuned visitor"
        );
    }

    // D27 on the rendered page: nothing on the home page may name the instance's prior. The test is
    // deliberately blunt — the words a disclosure would need are cheap to grep and expensive to leak.
    let page = tuned.body.to_lowercase();
    for word in ["taste", "prior", "gravity", "curated by", "operator's pick"] {
        assert!(!page.contains(word), "the home page must not name the prior ({word})");
    }
}

/// The prior is applied and never named (D27). This is the smallest honest version of that rule:
/// there is no route that answers about it, and the page a stranger lands on does not contain the
/// instance's source names or the word for what they build.
#[sqlx::test]
async fn the_instance_prior_has_no_public_surface(pool: PgPool) {
    let app = app(pool).await;

    let res = call(
        &app,
        Request::builder()
            .uri("/api/v1/taste")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    // ROADMAP step 1 hands `/api/v1/taste` to the **caller's own** lens, and D40 records why that is not
    // an announcement: it answers `tags` (the weights the reader set) and `dial` (the number they own),
    // never a source, a weight of the instance's, or a field describing one. The guard this replaced —
    // "the route must not exist at all" — is now this test plus `no_surface_names_the_prior`.
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let body = res.json();
    let keys: Vec<&str> = body.as_object().map(|o| o.keys().map(|k| k.as_str()).collect()).unwrap_or_default();
    assert_eq!(keys, vec!["dial", "tags"], "two fields, both the caller's: {}", res.body);

    let home = call(&app, Request::builder().uri("/").body(Body::empty()).unwrap()).await;
    assert_eq!(home.status, StatusCode::OK);
    for word in ["attuned", "steeltitan", "\"taste\"", "prior"] {
        assert!(
            !home.body.to_lowercase().contains(word),
            "the home page must not mention `{word}`: the prior is operator-only"
        );
    }
}

/// The dial rides with the tuner: inside the surface's declared ceiling it is stored and shown back,
/// above it the request is refused rather than quietly clamped (`polaris-gravity`'s rule, D36).
#[sqlx::test]
async fn the_dial_is_stored_inside_the_ceiling_and_refused_above_it(pool: PgPool) {
    let app = app(pool).await;

    let ok = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/prefs")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("lens=25"))
            .unwrap(),
    )
    .await;
    assert_eq!(ok.status, StatusCode::SEE_OTHER, "a legal dial is a redirect, not a page");
    let cookie = ok.vcookie.clone();

    let mut req = Request::builder().uri("/");
    if let Some(c) = &cookie {
        req = req.header("cookie", c);
    }
    let page = call(&app, req.body(Body::empty()).unwrap()).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(
        page.body.contains("value=\"25\""),
        "the slider must render the reader's own number"
    );

    let refused = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/prefs")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("lens=41"))
            .unwrap(),
    )
    .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "41% is above the 40% this surface declares and must be refused, not clamped"
    );
    assert!(
        refused.body.contains("at most 40%"),
        "the refusal names the ceiling the reader may choose: {}",
        refused.body
    );
}

/// The concentration numbers are an operator's instrument, not a public statistic: a page that says how
/// unevenly attention falls is a page about whose work the site favours. Nothing serves them (D34/D27).
#[sqlx::test]
async fn the_concentration_numbers_are_not_served(pool: PgPool) {
    let app = app(pool.clone()).await;
    let res = call(&app, get("/api/v1/concentration")).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND, "there is no concentration route");

    // And the surface that does exist does not grow the number: a public Gini is a sentence about whose
    // work the site favours, which no response may carry.
    let stats = call(&app, get("/api/v1/stats")).await;
    assert_eq!(stats.status, StatusCode::OK, "the stats route is unchanged");
    let body = stats.body.to_lowercase();
    for word in ["gini", "concentration", "top10", "top_10"] {
        assert!(!body.contains(word), "/api/v1/stats must not carry {word}");
    }
    let home = call(&app, get("/")).await;
    let page = home.body.to_lowercase();
    for word in ["gini", "concentration"] {
        assert!(!page.contains(word), "the home page must not carry {word}");
    }
}

/// THE GATE for the per-surface policy: a weight is declared per surface, and no surface reads another's.
/// One dataset, four surfaces, one number changed at a time.
#[sqlx::test]
async fn each_surface_declares_its_own_weight_and_reads_no_other(pool: PgPool) {
    // This test is about the surface weight and nothing else, so attention is pinned to `raw` here: the
    // damped default is a different step's subject (tests/attention.rs) and mixing them would make a
    // failure name two causes.
    let mut base = test_config();
    base.attention = orbital::attention::Config::raw();
    let declare = |surface: &str, key: &str, value: f64| {
        let raw = format!(
            r#"{{"surfaces":[{{"name":"{surface}","strength":0.40,"{key}":{value}}}]}}"#
        );
        let mut cfg = base.clone();
        cfg.dial = orbital::dial::Config::from_json(&raw).expect("a valid declaration");
        cfg
    };

    let plain = app_with(pool.clone(), base.clone()).await;
    register(&plain, "robi", "music").await;
    register(&plain, "zara", "music").await;
    give_tags(&pool, "robi", &["chiptune"]).await;
    give_tags(&pool, "zara", &["podcast"]).await;
    // Clicks are normalised by the busiest member, so the fixture is chosen where the margin is real:
    // robi 3 impressions of 4 (0.45) plus the lens, zara 4 of 4 (0.60). At full weight the lens wins,
    // at a quarter of it the traffic does.
    // One session per impression: since D41 a repeat visit is the same person, and a fixture that reuses
    // one session hash would be counting one visitor four times — the cap would flatten it to a tie and
    // this test would be measuring the cap instead of the weight.
    for (handle, times) in [("robi", 3), ("zara", 4)] {
        for n in 0..times {
            sqlx::query(
                "INSERT INTO source_hits (user_id, surface, session_hash, position)
                 SELECT id, 'profile', $2, 1 FROM users WHERE handle = $1",
            )
            .bind(handle)
            .bind(format!("gate-{handle}-{n}"))
            .execute(&pool)
            .await
            .expect("a real impression");
        }
    }

    let cold = call(&plain, get("/v/music")).await;
    let viewer = cold.vcookie.clone().expect("a visitor id");
    call(&plain, post_form("/prefs", "tag=chiptune&w=10", &viewer)).await;

    // The declaration is read from a file, so it is exercised the way an operator would set it: parse,
    // boot, ask the same URL with the same cookie.
    let with_vertical = app_with(pool.clone(), declare("vertical", "weight", 0.10)).await;
    let with_home = app_with(pool.clone(), declare("home", "weight", 0.10)).await;

    let before = order_of(&call(&plain, cookie(get("/v/music"), &viewer)).await.body, &["robi", "zara"]);
    let after = order_of(&call(&with_vertical, cookie(get("/v/music"), &viewer)).await.body, &["robi", "zara"]);
    assert_eq!(before, vec!["robi", "zara"], "the lens leads at the default weight");
    assert_eq!(
        after,
        vec!["zara", "robi"],
        "cutting the vertical's weight to a quarter must hand the order back to the traffic"
    );

    // The other three surfaces do not move: the home page byte for byte, the ring where it pointed.
    let home_before = call(&plain, cookie(get("/"), &viewer)).await.body;
    let home_after = call(&with_vertical, cookie(get("/"), &viewer)).await.body;
    assert_eq!(home_before, home_after, "the vertical's weight must not reach the home page");

    let ring_before = call(&plain, cookie(get("/ring"), &viewer)).await.location;
    let ring_after = call(&with_vertical, cookie(get("/ring"), &viewer)).await.location;
    assert_eq!(ring_before, ring_after, "nor the ring");

    // And the reverse: the home page's own weight does not reach a vertical.
    let vertical_under_home = order_of(
        &call(&with_home, cookie(get("/v/music"), &viewer)).await.body,
        &["robi", "zara"],
    );
    assert_eq!(vertical_under_home, before, "the home's weight must not reach /v/{{slug}}");
}

/// THE SILENCE GATE: walk every surface a stranger can reach and fail if any of them names the prior —
/// its vocabulary, a declared source's own name, or a field that offers to report it. The declaration here
/// names a source "the regulars", which is exactly the kind of editorial choice a reader must not see.
#[sqlx::test]
async fn no_surface_names_the_prior(pool: PgPool) {
    use orbital::silence::leaks;

    let mut cfg = test_config();
    cfg.taste = orbital::taste::TasteConfig::from_json(
        r#"{"sources":[{"name":"the regulars","selector":{"kind":"trust_level","at_least":3},
             "weight":1.0,"kinds":["like"]}]}"#,
    )
    .expect("a valid declaration");
    let extra = vec!["the regulars".to_string()];
    let app = app_with(pool.clone(), cfg).await;
    register(&app, "robi", "dev").await;
    give_tags(&pool, "robi", &["rust"]).await;

    let paths = [
        "/", "/about", "/@robi", "/v/dev", "/v/dev?tab=trending", "/v/dev?tab=new",
        "/search?q=rust", "/api/v1/members", "/api/v1/members/robi", "/api/v1/stats",
        "/api/v1/verticals", "/api/v1/search?q=rust", "/api/v1/ring/dev", "/feed.xml",
        "/feed.json", "/sitemap.xml", "/robots.txt",
        // `/prefs` is a POST-only write; the tuner it feeds is rendered inside `/v/{slug}`, which is
        // already in this list. A surface that answers 405 has no body to read.
    ];
    for path in paths {
        let res = call(&app, get(path)).await;
        assert!(
            res.status.is_success() || res.status.is_redirection(),
            "{path} answered {} — a surface that cannot be read cannot be proved silent",
            res.status
        );
        assert_eq!(
            leaks(&res.body, &extra),
            Vec::<String>::new(),
            "{path} names the prior"
        );
    }

    // `/api/v1/capabilities` is not in this router: nginx serves it from the estate-generated file (D32),
    // so a test of the app cannot see it. It is covered where it actually lives — `scripts/check-silence.sh`
    // runs against the deployed host, and it is the one surface allowed to say the word, as `taste: off`.
}

/// `/api/v1/taste` is the **reader's own** lens: their tag weights and their dial, and not one field of the
/// instance's. It is the only place the word appears in a route, and it is about the caller.
#[sqlx::test]
async fn the_lens_route_answers_only_the_callers_own_lens(pool: PgPool) {
    use orbital::silence::leaks;
    let app = app(pool.clone()).await;
    register(&app, "robi", "dev").await;

    let cold = call(&app, get("/")).await;
    let viewer = cold.vcookie.clone().expect("a visitor id");
    let empty = call(&app, cookie(get("/api/v1/taste"), &viewer)).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.body);
    let body = empty.json();
    assert_eq!(body["tags"], serde_json::json!({}), "a stranger has no lens, and that is an empty object");
    assert!(body["dial"].is_object(), "their dial is theirs to read: {}", empty.body);
    assert_eq!(leaks(&empty.body, &[]), Vec::<String>::new(), "{}", empty.body);

    call(&app, post_form("/prefs", "tag=chiptune&w=10", &viewer)).await;
    let tuned = call(&app, cookie(get("/api/v1/taste"), &viewer)).await;
    assert_eq!(tuned.status, StatusCode::OK);
    let tuned_body = tuned.json();
    assert_eq!(
        tuned_body["tags"]["chiptune"], 1.0,
        "the weight the reader set, at full scale: {}",
        tuned.body
    );
    assert_eq!(leaks(&tuned.body, &[]), Vec::<String>::new(), "{}", tuned.body);
}

/// The monument tab is reachable and honest: the lifetime total, in full digits, on the tab that claims it.
#[sqlx::test]
async fn the_monument_tab_shows_the_lifetime_record(pool: PgPool) {
    let app = app(pool.clone()).await;
    register(&app, "veteran", "dev").await;
    register(&app, "newcomer", "dev").await;
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE handle = 'veteran'")
        .fetch_one(&pool)
        .await
        .expect("a member");
    // The two hundred that are recent, which is the other half of the gate's sentence.
    let fresh: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE handle = 'newcomer'")
        .fetch_one(&pool)
        .await
        .expect("a member");
    sqlx::query(
        "INSERT INTO source_hits (user_id, surface, session_hash, position, created_at)
         SELECT $1, 'profile', 'new-' || g, 1, now() - interval '3 days' FROM generate_series(1, 200) g",
    )
    .bind(fresh)
    .execute(&pool)
    .await
    .expect("real clicks");
    sqlx::query(
        "INSERT INTO source_hits (user_id, surface, session_hash, position, created_at)
         SELECT $1, 'profile', 'old-' || g, 1, now() - interval '600 days' FROM generate_series(1, 10000) g",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("real clicks");

    let monument = call(&app, get("/v/dev?tab=monument")).await;
    assert_eq!(monument.status, StatusCode::OK, "{}", monument.body);
    assert!(
        monument.body.contains("10,000"),
        "the monument tab shows the record in full: {}",
        &monument.body[..monument.body.len().min(400)]
    );
    assert!(monument.body.contains("clicks, all time"));
    assert_eq!(
        order_of(&monument.body, &["veteran", "newcomer"]),
        vec!["veteran", "newcomer"],
        "the record leads the monument"
    );

    let top = call(&app, get("/v/dev?tab=top")).await;
    assert_eq!(top.status, StatusCode::OK);
    // What the store says, so a wrong page order is either a ranking bug or a page bug and never a guess.
    let pool_members = orbital::store::vertical_members(&pool, "dev").await.expect("pool");
    let proof: Vec<String> = pool_members
        .iter()
        .map(|m| format!("{} recent {:.1} clicks {}", m.handle, m.recent, m.clicks))
        .collect();
    let ranked: Vec<String> = orbital::ranking::rank_top(
        pool_members,
        &[],
        1.0,
        orbital::attention::Config::default(),
        orbital::ranking::Window::Recent,
    )
    .into_iter()
    .map(|m| m.handle)
    .collect();
    assert_eq!(ranked.first().map(|s| s.as_str()), Some("newcomer"), "{proof:?}");

    let at_vet = top.body.find("/@veteran").unwrap_or(0);
    let at_new = top.body.find("/@newcomer").unwrap_or(0);
    assert_eq!(
        order_of(&top.body, &["veteran", "newcomer"]),
        vec!["newcomer", "veteran"],
        "Top is a recent list, and the stale ten thousand ranks below it (veteran at {at_vet}, newcomer at {at_new})"
    );
    assert!(
        !top.body.contains("10,000"),
        "Top does not claim the lifetime number"
    );
}
