//! M3's gate: a shell can only be claimed with PDS-backed proof, and erasure is one transaction
//! whose second run reports zeros. No live shell is touched.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::{Json, Router};
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::AppState;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn pds(State(token): State<String>, uri: Uri) -> impl IntoResponse {
    let path = uri.path();
    let query = uri.query().unwrap_or_default();
    if path.contains("resolveHandle") {
        if query.contains("claimant.test") {
            Json(json!({ "did": "did:plc:claimant" }))
        } else if query.contains("noproof.test") {
            Json(json!({ "did": "did:plc:noproof" }))
        } else {
            Json(json!({}))
        }
    } else if path.contains("getProfile") && query.contains("did%3Aplc%3Aclaimant") {
        Json(json!({ "description": format!("orbital-verify:{token}") }))
    } else {
        Json(json!({ "description": "nothing here" }))
    }
}

async fn seed(pool: &PgPool) -> Uuid {
    sqlx::query("INSERT INTO categories (slug, label) VALUES ('dev', 'Makers') ON CONFLICT DO NOTHING")
        .execute(pool)
        .await
        .expect("a category");
    let uid: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ('claimable', 'Claimable', NULL) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("a shell");
    let eid: Uuid = sqlx::query_scalar(
        "INSERT INTO entities (slug, kind, canonical_key, title, category_slug)
         VALUES ('claimable', 'person', 'entity:claimable', 'Claimable', 'dev') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("an entity");
    sqlx::query("INSERT INTO entity_people (entity_id, user_id, role) VALUES ($1, $2, 'subject')")
        .bind(eid)
        .bind(uid)
        .execute(pool)
        .await
        .expect("a subject link");
    sqlx::query("INSERT INTO tags (user_id, tag_slug) VALUES ($1, 'maker')")
        .bind(uid)
        .execute(pool)
        .await
        .expect("a tag");
    sqlx::query("INSERT INTO entity_tags (entity_id, tag_slug) VALUES ($1, 'maker')")
        .bind(eid)
        .execute(pool)
        .await
        .expect("an entity tag");
    sqlx::query(
        "INSERT INTO links (user_id, url, label, entity_id)
         VALUES ($1, 'https://example.test/claimable', 'site', $2)",
    )
    .bind(uid)
    .bind(eid)
    .execute(pool)
    .await
    .expect("a link");
    sqlx::query("INSERT INTO source_hits (user_id, surface, session_hash) VALUES ($1, 'profile', 'claim-session')")
        .bind(uid)
        .execute(pool)
        .await
        .expect("a visit");
    uid
}

async fn post(app: &Router, body: Value) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/claim")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("claim responds");
    let status = res.status();
    let body = res.into_body().collect().await.expect("body").to_bytes();
    (status, String::from_utf8_lossy(&body).to_string())
}

#[sqlx::test]
async fn claim_needs_proof_and_erasure_is_idempotent(pool: PgPool) {
    let uid = seed(&pool).await;
    // A local PDS: only claimant.test has a DID, and only that DID carries the token.
    let token: String =
        sqlx::query_scalar("SELECT verify_token FROM users WHERE id = $1")
            .bind(uid)
            .fetch_one(&pool)
            .await
            .expect("the shell's token");
    let mock = Router::new().fallback(any(pds)).with_state(token.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a mock port");
    let addr = listener.local_addr().expect("a mock address");
    tokio::spawn(async move { axum::serve(listener, mock).await.expect("mock server") });

    let mut cfg = Config::for_test("http://orbital.test");
    cfg.bsky_pds = format!("http://{addr}");
    let state = AppState {
        pool: pool.clone(),
        cfg: std::sync::Arc::new(cfg),
        http: reqwest::Client::new(),
    };
    let app = orbital::build_router(state).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/claim-token?handle=claimable")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("token endpoint responds");
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.expect("body").to_bytes();
    let body: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(body["verify_token"], token);

    let payload = json!({
        "handle": "claimable",
        "bsky_handle": "unknown.test",
        "password": "a-long-claim-password"
    });
    let (status, text) = post(&app, payload.clone()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unproved claim refused: {text}");
    let claimed: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT claimed_at FROM users WHERE handle = 'claimable'")
            .fetch_one(&pool)
            .await
            .expect("a shell");
    assert!(claimed.is_none(), "a refused claim changes nothing");

    let mut proved = payload.clone();
    proved["bsky_handle"] = json!("claimant.test");
    let (status, text) = post(&app, proved.clone()).await;
    assert_eq!(status, StatusCode::OK, "proved claim accepted: {text}");
    let row: (Option<chrono::DateTime<chrono::Utc>>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT claimed_at, password_hash, bsky_did FROM users WHERE handle = 'claimable'")
            .fetch_one(&pool)
            .await
            .expect("a claimed member");
    assert!(row.0.is_some());
    assert!(row.1.is_some());
    assert_eq!(row.2.as_deref(), Some("did:plc:claimant"));
    let link_claimed: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT claimed_at FROM entity_people WHERE user_id = $1")
            .bind(uid)
            .fetch_one(&pool)
            .await
            .expect("a subject link");
    assert!(link_claimed.is_some());
    let entries = orbital::store::catalogue_entries(&pool, true, 10).await.expect("entries");
    let entry = entries.iter().find(|e| e.handle == "claimable").expect("the entity");
    assert!(entry.claimed, "the entity is now rankable, not a nofollow shell");

    let (status, _) = post(&app, proved).await;
    assert_eq!(status, StatusCode::CONFLICT, "a second claim is refused");

    let dry = orbital::ops::erase_shell(&pool, "claimable", false).await.expect("dry run");
    assert_eq!(dry.user, 1);
    assert_eq!(dry.entities, 1);
    assert_eq!(dry.links, 1);
    assert_eq!(dry.tags, 2);
    assert_eq!(dry.visits, 1);
    let still: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM users WHERE handle = 'claimable'")
        .fetch_one(&pool)
        .await
        .expect("still there");
    assert_eq!(still, 1, "a dry run changes nothing");

    let applied = orbital::ops::erase_shell(&pool, "claimable", true).await.expect("apply");
    assert_eq!(applied, dry);
    let second = orbital::ops::erase_shell(&pool, "claimable", true).await.expect("second apply");
    assert_eq!(second.user + second.entities + second.links + second.tags + second.visits, 0);
    let mut remaining = 0i64;
    for sql in [
        "SELECT count(*)::bigint FROM users WHERE handle = 'claimable'",
        "SELECT count(*)::bigint FROM links WHERE user_id = $1",
        "SELECT count(*)::bigint FROM tags WHERE user_id = $1",
        "SELECT count(*)::bigint FROM source_hits WHERE user_id = $1",
        "SELECT count(*)::bigint FROM entities WHERE slug = 'claimable'",
    ] {
        let mut q = sqlx::query_scalar::<_, i64>(sql);
        if sql.contains("$1") {
            q = q.bind(uid);
        }
        let n: i64 = q.fetch_one(&pool).await.expect("a clean table");
        remaining += n;
    }
    assert_eq!(remaining, 0);
}
