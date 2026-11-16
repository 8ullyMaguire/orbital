//! M5b's gate: the link budget checks known failures first, and the failure count is printed with
//! its denominator instead of alone.

use orbital::models::LinkCoverage;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn link_budget_orders_failures_then_new_then_clicked(pool: PgPool) {
    let uid: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name) VALUES ('link-owner', 'Link Owner') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("a user");

    async fn link(pool: &PgPool, uid: Uuid, label: &str, sql: &str) -> Uuid {
        sqlx::query(sql)
            .bind(uid)
            .bind(format!("https://example.test/{label}"))
            .bind(label)
            .execute(pool)
            .await
            .expect("a link");
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM links WHERE label = $1")
            .bind(label)
            .fetch_one(pool)
            .await
            .expect("the link id")
    }

    let failing = link(
        &pool,
        uid,
        "failing",
        "INSERT INTO links (user_id, url, label, last_checked, fail_count, position)
         VALUES ($1, $2, $3, now(), 3, 10)",
    )
    .await;
    let flaky = link(
        &pool,
        uid,
        "flaky",
        "INSERT INTO links (user_id, url, label, last_checked, fail_count, position)
         VALUES ($1, $2, $3, now() - interval '1 hour', 1, 11)",
    )
    .await;
    let new1 = link(
        &pool,
        uid,
        "new1",
        "INSERT INTO links (user_id, url, label, position) VALUES ($1, $2, $3, 12)",
    )
    .await;
    let new2 = link(
        &pool,
        uid,
        "new2",
        "INSERT INTO links (user_id, url, label, position) VALUES ($1, $2, $3, 13)",
    )
    .await;
    let clicked = link(
        &pool,
        uid,
        "clicked",
        "INSERT INTO links (user_id, url, label, last_checked, click_count, position)
         VALUES ($1, $2, $3, now() - interval '2 hours', 5, 14)",
    )
    .await;
    let plain = link(
        &pool,
        uid,
        "plain",
        "INSERT INTO links (user_id, url, label, last_checked, position)
         VALUES ($1, $2, $3, now() - interval '1 hour', 15)",
    )
    .await;
    link(
        &pool,
        uid,
        "hidden",
        "INSERT INTO links (user_id, url, label, fail_count, hidden)
         VALUES ($1, $2, $3, 9, true)",
    )
    .await;

    let order: Vec<Uuid> = orbital::store::links_to_check(&pool, 6)
        .await
        .expect("link budget")
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        order,
        vec![failing, flaky, new1, new2, clicked, plain],
        "failing and flaky first, never-checked next, clicked before the rest, hidden excluded"
    );

    let coverage = orbital::store::link_health_coverage(&pool)
        .await
        .expect("coverage");
    assert_eq!(
        coverage,
        LinkCoverage {
            total: 6,
            checked: 4,
            unchecked: 2,
            failing: 1,
            clicked: 1,
        }
    );
    let line = coverage.to_string();
    assert!(line.contains("6 total, 2 unchecked (33.33%), 1 failing, 1 clicked"), "{line}");
}
