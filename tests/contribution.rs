//! D60 gates: contribution intake, the trust-gated action log, and the anonymous write budget.
//! Each test drives the production router against the migration-created schema.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::contribution::GateMode;
use orbital::{auth, AppState};
use polaris_ratelimit::{Limiter, Limits, Quota};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

fn cfg() -> Config {
    Config::for_test("http://orbital.test")
}

async fn app_with(pool: PgPool, cfg: Config) -> Router {
    let state = AppState {
        pool,
        cfg: Arc::new(cfg),
        http: reqwest::Client::new(),
    };
    orbital::build_router(state).await
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, String, Option<String>) {
    let res = app.clone().oneshot(req).await.expect("router answered");
    let status = res.status();
    let cookie = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("orbital_s="))
        .map(|v| v.split(';').next().unwrap_or("").to_string());
    let retry = res
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = res.into_body().collect().await.expect("body").to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned(), cookie.or(retry))
}

async fn post(app: &Router, path: &str, body: Value, xff: &str, cookie: Option<&str>) -> (StatusCode, String, Option<String>) {
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("x-forwarded-for", xff);
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    call(app, req.body(Body::from(body.to_string())).unwrap()).await
}

async fn get(app: &Router, path: &str, cookie: &str) -> (StatusCode, String, Option<String>) {
    call(app, Request::get(path).header("cookie", cookie).body(Body::empty()).unwrap()).await
}

/// A real account with a real password hash, then a real login, so the session cookie is the one a
/// browser would hold rather than a fixture-invented token.
async fn member(pool: &PgPool, handle: &str, trust: i16) -> String {
    let hash = auth::hash_password("a-long-test-password").expect("hash");
    sqlx::query(
        "INSERT INTO users (handle, display_name, password_hash, trust_level)
         VALUES ($1, $1, $2, $3)",
    )
    .bind(handle).bind(&hash).bind(trust)
    .execute(pool).await.expect("member");
    let app = app_with(pool.clone(), cfg()).await;
    let (status, _, cookie) = post(
        &app, "/api/auth/login",
        json!({"handle": handle, "password": "a-long-test-password"}),
        "198.51.100.9", None,
    ).await;
    assert_eq!(status, StatusCode::OK, "login should work");
    cookie.expect("session cookie")
}

#[sqlx::test]
async fn anonymous_profile_intake_is_logged_and_idempotent_on_the_canonical_url(pool: PgPool) {
    let app = app_with(pool.clone(), cfg()).await;
    let input = json!({
        "url": "https://allmylinks.com/daphomet",
        "display_name": "Mommy Daphomet",
        "vertical": "nsfw",
    });
    let (s1, b1, _) = post(&app, "/api/contribute/profile", input.clone(), "198.51.100.1", None).await;
    assert_eq!(s1, StatusCode::CREATED, "{b1}");
    let first: Value = serde_json::from_str(&b1).unwrap();
    assert_eq!(first["already_existing"], false);
    assert_eq!(first["display_name"], "Mommy Daphomet");

    // www. and a trailing slash are the same canonical key: no second shell.
    let mut again = input.clone();
    again["url"] = json!("https://www.allmylinks.com/daphomet/");
    let (s2, b2, _) = post(&app, "/api/contribute/profile", again, "198.51.100.2", None).await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    let second: Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(second["already_existing"], true);
    assert_eq!(second["handle"], first["handle"]);

    let users: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM users WHERE handle = $1",
    ).bind(first["handle"].as_str().unwrap())
        .fetch_one(&pool).await.unwrap();
    assert_eq!(users, 1, "one profile for one canonical link");
    let logged: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM contribution_log WHERE action = 'profile.create' AND decision = 'applied'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(logged, 1, "a no-change repeat is a lookup, not a logged decision");
}

#[sqlx::test]
async fn modlog_is_a_user_surface_gated_by_trust(pool: PgPool) {
    let low = member(&pool, "lowtrust", 0).await;
    let high = member(&pool, "hightrust", 2).await;
    let app = app_with(pool.clone(), cfg()).await;
    let (s1, b1, _) = get(&app, "/api/modlog", &low).await;
    assert_eq!(s1, StatusCode::FORBIDDEN, "{b1}");
    let (s2, b2, _) = get(&app, "/api/modlog", &high).await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    let list: Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(list["entries"], json!([]));
    let (s3, b3, _) = get(&app, "/api/modlog/summary", &high).await;
    assert_eq!(s3, StatusCode::OK, "{b3}");
    let summary: Value = serde_json::from_str(&b3).unwrap();
    assert_eq!(summary["declaration"]["modlog.review"], "trust(2)");
    assert_eq!(summary["population_signal"]["logged_in_actors"], 0,
        "imported shells are not a growth signal");
}

