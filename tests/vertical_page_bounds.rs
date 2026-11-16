//! M2c's gate: the vertical page renders 100 cards from a 300-row candidate pool while its counts
//! still describe the whole vertical. A test that only checked for 200 would pass on the old code.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::AppState;
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test]
async fn vertical_page_is_bounded_but_its_counts_are_whole(pool: PgPool) {
    for i in 0..120 {
        let id: sqlx::types::Uuid = sqlx::query_scalar(
            "INSERT INTO users (handle, display_name) VALUES ($1, $1) RETURNING id",
        )
        .bind(format!("maker-{i:03}"))
        .fetch_one(&pool)
        .await
        .expect("a user");
        sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'dev', 'primary')")
            .bind(id)
            .execute(&pool)
            .await
            .expect("membership");
        sqlx::query("INSERT INTO tags (user_id, tag_slug) VALUES ($1, 'maker')")
            .bind(id)
            .execute(&pool)
            .await
            .expect("a tag");
        sqlx::query(
            "INSERT INTO source_hits (user_id, surface, vertical_context, session_hash)
             VALUES ($1, 'profile', 'dev', $2)",
        )
        .bind(id)
        .bind(format!("session-{i}"))
        .execute(&pool)
        .await
        .expect("a visit");
    }

    let candidates = orbital::store::vertical_members_limited(&pool, "dev", 300)
        .await
        .expect("bounded candidates");
    assert_eq!(candidates.len(), 120, "120 members fit inside the 300-row pool");
    assert_eq!(
        orbital::store::vertical_member_count(&pool, "dev").await.expect("count"),
        120
    );
    assert_eq!(
        orbital::store::vertical_visitors_total(&pool, "dev").await.expect("visitors"),
        120
    );
    assert_eq!(
        orbital::store::vertical_trending_count(&pool, "dev").await.expect("trending"),
        120
    );
    let tags = orbital::store::vertical_tag_counts(&pool, "dev", 18)
        .await
        .expect("tag counts");
    assert_eq!(tags, vec![("maker".to_string(), 120)]);

    let cfg = Config::for_test("http://orbital.test");
    let state = AppState {
        pool: pool.clone(),
        cfg: std::sync::Arc::new(cfg),
        http: reqwest::Client::new(),
    };
    let app = orbital::build_router(state).await;
    let res = app
        .oneshot(Request::builder().uri("/v/dev").body(Body::empty()).unwrap())
        .await
        .expect("vertical responds");
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.expect("body").to_bytes();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("120 people"), "the header counts the whole vertical");
    let rows = body.matches("<li class=\"row\"").count();
    assert_eq!(rows, 100, "only the 100 rendered rows are decoded into cards");
    assert!(body.contains("#maker"), "the SQL-derived tag chip is served");
}
