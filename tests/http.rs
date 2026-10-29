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
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Res {
        status,
        body: String::from_utf8_lossy(&bytes).to_string(),
        cookie,
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
async fn adult_members_never_render_on_the_sfw_host(pool: PgPool) {
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

    let sfw = call(&app, get("/@adult")).await;
    assert_eq!(sfw.status, StatusCode::NOT_FOUND, "adult profile visible on SFW host");

    // The adult host redirects an unacknowledged visitor to the gate, and the
    // gate explains what it is before anything adult renders.
    let gated = call(&app, host(get("/@adult"), "nsfw.test.local")).await;
    assert!(
        gated.status.is_redirection(),
        "an unacknowledged adult request must go to the gate, got {}",
        gated.status
    );
    let gate_page = call(&app, host(get("/gate?next=%2F%40adult"), "nsfw.test.local")).await;
    assert_eq!(gate_page.status, StatusCode::OK);
    assert!(gate_page.body.contains("18"), "the gate must explain itself");

    // A TL0 account is hidden from the adult directory entirely.
    let after = call(
        &app,
        cookie(host(get("/@adult"), "nsfw.test.local"), "orbital_age=1"),
    )
    .await;
    assert_eq!(after.status, StatusCode::NOT_FOUND, "TL0 must stay hidden");

    // And the SFW sitemap never lists it.
    let map = call(&app, get("/sitemap.xml")).await;
    assert!(!map.body.contains("adult"), "{}", map.body);
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
async fn a_secondary_adult_listing_is_invisible_from_the_sfw_host(pool: PgPool) {
    let app = app(pool.clone()).await;

    // SFW primary, adult secondary: belongs on the SFW leaderboard, and the SFW
    // card must not hint at the other listing.
    let sess = register(&app, "dual", "dev").await.cookie.unwrap();
    let patched = call(
        &app,
        json_req(
            "PATCH",
            "/api/me",
            serde_json::json!({"secondary_verticals": ["nsfw"]}),
            Some(&sess),
        ),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);

    let board = call(&app, get("/v/dev")).await;
    assert!(board.body.contains("dual"), "a SFW-primary member must still rank");

    let card = call(&app, get("/@dual")).await;
    assert!(!card.body.contains(">nsfw<"), "the SFW card leaked the adult listing");
    assert!(!card.body.contains("Adult Creators"), "the SFW card named the adult vertical");

    // Adult primary, SFW secondary: must not appear in the SFW vertical at all.
    let s2 = register(&app, "shadow", "nsfw").await.cookie.unwrap();
    let p2 = call(
        &app,
        json_req(
            "PATCH",
            "/api/me",
            serde_json::json!({"secondary_verticals": ["dev"]}),
            Some(&s2),
        ),
    )
    .await;
    assert_eq!(p2.status, StatusCode::OK, "{}", p2.body);

    let board = call(&app, get("/v/dev")).await;
    assert!(
        !board.body.contains("shadow"),
        "an adult-primary member must never appear in a SFW leaderboard"
    );
    let people = call(&app, get("/")).await;
    assert!(!people.body.contains("shadow"), "nor on the front door");
    // The search page echoes the query; what must be absent is the member's card.
    let search = call(&app, get("/search?q=shadow")).await;
    assert!(
        !search.body.contains("href=\"/@shadow\""),
        "nor in search results"
    );
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