#[sqlx::test]
async fn anonymous_writes_have_a_per_client_budget_with_a_true_retry_after(pool: PgPool) {
    let mut c = cfg();
    c.contribution.enabled = true;
    c.contribution.limiter = Arc::new(Limiter::new(
        Limits::new(Quota::per_minute(240))
            .class(orbital::contribution::CLASS_ANON_WRITE, Quota {
                permits: 2, per: std::time::Duration::from_secs(300),
            })
            .class(orbital::contribution::CLASS_USER_WRITE, Quota::per_minute(120)),
    ));
    let app = app_with(pool.clone(), c).await;
    let body = |n: u32| json!({"url": format!("https://example.com/budget-{n}"), "vertical": "nsfw"});
    let (s1, b1, _) = post(&app, "/api/contribute/profile", body(1), "198.51.100.10", None).await;
    let (s2, b2, _) = post(&app, "/api/contribute/profile", body(2), "198.51.100.10", None).await;
    assert_eq!(s1, StatusCode::CREATED, "{b1}");
    assert_eq!(s2, StatusCode::CREATED, "{b2}");
    let (s3, b3, retry) = post(&app, "/api/contribute/profile", body(3), "198.51.100.10", None).await;
    assert_eq!(s3, StatusCode::TOO_MANY_REQUESTS, "{b3}");
    assert!(retry.unwrap_or_default().parse::<u64>().unwrap_or(0) > 0, "retry-after is numeric");
    let refused: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM contribution_log WHERE decision = 'refused' AND reason = 'rate limited'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(refused, 1, "a refusal is an audited decision");

    let (s4, b4, _) = post(&app, "/api/contribute/profile", body(4), "198.51.100.11", None).await;
    assert_eq!(s4, StatusCode::CREATED, "a different client has its own bucket: {b4}");
}

#[sqlx::test]
async fn a_closed_gate_refuses_before_the_write_and_records_the_refusal(pool: PgPool) {
    let mut c = cfg();
    c.contribution = c.contribution.with_gate("profile.create", GateMode::Grant);
    let app = app_with(pool.clone(), c).await;
    let (status, body, _) = post(
        &app, "/api/contribute/profile",
        json!({"url": "https://example.com/closed", "vertical": "nsfw"}),
        "198.51.100.20", None,
    ).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    let users: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM users")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(users, 0, "the gate ran before the INSERT");
    let refused: (String, String) = sqlx::query_as(
        "SELECT decision, gate_mode FROM contribution_log WHERE action = 'profile.create'",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(refused, ("refused".into(), "grant".into()));
}

#[sqlx::test]
async fn moderation_needs_a_reason_and_a_higher_trust_than_the_subject(pool: PgPool) {
    let low = member(&pool, "subject", 0).await;
    let mid = member(&pool, "moderator", 2).await;
    let app = app_with(pool.clone(), cfg()).await;
    let (_, intake, _) = post(
        &app, "/api/contribute/profile",
        json!({"url": "https://example.com/reviewed", "vertical": "nsfw"}),
        "198.51.100.30", None,
    ).await;

    // Seed a contribution whose actor is the TL0 member, through the production link route.
    let cookie_low = low;
    let (s_link, b_link, _) = post(
        &app, "/api/auth/login",
        json!({"handle": "subject", "password": "a-long-test-password"}),
        "198.51.100.31", None,
    ).await;
    assert_eq!(s_link, StatusCode::OK, "{b_link}");
    let _ = intake;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO contribution_log (action, decision, actor_id, actor_handle, subject_type, subject_id, reason, gate_mode)
         VALUES ('link.add','applied',(SELECT id FROM users WHERE handle='subject'),'subject','link','https://example.com/reviewed','member added a link','open') RETURNING id",
    ).fetch_one(&pool).await.unwrap();

    let (s_no, b_no, _) = post(
        &app, &format!("/api/moderation/{id}"),
        json!({"action": "moderation.act", "reason": "  "}), "198.51.100.32", Some(&mid),
    ).await;
    assert_eq!(s_no, StatusCode::BAD_REQUEST, "{b_no}");

    let (s_ok, b_ok, _) = post(
        &app, &format!("/api/moderation/{id}"),
        json!({"action": "moderation.act", "reason": "duplicate of an existing profile"}),
        "198.51.100.32", Some(&mid),
    ).await;
    assert_eq!(s_ok, StatusCode::CREATED, "{b_ok}");
    let row: Value = serde_json::from_str(&b_ok).unwrap();
    assert_eq!(row["reverses"], id);
    assert_eq!(row["actor"], "moderator");

    let (s_self, b_self, _) = post(
        &app, &format!("/api/moderation/{id}"),
        json!({"action": "moderation.act", "reason": "my own link"}),
        "198.51.100.32", Some(&cookie_low),
    ).await;
    // TL0 cannot reach the moderation capability in the first place.
    assert_eq!(s_self, StatusCode::FORBIDDEN, "{b_self}");
}

