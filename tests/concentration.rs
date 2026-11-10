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

/// THE GATE, the number the step must print: the share of a window's impressions that went to members below the
/// median exposure. Hand-computed, because a share nobody can check is a claim.
#[test]
fn the_long_tail_share_is_the_share_of_impressions_below_the_median() {
    // Three members: exposure 100, 50, 10; impressions 100, 50, 10.
    let rows = vec![(100.0, 100i64), (50.0, 50), (10.0, 10)];
    let lt = orbital::concentration::long_tail(&rows).expect("a window with impressions");
    // Median exposure 50, so the tail is the member at 10: 10 of 160 impressions.
    assert_eq!(lt.total, 160, "the denominator is every impression in the window");
    assert_eq!(lt.tail_members, 1);
    assert!((lt.share - 10.0 / 160.0).abs() < 1e-12, "{}", lt.share);

    // Half the pool below the median is a normal shape and the share is not zero.
    let rows = vec![(100.0, 90i64), (100.0, 90), (10.0, 10), (10.0, 10)];
    let lt = orbital::concentration::long_tail(&rows).expect("impressions");
    assert_eq!(lt.tail_members, 2);
    assert!((lt.share - 20.0 / 200.0).abs() < 1e-12);

    // A window with no impressions has no share — not a zero, which would read as "the tail got nothing".
    assert!(orbital::concentration::long_tail(&[(1.0, 0i64), (2.0, 0)]).is_none());
    assert!(orbital::concentration::long_tail(&[]).is_none());
}

/// The floor, and the refusal under it: the step may not claim a result off a handful of impressions.
#[test]
fn below_the_floor_there_is_no_verdict() {
    let rows = vec![(100.0, 100i64), (10.0, 10)];
    let lt = orbital::concentration::long_tail(&rows).expect("impressions");
    assert!(lt.total < orbital::concentration::LONG_TAIL_FLOOR);
    assert!(
        lt.verdict().starts_with("NO VERDICT"),
        "under {} impressions the answer is NO VERDICT, got {:?}",
        orbital::concentration::LONG_TAIL_FLOOR,
        lt.verdict()
    );
    // Enough impressions, but not enough members: with three the tail is one member and the share is arithmetic.
    let thin = vec![(100.0, 1000i64), (50.0, 500), (10.0, 400)];
    let lt = orbital::concentration::long_tail(&thin).expect("impressions");
    assert!(lt.total >= orbital::concentration::LONG_TAIL_FLOOR);
    assert!(
        lt.verdict().starts_with("NO VERDICT"),
        "{} members is not a pool: {}",
        lt.pool_members,
        lt.verdict()
    );

    // Both floors passed, and only then a share.
    let mut big: Vec<(f64, i64)> = (0..12).map(|i| (100.0 - i as f64, 100)).collect();
    big.push((1.0, 60));
    let lt = orbital::concentration::long_tail(&big).expect("impressions");
    assert!(lt.total >= orbital::concentration::LONG_TAIL_FLOOR);
    assert!(lt.pool_members >= orbital::concentration::LONG_TAIL_MIN_MEMBERS);
    assert!(lt.verdict().contains("long tail"), "over both floors it states the share: {}", lt.verdict());
}
