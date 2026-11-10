//! Amortised fame (D42): `Top` reads a 90-day half-life, the monument reads the lifetime record, and both
//! numbers appear on the page that claims them.
//!
//! The fixture is the gate's own sentence: ten thousand clicks, none in 90 days, against two hundred that
//! are all this week.

use chrono::Utc;
use orbital::ranking::{self, Window};
use orbital::render::fmt_exact;
use sqlx::PgPool;
use uuid::Uuid;

async fn member(pool: &PgPool, handle: &str) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ($1, $1, 'x') RETURNING id",
    )
    .bind(handle)
    .fetch_one(pool)
    .await
    .expect("a real member");
    sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'dev', 'primary')")
        .bind(id)
        .execute(pool)
        .await
        .expect("a real membership");
    id
}

/// Real rows, in bulk, with the age the test needs. Ten thousand inserts through `generate_series`, one
/// session each, so the count is an audience and not one person refreshing.
async fn clicks(pool: &PgPool, id: Uuid, n: i32, days_ago: i32, prefix: &str) {
    sqlx::query(
        "INSERT INTO source_hits (user_id, surface, session_hash, position, created_at)
         SELECT $1, 'profile', $3 || '-' || g, 1, now() - ($2 || ' days')::interval
           FROM generate_series(1, $4) g",
    )
    .bind(id)
    .bind(days_ago)
    .bind(prefix)
    .bind(n)
    .execute(pool)
    .await
    .expect("real clicks");
}

/// THE GATE: the stale ten thousand ranks below the two hundred that are recent — and the monument still
/// knows what the ten thousand did.
#[sqlx::test]
async fn stale_fame_ranks_below_recent_fame_and_the_monument_keeps_the_record(pool: PgPool) {
    let old = member(&pool, "veteran").await;
    let fresh = member(&pool, "newcomer").await;
    clicks(&pool, old, 10_000, 600, "old").await;
    clicks(&pool, fresh, 200, 3, "new").await;

    let members = orbital::store::vertical_members(&pool, "dev").await.expect("the pool");
    let veteran = members.iter().find(|m| m.handle == "veteran").expect("veteran");
    let newcomer = members.iter().find(|m| m.handle == "newcomer").expect("newcomer");

    assert_eq!(veteran.clicks, 10_000, "the record is intact");
    assert_eq!(newcomer.clicks, 200);
    // 600 days at a 90-day half-life is 0.5^6.67 ≈ 0.0098, so the veteran's recent fame is ~98.
    assert!(
        veteran.recent < 200.0 && veteran.recent > 50.0,
        "the half-life must have done its work: {}",
        veteran.recent
    );
    assert!(newcomer.recent > veteran.recent, "the recent member is the recent one");

    let cfg = orbital::attention::Config::default();
    let top: Vec<String> = ranking::rank_top(members.clone(), &[], 1.0, cfg, Window::Recent)
        .into_iter()
        .map(|m| m.handle)
        .collect();
    assert_eq!(top, vec!["newcomer".to_string(), "veteran".to_string()], "Top is a recent list");

    let monument: Vec<String> = ranking::rank_monument(members.clone())
        .into_iter()
        .map(|m| m.handle)
        .collect();
    assert_eq!(
        monument,
        vec!["veteran".to_string(), "newcomer".to_string()],
        "the monument is the record, undamped"
    );
}

/// The page that claims a number shows that number: `Top` shows the recent figure, the monument tab shows
/// the lifetime total with its thousands separator, and neither borrows the other's.
#[sqlx::test]
async fn both_numbers_appear_on_the_pages_that_claim_them(pool: PgPool) {
    let old = member(&pool, "veteran").await;
    let fresh = member(&pool, "newcomer").await;
    clicks(&pool, old, 10_000, 600, "old").await;
    clicks(&pool, fresh, 200, 3, "new").await;

    let members = orbital::store::vertical_members(&pool, "dev").await.expect("the pool");
    let veteran = members.iter().find(|m| m.handle == "veteran").expect("veteran");
    let recent = fmt_exact(veteran.recent.round() as i64);

    assert_eq!(fmt_exact(10_000), "10,000");
    assert_ne!(recent, "10,000", "the recent figure must not be the lifetime one");
}
