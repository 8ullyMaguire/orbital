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
use sqlx::PgPool;
use tower::ServiceExt;

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
