//! M3b's gate: the 30-day conversion snapshot is a read-only command with a fixed denominator,
//! and an empty population is NO VERDICT rather than a tidy-looking zero.

use orbital::models::ClaimConversion;
use sqlx::PgPool;

#[sqlx::test]
async fn claims_snapshot_counts_the_window_and_refuses_an_empty_ratio(pool: PgPool) {
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO users (handle, display_name, password_hash)
             VALUES ($1, $1, NULL)",
        )
        .bind(format!("shell-{i}"))
        .execute(&pool)
        .await
        .expect("a shell");
    }
    sqlx::query(
        "INSERT INTO users (handle, display_name, password_hash, claimed_at)
         VALUES ('claimant', 'Claimant', 'hash', now())",
    )
    .execute(&pool)
    .await
    .expect("a claimed member");
    sqlx::query(
        "INSERT INTO import_runs (source, rows_seen, rows_inserted, note, finished_at)
         VALUES ('claim-proof', 1, 1, 'test claim', now())",
    )
    .execute(&pool)
    .await
    .expect("a claim receipt");
    sqlx::query(
        "INSERT INTO import_runs (source, rows_seen, rows_inserted, note, finished_at)
         VALUES ('erasure', 1, 1, 'test erasure', now())",
    )
    .execute(&pool)
    .await
    .expect("an erasure receipt");

    let snap = orbital::ops::claim_conversion(&pool, 30)
        .await
        .expect("a conversion snapshot");
    assert_eq!(
        snap,
        ClaimConversion {
            days: 30,
            receipts: 1,
            claims: 1,
            shells: 3,
            erasures: 1,
        }
    );
    assert_eq!(snap.ratio(), Some(0.25));
    let text = snap.to_string();
    assert!(text.contains("claims 30 day(s)"), "the window is printed: {text}");
    assert!(text.contains("conversion 25.00%"), "the ratio is printed: {text}");

    // A window with no population must not be reported as 0%: that would look like a measurement.
    sqlx::query("DELETE FROM users").execute(&pool).await.expect("no users");
    sqlx::query("DELETE FROM import_runs")
        .execute(&pool)
        .await
        .expect("no receipts");
    let empty = orbital::ops::claim_conversion(&pool, 30)
        .await
        .expect("an empty snapshot");
    assert_eq!(empty.ratio(), None);
    assert!(empty.to_string().contains("NO VERDICT"));
}
