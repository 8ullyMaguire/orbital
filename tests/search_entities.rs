//! M2c's last gate: search reads the catalogue as a second bounded source, without changing the
//! member results it already returned.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::AppState;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn user(pool: &PgPool, handle: &str, with_password: bool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(handle)
    .bind(format!("{handle} person"))
    .bind(if with_password { Some("hash") } else { None })
    .fetch_one(pool)
    .await
    .expect("a user")
}

async fn entity(pool: &PgPool, slug: &str, title: &str, nsfw: bool, hidden: bool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO entities (slug, kind, canonical_key, title, category_slug, nsfw, hidden)
         VALUES ($1, 'person', $2, $3, 'dev', $4, $5) RETURNING id",
    )
    .bind(slug)
    .bind(format!("entity:{slug}"))
    .bind(title)
    .bind(nsfw)
    .bind(hidden)
    .fetch_one(pool)
    .await
    .expect("an entity")
}

async fn seed(pool: &PgPool) {
    sqlx::query("INSERT INTO categories (slug, label) VALUES ('dev', 'Developers') ON CONFLICT DO NOTHING")
        .execute(pool)
        .await
        .expect("a category");
    let account = user(pool, "patreon-person", true).await;
    let shell = user(pool, "patreon-shell", false).await;
    let claimed = entity(pool, "patreon-archive", "Patreon Archive", false, false).await;
    let unclaimed = entity(pool, "patreon-gallery", "Patreon Gallery", false, false).await;
    let hidden = entity(pool, "patreon-hidden", "Patreon Hidden", false, true).await;
    let adult = entity(pool, "patreon-adult", "Patreon Adult", true, false).await;
    for (e, u) in [(claimed, account), (unclaimed, shell), (hidden, account), (adult, account)] {
        sqlx::query("INSERT INTO entity_people (entity_id, user_id, role) VALUES ($1, $2, 'subject')")
            .bind(e)
            .bind(u)
            .execute(pool)
            .await
            .expect("a subject link");
    }
}

#[sqlx::test]
async fn search_reads_the_catalogue_but_keeps_member_results(pool: PgPool) {
    seed(&pool).await;

    let sfw = orbital::store::search_entities(&pool, "patreon", false, 12)
        .await
        .expect("sfw entity matches");
    let titles: Vec<String> = sfw.iter().map(|e| e.title.clone()).collect();
    assert_eq!(
        titles,
        vec!["Patreon Archive".to_string(), "Patreon Gallery".to_string()],
        "hidden and nsfw entities are excluded; claimed and unclaimed are both visible"
    );
    let claimed = sfw.iter().find(|e| e.title == "Patreon Archive").unwrap();
    assert!(claimed.claimed);
    let shell = sfw.iter().find(|e| e.title == "Patreon Gallery").unwrap();
    assert!(!shell.claimed);

    let all = orbital::store::search_entities(&pool, "patreon", true, 12)
        .await
        .expect("all entity matches");
    assert_eq!(all.len(), 3, "the adult row appears only when asked for");
    let limited = orbital::store::search_entities(&pool, "patreon", true, 1)
        .await
        .expect("one match");
    assert_eq!(limited.len(), 1);

    let cfg = Config::for_test("http://orbital.test");
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
                .uri("/search?q=patreon")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("search responds");
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.expect("body").to_bytes();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("data-shelf=\"catalogue-search\""), "the catalogue section is served");
    assert!(body.contains("Patreon Archive"), "a catalogue title is in the real body");
    assert!(body.contains("Patreon Gallery"), "the unclaimed entry is served");
    assert!(body.contains("patreon-person"), "the member result still appears");
    assert!(!body.contains("Patreon Hidden"), "hidden entities never surface");

    let res = app
        .oneshot(
            Request::builder()
                .uri("/search?q=zzz-no-match")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("empty search responds");
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.expect("body").to_bytes();
    let body = String::from_utf8_lossy(&body);
    assert!(!body.contains("data-shelf=\"catalogue-search\""), "no empty catalogue section");
}
