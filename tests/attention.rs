//! Attention: what a click is worth, and the three gates the roadmap sets for it.
//!
//! The tests are written against the numbers the declaration promises, not against whatever the code
//! happens to produce: a lead of ten times becomes `(ln 11 / ln 2)^alpha` times, a second click from the
//! same session is the same person, and a click at slot nine counts more than one at slot one.

use chrono::Utc;
use orbital::attention::{self, Config, Mode};
use orbital::models::MemberStat;
use orbital::ranking;
use sqlx::PgPool;
use uuid::Uuid;

fn member(handle: &str, clicks: i64, attention: f64) -> MemberStat {
    let now = Utc::now();
    MemberStat {
        id: Uuid::new_v4(),
        handle: handle.to_string(),
        display_name: handle.to_string(),
        bio: String::new(),
        accent: "#7c5cff".into(),
        avatar_url: None,
        trust_level: 1,
        bsky_handle: None,
        nsfw_enabled: false,
        created_at: now,
        last_active_at: now,
        role: "member".into(),
        visitors: clicks,
        visitors_7d: clicks,
        clicks,
        tags: Vec::new(),
        verticals: Vec::new(),
        endorse: 0.0,
        attention,
    }
}

/// Deterministic pseudo-randomness, so a failure is reproducible from the seed printed in the message.
fn noise(seed: u64) -> f64 {
    let mut x = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x % 100_000) as f64 / 100_000.0
}

/// THE GATE, first half: `raw` must reproduce the ordering this code shipped before the declaration
/// existed — `clicks / max_clicks`, ties by handle — over a pool big enough that a coincidental match is
/// not plausible. The old formula is re-implemented here on purpose: comparing the new code to itself
/// would prove nothing.
#[test]
fn raw_reproduces_the_old_order_over_two_hundred_seeds() {
    for seed in 0..200u64 {
        let pool: Vec<MemberStat> = (0..12)
            .map(|i| {
                let clicks = (noise(seed * 100 + i) * 500.0) as i64;
                member(&format!("m{i}"), clicks, clicks as f64)
            })
            .collect();

        let max = pool.iter().map(|m| m.clicks).max().unwrap_or(0).max(1) as f64;
        let mut old: Vec<(f64, String)> = pool
            .iter()
            .map(|m| (m.clicks as f64 / max, m.handle.clone()))
            .collect();
        old.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let old_order: Vec<String> = old.into_iter().map(|(_, h)| h).collect();

        let new_order: Vec<String> = ranking::rank_top(pool, &[], 1.0, Config::raw())
            .into_iter()
            .map(|m| m.handle)
            .collect();
        assert_eq!(old_order, new_order, "seed {seed}: raw must not re-rank anything");
    }
}

/// THE GATE, second half: on a fixture where one member leads ten to one, `damped` must land on the ratio
/// the declaration states — and the declaration states it as a formula, not a constant.
#[test]
fn damped_shrinks_a_ten_fold_lead_to_the_declared_ratio() {
    let cfg = Config::default();
    assert_eq!(cfg.mode, Mode::Damped);
    // (10 of a max of 10) against (1 of a max of 10): the same additive offset on both sides, so the
    // ratio is exactly the formula the declaration publishes.
    let busiest = cfg.share(10.0, 10.0);
    let tenth = cfg.share(1.0, 10.0);
    let lead = busiest / tenth;
    let declared = cfg.lead_after(10.0);
    assert!(
        (lead - declared).abs() < 0.03,
        "a 10x lead must become {declared:.3}x, got {lead:.3}x"
    );
    assert!(lead < 3.0, "and the compression must be real, got {lead:.3}x");

    // A sharper alpha compresses less; alpha 1.0 is the log alone.
    let log_only = Config { mode: Mode::Damped, alpha: 1.0 };
    assert!((log_only.lead_after(10.0) - 11f64.ln() / 2f64.ln()).abs() < 1e-9);
}

/// A click seen at slot 9 is worth more than one at slot 1, and an unrecorded position is worth least.
#[test]
fn a_far_slot_click_is_worth_more_than_a_top_slot_click() {
    assert_eq!(attention::exposure(Some(1)), 1.0);
    assert_eq!(attention::exposure(None), 1.0);
    assert!((attention::exposure(Some(9)) - 3.197).abs() < 0.01);
}

/// THE GATE, third part, against real rows: a second click from the same session is the same person. The
/// raw count goes up (the page prints what happened), the attention the ranker reads does not.
#[sqlx::test]
async fn a_second_click_from_the_same_session_does_not_move_a_score(pool: PgPool) {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ('fan', 'Fan', 'x') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("a real member");

    sqlx::query("INSERT INTO source_hits (user_id, surface, session_hash, position) VALUES ($1, 'home', 's1', 1)")
        .bind(id)
        .execute(&pool)
        .await
        .expect("a click");
    let after_one = attention_of(&pool, "fan").await;
    assert_eq!(after_one.0, 1, "one click counted");
    assert!((after_one.1 - 1.0).abs() < 1e-9, "slot 1 is worth exactly one: {}", after_one.1);

    sqlx::query("INSERT INTO source_hits (user_id, surface, session_hash, position) VALUES ($1, 'home', 's1', 1)")
        .bind(id)
        .execute(&pool)
        .await
        .expect("the same person again");
    let after_two = attention_of(&pool, "fan").await;
    assert_eq!(after_two.0, 2, "the raw count is honest about what happened");
    assert!(
        (after_two.1 - after_one.1).abs() < 1e-9,
        "but one session is one person: {} vs {}",
        after_two.1,
        after_one.1
    );

    // A different session is a different person, and a click at slot 9 counts 1 + ln 9.
    sqlx::query("INSERT INTO source_hits (user_id, surface, session_hash, position) VALUES ($1, 'home', 's2', 9)")
        .bind(id)
        .execute(&pool)
        .await
        .expect("someone new, from far down the page");
    let after_third = attention_of(&pool, "fan").await;
    // Session one is still worth 1.0; session two is worth 1 + ln 9. The second click in session one
    // added nothing, which is the whole claim.
    assert!(
        (after_third.1 - (1.0 + (1.0 + 9f64.ln()))).abs() < 1e-6,
        "two audiences: {}",
        after_third.1
    );
}

/// `(raw clicks, attention)` for one handle, read through the same query the pages use.
async fn attention_of(pool: &PgPool, handle: &str) -> (i64, f64) {
    let members = orbital::store::all_members(pool, true).await.expect("members");
    let m = members
        .iter()
        .find(|m| m.handle == handle)
        .unwrap_or_else(|| panic!("{handle} is not in the pool"));
    (m.clicks, m.attention)
}
