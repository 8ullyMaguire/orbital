//! The offline evaluation against real rows: a session clicked a member's card, and the question is
//! whether the ranking puts that member higher than a recency list would.
//!
//! These tests are about the two failures that would make the instrument useless: a metric that cannot
//! see a difference the ordering really has, and a decision rule that reports a number below its own
//! sample floor.

use chrono::Utc;
use orbital::eval::{self, Case, Golden, MIN_CASES};
use orbital::models::MemberStat;
use sqlx::PgPool;
use uuid::Uuid;

fn member(handle: &str, age_days: i64, clicks: i64, tags: &[&str]) -> MemberStat {
    let now = Utc::now();
    MemberStat {
        id: Uuid::new_v4(),
        handle: handle.to_string(),
        display_name: handle.to_string(),
        bio: String::new(),
        accent: "#7c5cff".into(),
        avatar_url: None,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        verticals: Vec::new(),
        visitors: clicks,
        visitors_7d: clicks,
        clicks,
        endorse: 0.0,
        attention: clicks as f64,
        recent: clicks as f64,
        created_at: now - chrono::Duration::days(age_days),
        last_active_at: now,
        trust_level: 1,
        nsfw_enabled: false,
        bsky_handle: None,
        role: "member".into(),
    }
}

/// A recency list and a clicks-and-lens list disagree on this case, and the metric has to see it: the
/// clicked member is the OLD one that the session's lens favours, so recency buries it at the bottom
/// and the candidate lifts it. If this test cannot fail, the eval measures nothing.
#[test]
fn the_metric_sees_a_difference_the_ordering_really_has() {
    let clicked = member("clicked", 300, 1, &["rust"]);
    let newer = member("newer", 1, 1, &["gardening"]);
    let case = Case {
        session: "s1".into(),
        members: vec![newer, clicked.clone()],
        weights: vec![("rust".to_string(), 1.0)],
        clicked: vec![clicked.id],
    };

    let (arm_a, arm_b) = eval::score(std::slice::from_ref(&case), Utc::now());
    assert_eq!(arm_a.len(), 1);
    assert_eq!(arm_b.len(), 1);
    let a = arm_a[&0];
    let b = arm_b[&0];
    assert!(
        b > a,
        "the candidate must place the clicked member above where recency puts it: recency {a} vs candidate {b}"
    );
    assert!(a < 0.7, "recency should score badly here, got {a}");
}

/// The floor is the whole point of a decision rule: a run with few cases must not produce a number a
/// person could quote as a result.
#[sqlx::test]
async fn a_run_below_the_floor_prints_no_verdict_and_blocks_the_change(pool: PgPool) {
    // One real member and one real click row: a real case, and exactly one of them.
    let id: (Uuid,) = sqlx::query_as(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ('clicked', 'Clicked', 'x') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("a real member row");
    sqlx::query(
        "INSERT INTO source_hits (user_id, surface, session_hash) VALUES ($1, 'home', 'session-one')",
    )
    .bind(id.0)
    .execute(&pool)
    .await
    .expect("a real click row");

    let golden = eval::emit_seeds(&pool, 500).await.expect("seeds come from rows");
    assert_eq!(golden.seeds, vec!["session-one".to_string()]);

    let report = eval::run(&pool, &golden).await.expect("the run itself succeeds");
    assert_eq!(report.cases, 1);
    assert!(
        report.sentence().starts_with("NO VERDICT"),
        "one case is not evidence: {}",
        report.sentence()
    );
    assert_eq!(report.exit_code(), 1, "a NO VERDICT must block the change");
    assert!(
        report.paired.why.contains(&MIN_CASES.to_string()),
        "the refusal must name the floor it did not reach: {}",
        report.paired.why
    );
}

#[sqlx::test]
async fn a_golden_set_with_no_seeds_is_refused_rather_than_run_empty(pool: PgPool) {
    let _ = &pool;
    let empty = Golden { seeds: vec![], ..Golden::empty() };
    let err = Golden::from_json(&empty.to_json())
        .expect_err("an empty golden set is a broken deployment, not a clean result");
    assert!(
        format!("{err:#}").contains("no seeds"),
        "the refusal says how to fix it: {err:#}"
    );
}

/// A seed that no longer matches a row must be counted, not quietly dropped: a golden set that shrinks
/// makes the number improve while nothing does.
#[sqlx::test]
async fn a_seed_with_no_row_is_counted_as_missing(pool: PgPool) {
    let mut golden = eval::emit_seeds(&pool, 10).await.expect("an empty database is legal");
    golden.seeds.push("a-session-that-never-existed".into());

    let report = eval::run(&pool, &golden).await.expect("run");
    assert_eq!(report.cases, 0);
    assert_eq!(report.missing, 1);
    assert!(report.sentence().starts_with("NO VERDICT"));
}
