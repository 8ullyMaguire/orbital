//! M1's gate: the substrate fills once, a second run inserts nothing, accounts are never claimed
//! as somebody's subject, and a shared URL is one entity rather than two.

use sqlx::PgPool;
use uuid::Uuid;

async fn user(pool: &PgPool, handle: &str, with_password: bool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(handle)
    .bind(handle)
    .bind(if with_password { Some("hash") } else { None })
    .fetch_one(pool)
    .await
    .expect("a user")
}

async fn seed(pool: &PgPool, id: Uuid, url: &str) {
    sqlx::query("INSERT INTO links (user_id, url, label) VALUES ($1, $2, 'site')")
        .bind(id)
        .bind(url)
        .execute(pool)
        .await
        .expect("a link");
    sqlx::query("INSERT INTO tags (user_id, tag_slug) VALUES ($1, 'rust'), ($1, 'bad+tag')")
        .bind(id)
        .execute(pool)
        .await
        .expect("two tags, one of which entity_tags refuses");
    sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'dev', 'primary')")
        .bind(id)
        .execute(pool)
        .await
        .expect("a primary vertical");
}

#[sqlx::test]
async fn backfill_fills_the_substrate_and_the_second_run_inserts_nothing(pool: PgPool) {
    sqlx::query("INSERT INTO verticals (slug, label) VALUES ('dev', 'Developers') ON CONFLICT DO NOTHING")
        .execute(&pool)
        .await
        .expect("a vertical");
    let shell = user(&pool, "sample-creator", false).await;
    let account = user(&pool, "real-person", true).await;
    seed(&pool, shell, "https://example.com/creator").await;
    seed(&pool, account, "https://example.com/real").await;

    let first = orbital::store::backfill_catalogue(&pool, None)
        .await
        .expect("first run");
    assert_eq!(first.seen, 1, "accounts are not shells");
    assert_eq!(first.inserted, 1);
    assert_eq!(first.tags, 1, "bad+tag is refused by entity_tags' own CHECK");

    let second = orbital::store::backfill_catalogue(&pool, None)
        .await
        .expect("second run");
    assert_eq!(second.seen, 0, "the second run's own INSERT 0 proves idempotency");
    assert_eq!(second.inserted, 0);

    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM entities WHERE kind = 'person'")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(people, 1);
    let dev_category: i64 = sqlx::query_scalar("SELECT count(*) FROM categories WHERE slug = 'dev'")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(dev_category, 1, "categories were filled from verticals");
    let c = orbital::store::catalogue_composition(&pool)
        .await
        .expect("composition");
    assert_eq!(c.shells, 1);
    assert_eq!(c.subjects, 1);
    assert_eq!(c.entities, 1);
}

#[sqlx::test]
async fn backfill_merges_two_shells_that_share_one_surface(pool: PgPool) {
    let a = user(&pool, "first-creator", false).await;
    let b = user(&pool, "second-creator", false).await;
    seed(&pool, a, "https://example.com/shared").await;
    seed(&pool, b, "https://example.com/shared").await;

    let c = orbital::store::backfill_catalogue(&pool, None)
        .await
        .expect("run");
    assert_eq!(c.seen, 2);
    assert_eq!(c.inserted, 1);
    assert_eq!(c.conflicts, 1, "a shared own surface is one canonical entity");
}
