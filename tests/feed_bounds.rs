//! M2c's feed gate: the site feeds emit 100 items from 100 decoded rows, not from a whole-roster
//! decode. Order stays the one `all_members` has always used.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::AppState;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test]
async fn feeds_are_bounded_and_keep_their_order(pool: PgPool) {
    for i in 0..120 {
        let id: sqlx::types::Uuid = sqlx::query_scalar(
            "INSERT INTO users (handle, display_name, created_at)
             VALUES ($1, $1, now() - make_interval(secs => $2)) RETURNING id",
        )
        .bind(format!("feed-{i:03}"))
        .bind(i as f64)
        .fetch_one(&pool)
        .await
        .expect("a user");
        sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'dev', 'primary')")
            .bind(id)
            .execute(&pool)
            .await
            .expect("membership");
    }

    let all = orbital::store::all_members(&pool, true).await.expect("all members");
    let limited = orbital::store::all_members_limited(&pool, true, 10)
        .await
        .expect("limited members");
    assert_eq!(limited.len(), 10);
    let all_head: Vec<String> = all.iter().take(10).map(|m| m.handle.clone()).collect();
    let limited_head: Vec<String> = limited.iter().map(|m| m.handle.clone()).collect();
    assert_eq!(limited_head, all_head, "the bounded read is the head of the old ordering");

    let cfg = Config::for_test("http://orbital.test");
    let state = AppState {
        pool: pool.clone(),
        cfg: std::sync::Arc::new(cfg),
        http: reqwest::Client::new(),
    };
    let app = orbital::build_router(state).await;

    async fn get(app: &axum::Router, uri: &str) -> String {
        let res = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .expect("a response");
        assert_eq!(res.status(), StatusCode::OK, "{uri}");
        let body = res.into_body().collect().await.expect("body").to_bytes();
        String::from_utf8_lossy(&body).to_string()
    }

    let json = get(&app, "/feed.json").await;
    let doc: serde_json::Value = serde_json::from_str(&json).expect("valid json feed");
    assert_eq!(doc["items"].as_array().map(|a| a.len()), Some(100));
    assert!(doc["items"][0]["id"].as_str().unwrap().contains("feed-000"));

    let xml = get(&app, "/feed.xml").await;
    assert_eq!(xml.matches("<item>").count(), 100, "RSS emits 100 items");

    let vertical = get(&app, "/v/dev/feed.json").await;
    let doc: serde_json::Value = serde_json::from_str(&vertical).expect("valid vertical feed");
    assert_eq!(doc["items"].as_array().map(|a| a.len()), Some(100));

    let sitemap = get(&app, "/sitemap.xml").await;
    assert!(sitemap.contains("/v/dev"), "a populated vertical is still listed");
    assert_eq!(json!(sitemap.contains("/v/dev")), json!(true));
}
