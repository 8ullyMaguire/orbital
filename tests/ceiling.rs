//! The step's gate: a ceiling that bites for a day and lets go the next, and a tail that rotates daily and
//! is reproducible from the date alone (D45).

use chrono::{Duration, NaiveDate, Utc};
use orbital::ceiling::{self, Config};
use sqlx::PgPool;
use uuid::Uuid;

fn ids(n: usize) -> Vec<Uuid> {
    // Deterministic ids, so a golden hash can be asserted: a rotation that changes with the RNG is not
    // reproducible from the date alone.
    (0..n)
        .map(|i| {
            let mut b = [0u8; 16];
            b[0] = i as u8;
            b[15] = 7;
            Uuid::from_bytes(b)
        })
        .collect()
}

/// THE GATE, first and second halves of the rotation: the top is the ranker's and does not move; the tail
/// changes between two consecutive days; and the same date always produces the same tail.
#[test]
fn two_consecutive_days_differ_in_the_tail_and_agree_on_the_top() {
    let ids = ids(24);
    let pool: Vec<usize> = (0..24).collect();
    let top: Vec<usize> = (0..6).collect();
    let cfg = Config::default();
    assert_eq!(cfg.tail_slots, 2, "the declaration under test keeps the last two slots rotating");

    let today = NaiveDate::from_ymd_opt(2026, 11, 10).expect("a real date");
    let tomorrow = today + Duration::days(1);
    let a = ceiling::rotate(&top, &pool, &ids, ceiling::day_key(today), &cfg);
    let b = ceiling::rotate(&top, &pool, &ids, ceiling::day_key(tomorrow), &cfg);

    assert_eq!(&a[..4], &top[..4], "the top of the shelf is not the rotation's to move: {a:?}");
    assert_eq!(&b[..4], &top[..4]);
    assert_eq!(a.len(), 6, "rotation may reorder the tail, never shorten the shelf");
    assert_ne!(a[4..], b[4..], "two consecutive days must not show the same tail: {a:?} vs {b:?}");

    let again = ceiling::rotate(&top, &pool, &ids, ceiling::day_key(today), &cfg);
    assert_eq!(a, again, "the same date must produce the same page, byte for byte");
}

/// The hash is written out, not taken from `std`, and pinned here: a rotation that changes when the compiler
/// changes is not reproducible from the date alone.
#[test]
fn the_daily_hash_is_pinned_and_not_the_standard_librarys() {
    let id = Uuid::from_bytes([0u8; 16]);
    // The golden value: FNV-1a over 24 zero bytes (eight for the day, sixteen for the id). Pinned because
    // the promise is that the same date and the same member give the same slot, in any build.
    assert_eq!(ceiling::hash64(0, &id), 9_354_609_568_656_401_157);
    assert_eq!(ceiling::hash64(0, &id), ceiling::hash64(0, &id));
    assert_ne!(ceiling::hash64(0, &id), ceiling::hash64(1, &id), "the day is an input");
    let mut other = [0u8; 16];
    other[0] = 1;
    assert_ne!(ceiling::hash64(0, &id), ceiling::hash64(0, &Uuid::from_bytes(other)), "the member is an input");
}

/// The declaration's escape hatch is total: with tail_slots 0 the order is the ranker's own.
#[test]
fn the_rotation_can_be_declared_off() {
    let ids = ids(24);
    let pool: Vec<usize> = (0..24).collect();
    let top: Vec<usize> = (0..6).collect();
    let out = ceiling::rotate(&top, &pool, &ids, 20_000, &Config::off());
    assert_eq!(out, top);
}

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

/// THE GATE, the ceiling: a member over the share is capped for the day the impressions happened and is
/// uncapped once they fall out of the 24-hour window — proved by moving the buckets back a day, not by
/// waiting.
#[sqlx::test]
async fn the_ceiling_bites_for_a_day_and_lets_go_the_next(pool: PgPool) {
    for handle in ["loud", "bravo", "charlie", "delta", "echo", "foxtrot"] {
        member(&pool, handle).await;
    }
    let loud: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE handle = 'loud'")
        .fetch_one(&pool)
        .await
        .expect("a member");

    // 100 impressions for one member and one for each of five others: 105 in the window, 95 % for one.
    sqlx::query(
        "INSERT INTO impressions (surface, user_id, bucket, count)
         VALUES ('home', $1, date_trunc('hour', now()), 100)",
    )
    .bind(loud)
    .execute(&pool)
    .await
    .expect("real impressions");
    sqlx::query(
        "INSERT INTO impressions (surface, user_id, bucket, count)
         SELECT 'home', id, date_trunc('hour', now()), 1 FROM users WHERE handle <> 'loud'",
    )
    .execute(&pool)
    .await
    .expect("real impressions");

    let window = orbital::store::impressions_window(&pool, "home", 24).await.expect("the window");
    let total: i64 = window.values().sum();
    assert_eq!(total, 105);
    assert_eq!(window.get(&loud).copied().unwrap_or(0), 100);

    let cfg = Config::default();
    assert!(
        ceiling::over_ceiling(window[&loud], total, &cfg),
        "100 of 105 is over a 50% ceiling"
    );
    let quiet = window.iter().find(|(id, _)| **id != loud).map(|(_, n)| *n).unwrap_or(0);
    assert!(!ceiling::over_ceiling(quiet, total, &cfg), "one of 105 is not");

    // The next day: the same rows, one day older, are outside the window and nobody is capped.
    sqlx::query("UPDATE impressions SET bucket = bucket - interval '25 hours'")
        .execute(&pool)
        .await
        .expect("the window moves");
    let next = orbital::store::impressions_window(&pool, "home", 24).await.expect("the window");
    assert!(next.is_empty(), "yesterday's impressions are not today's: {next:?}");
    assert!(!ceiling::over_ceiling(100, 0, &cfg), "with no impressions in the window, nobody is capped");
}

/// Real rows get recorded by a real render, and the recording is a count per hour, not a row per card.
#[sqlx::test]
async fn a_render_records_what_it_showed_once_per_hour(pool: PgPool) {
    let a = member(&pool, "alpha").await;
    let b = member(&pool, "bravo").await;
    orbital::store::record_impressions(&pool, "home", &[a, b])
        .await
        .expect("recorded");
    orbital::store::record_impressions(&pool, "home", &[a, b])
        .await
        .expect("recorded again");

    let window = orbital::store::impressions_window(&pool, "home", 24).await.expect("the window");
    assert_eq!(window.get(&a).copied(), Some(2), "two renders, two impressions");
    assert_eq!(window.values().sum::<i64>(), 4, "and one row per member per hour, not per card");
}
