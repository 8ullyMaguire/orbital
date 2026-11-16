//! M2b's gate: the home's vertical cards read a bounded candidate pool and a count, not ten
//! full-roster decodes. The 30-member leader must still be the first candidate.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::AppState;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

#[sqlx::test]
async fn vertical_cards_are_bounded_and_keep_the_leader(pool: PgPool) {
    let mut ids: Vec<Uuid> = Vec::new();
    for i in 0..30 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (handle, display_name) VALUES ($1, $1) RETURNING id",
        )
        .bind(format!("dev-{i:02}"))
        .fetch_one(&pool)
        .await
        .expect("a user");
        sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'dev', 'primary')")
            .bind(id)
            .execute(&pool)
            .await
            .expect("a vertical membership");
        // Distinct sessions are distinct visitors; the leader has 30, the last member has 1.
        for s in 0..(30 - i) {
            sqlx::query(
                "INSERT INTO source_hits (user_id, surface, vertical_context, session_hash)
                 VALUES ($1, 'profile', 'dev', $2)",
            )
            .bind(id)
            .bind(format!("s-{i}-{s}"))
            .execute(&pool)
            .await
            .expect("a visit");
        }
        ids.push(id);
    }

    let bounded = orbital::store::vertical_members_limited(&pool, "dev", 24)
        .await
        .expect("bounded members");
    assert_eq!(bounded.len(), 24, "LIMIT is enforced before decode");
    assert_eq!(bounded[0].id, ids[0], "the 30-visitor leader survives the cut");
    assert!(
        !bounded.iter().any(|m| m.id == ids[29]),
        "the one-visitor tail is not decoded at all"
    );

    let count = orbital::store::vertical_member_count(&pool, "dev")
        .await
        .expect("count");
    assert_eq!(count, 30, "the pill counts members without decoding them");

    let cfg = Config::for_test("http://orbital.test");
    let state = AppState {
        pool: pool.clone(),
        cfg: std::sync::Arc::new(cfg),
        http: reqwest::Client::new(),
    };
    let app = orbital::build_router(state).await;
    let res = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("home responds");
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.expect("home body").to_bytes();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("href=\"/v/dev\""), "the dev card is served");
    assert!(
        body.contains("<span class=\"pill\">30</span>"),
        "the card pill shows the counted 30, not the 24 decoded candidates"
    );
}
