//! M2a's gate: the first public read from `entities` is bounded, excludes hidden and merged rows,
//! honours the SFW filter, and never mistakes an unclaimed shell for a claimed member.

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
        "INSERT INTO users (handle, display_name, password_hash) VALUES ($1, $1, $2) RETURNING id",
    )
    .bind(handle)
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

async fn subject(pool: &PgPool, entity_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO entity_people (entity_id, user_id, role) VALUES ($1, $2, 'subject')",
    )
    .bind(entity_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("a subject link");
}

async fn seed(pool: &PgPool) {
    sqlx::query("INSERT INTO categories (slug, label) VALUES ('dev', 'Developers') ON CONFLICT DO NOTHING")
        .execute(pool)
        .await
        .expect("a category");
    let account = user(pool, "beacon-owner", true).await;
    let shell = user(pool, "beacon-shell", false).await;
    let claimed = entity(pool, "beacon-foundry", "Beacon Foundry", false, false).await;
    let unclaimed = entity(pool, "quiet-gallery", "Quiet Gallery", false, false).await;
    let hidden = entity(pool, "hidden-workshop", "Hidden Workshop", false, true).await;
    let adult = entity(pool, "adult-archive", "Adult Archive", true, false).await;
    let merged = entity(pool, "merged-page", "Merged Page", false, false).await;
    sqlx::query("UPDATE entities SET merged_into = $1 WHERE id = $2")
        .bind(claimed)
        .bind(merged)
        .execute(pool)
        .await
        .expect("a merged row");
    subject(pool, claimed, account).await;
    subject(pool, unclaimed, shell).await;
    subject(pool, hidden, account).await;
    subject(pool, adult, account).await;
}

#[sqlx::test]
async fn catalogue_read_is_bounded_and_honest(pool: PgPool) {
    seed(&pool).await;

    let sfw = orbital::store::catalogue_entries(&pool, false, 10)
        .await
        .expect("sfw entries");
    assert_eq!(sfw.len(), 2, "hidden, merged and nsfw rows are excluded");
    let claimed = sfw.iter().find(|e| e.title == "Beacon Foundry").expect("claimed entry");
    assert!(claimed.claimed, "a password-holding subject makes the entity claimed");
    assert_eq!(claimed.handle, "beacon-owner");
    assert_eq!(claimed.category_label, "Developers");
    let shell = sfw.iter().find(|e| e.title == "Quiet Gallery").expect("shell entry");
    assert!(!shell.claimed, "an imported shell is not a claimed member");
    assert_eq!(shell.handle, "beacon-shell");

    let all = orbital::store::catalogue_entries(&pool, true, 10)
        .await
        .expect("all entries");
    assert_eq!(all.len(), 3, "the adult row appears only when asked for");

    let limited = orbital::store::catalogue_entries(&pool, true, 1)
        .await
        .expect("limited entries");
    assert_eq!(limited.len(), 1, "LIMIT is part of the read path's scale contract");
}

#[sqlx::test]
async fn home_renders_the_bounded_catalogue_section(pool: PgPool) {
    seed(&pool).await;
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
    assert!(body.contains("data-shelf=\"catalogue\""), "the section is served");
    assert!(body.contains("Beacon Foundry"), "a catalogue title is in the real body");
    assert!(body.contains("Unclaimed"), "the claim CTA is visible on a shell entry");
}
