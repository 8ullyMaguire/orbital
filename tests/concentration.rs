//! The concentration baseline: Gini and top-10 share per surface, snapshotted weekly.
//!
//! The test that matters is the one with a known answer: a pool where one member takes nine tenths of the
//! impressions must read Gini > 0.8 and a top-10 share of ~90.9%, and a perfectly flat pool must read 0.
//! If the instrument cannot reproduce those two numbers it cannot be trusted with a policy.

use chrono::{NaiveDate, TimeZone, Utc};
use orbital::concentration::{self, TOP_K};
use sqlx::PgPool;
use uuid::Uuid;

/// The shape the fame headwind exists for: one member with 900 impressions and ninety-nine with one each.
/// Both numbers are hand-checkable — top-10 share (900 + 9·1)/999 = 0.9099, and
/// 2·99·899 / (2·100²·9.99) = 0.8909.
#[test]
fn one_member_with_nine_tenths_of_impressions_reads_as_concentrated() {
    let mut v: Vec<f64> = vec![900.0];
    v.extend(std::iter::repeat(1.0).take(99));

    let g = concentration::gini(&v);
    let t = concentration::top_share(&v, TOP_K);
    assert!((g - 0.8909).abs() < 0.001, "the coefficient has a hand-computable value, got {g}");
    assert!(g > 0.8, "one member holding nine tenths of the impressions is concentrated, got {g}");
    assert!((t - 0.9099).abs() < 0.001, "top-10 share should be 909/999, got {t}");
}

/// A pool where everybody was seen equally often has no concentration, whatever its size.
#[test]
fn a_flat_pool_reads_as_zero() {
    let flat = vec![7.0; 12];
    assert!(concentration::gini(&flat).abs() < 1e-9);
    assert!(
        (concentration::top_share(&flat, TOP_K) - 10.0 / 12.0).abs() < 1e-9,
        "ten of twelve equal members hold ten twelfths"
    );
    let few = vec![7.0; 5];
    assert!((concentration::top_share(&few, TOP_K) - 1.0).abs() < 1e-9, "a pool smaller than k is all of it");
}

/// An empty surface is not a concentration of zero — there is nothing to measure, and reporting 0.0 would
/// let a dead surface look healthy on a chart.
#[test]
fn an_empty_pool_has_no_answer() {
    assert_eq!(concentration::gini(&[]), 0.0);
    assert_eq!(concentration::top_share(&[], TOP_K), 0.0);
}

/// Two weeks produce two rows, and taking the same week twice corrects the row instead of appending one.
#[sqlx::test]
async fn two_weeks_produce_two_rows_and_the_week_is_idempotent(pool: PgPool) {
    let id: (Uuid,) = sqlx::query_as(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ('clicked', 'Clicked', 'x') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("a real member row");
    for _ in 0..4 {
        sqlx::query(
            "INSERT INTO source_hits (user_id, surface, session_hash) VALUES ($1, 'home', 'week-test')",
        )
            .bind(id.0)
            .execute(&pool)
            .await
            .expect("a real impression row");
    }

    let week_one = Utc.with_ymd_and_hms(2026, 11, 3, 9, 0, 0).unwrap();
    let week_two = Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap();
    concentration::record(&pool, week_one).await.expect("week one");
    concentration::record(&pool, week_one).await.expect("week one again");
    concentration::record(&pool, week_two).await.expect("week two");

    let rows: Vec<(NaiveDate, i32, i64)> = sqlx::query_as(
        "SELECT week, members, impressions FROM concentration_snapshots WHERE surface = 'home' ORDER BY week",
    )
    .fetch_all(&pool)
    .await
    .expect("rows");
    assert_eq!(rows.len(), 2, "two weeks, two rows — the repeat corrected rather than appended");
    assert_eq!(rows[0].1, 1);
    assert_eq!(rows[0].2, 4);
    assert_eq!(rows[1].0, NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(), "keyed by the ISO week's Monday");
}