#[sqlx::test]
async fn modlog_public_projection_never_carries_account_ids_or_client_hashes(pool: PgPool) {
    let rev = member(&pool, "reviewer", 2).await;
    let owner_hash = auth::hash_password("a-long-test-password").unwrap();
    sqlx::query(
        "INSERT INTO users (handle, display_name, password_hash, trust_level, is_owner)
         VALUES ('owner','Owner',$1,3,true)",
    ).bind(&owner_hash).execute(&pool).await.unwrap();
    let app = app_with(pool.clone(), cfg()).await;
    let (s_login, login_body, owner_cookie) = post(
        &app, "/api/auth/login",
        json!({"handle": "owner", "password": "a-long-test-password"}), "198.51.100.40", None,
    ).await;
    assert_eq!(s_login, StatusCode::OK, "owner login: {login_body}");
    let owner_cookie = owner_cookie.expect("session cookie");

    let (s, body, _) = post(
        &app, "/api/contribute/profile",
        json!({"url": "https://example.com/proj", "vertical": "nsfw"}),
        "198.51.100.41", None,
    ).await;
    assert_eq!(s, StatusCode::CREATED, "{body}");

    let (_, rev_body, _) = get(&app, "/api/modlog", &rev).await;
    let rev_json: Value = serde_json::from_str(&rev_body).unwrap();
    let entry = &rev_json["entries"][0];
    assert!(entry.get("actor_id").is_none(), "no account id in a TL2 view: {entry}");
    assert!(entry.get("client_hash").is_none(), "no client hash in a TL2 view: {entry}");
    assert_eq!(entry["actor"], json!(null), "an anonymous contribution has no handle");

    // A signed-in contribution names its author to TL2; that is the whole point of review.
    let (s2, b2, _) = post(
        &app, "/api/contribute/profile",
        json!({"url": "https://example.com/attributed", "display_name": "Attributed", "vertical": "nsfw"}),
        "198.51.100.42", Some(&rev),
    ).await;
    assert_eq!(s2, StatusCode::CREATED, "a signed-in contributor creates a profile: {b2}");
    let (_, rev_body2, _) = get(&app, "/api/modlog?actor=reviewer", &rev).await;
    let rev_json2: Value = serde_json::from_str(&rev_body2).unwrap();
    assert_eq!(rev_json2["entries"][0]["actor"], json!("reviewer"));

    let (_, own_body, _) = get(&app, "/api/modlog", &owner_cookie).await;
    let own_json: Value = serde_json::from_str(&own_body).unwrap();
    assert!(
        own_json["entries"][0].get("actor_id").is_some(),
        "the owner sees the private fields: {own_body}"
    );
}

#[sqlx::test]
async fn a_daily_cap_stops_dozen_a_day_anonymous_profiles_even_when_the_burst_is_wide(pool: PgPool) {
    let mut c = cfg();
    c.contribution.enabled = true;
    c.contribution.limiter = Arc::new(Limiter::new(
        Limits::new(Quota::per_minute(240))
            .class(orbital::contribution::CLASS_ANON_WRITE, Quota::per_minute(240))
            .class(orbital::contribution::CLASS_ANON_DAY, Quota {
                permits: 2, per: std::time::Duration::from_secs(86_400),
            }),
    ));
    let app = app_with(pool.clone(), c).await;
    let body = |n: u32| json!({"url": format!("https://example.com/daily-{n}"), "vertical": "nsfw"});
    for n in 1..=2 {
        let (s, b, _) = post(&app, "/api/contribute/profile", body(n), "198.51.100.50", None).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
    }
    let (s, b, retry) = post(&app, "/api/contribute/profile", body(3), "198.51.100.50", None).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{b}");
    assert!(retry.unwrap_or_default().parse::<u64>().unwrap_or(0) > 0, "honest retry-after");
    // A fresh client still has its own daily allowance.
    let (s, b, _) = post(&app, "/api/contribute/profile", body(4), "198.51.100.51", None).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
}

#[sqlx::test]
async fn finding_a_url_is_not_authority_to_rename_the_profile(pool: PgPool) {
    let app = app_with(pool.clone(), cfg()).await;
    let (s, b, _) = post(
        &app, "/api/contribute/profile",
        json!({"url": "https://example.com/rename-me", "display_name": "Old Name", "vertical": "nsfw"}),
        "198.51.100.60", None,
    ).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");

    // The same canonical URL, a different name, no session: refused and logged.
    let (s, b, _) = post(
        &app, "/api/contribute/profile",
        json!({"url": "https://example.com/rename-me", "display_name": "Squatted Name", "vertical": "nsfw"}),
        "198.51.100.60", None,
    ).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let name: String = sqlx::query_scalar(
        "SELECT display_name FROM users WHERE handle = (SELECT subject_id FROM contribution_log WHERE reason = 'anonymous rename of an existing profile' LIMIT 1)",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(name, "Old Name", "the refused write did not land");

    // A session may correct it, and the correction names its author.
    let reviewer = member(&pool, "corrector", 1).await;
    let (s, b, _) = post(
        &app, "/api/contribute/profile",
        json!({"url": "https://example.com/rename-me", "display_name": "Corrected Name", "vertical": "nsfw"}),
        "198.51.100.61", Some(&reviewer),
    ).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let actor: Option<String> = sqlx::query_scalar(
        "SELECT actor_handle FROM contribution_log WHERE actor_handle = 'corrector' AND decision = 'applied' LIMIT 1",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(actor.as_deref(), Some("corrector"));
}
