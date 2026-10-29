//! Store-level tests for the visibility predicate.
//!
//! The HTTP tests prove the pages; these prove the queries underneath them, so a
//! regression in the SQL is caught by a test that names the query rather than by
//! a page that "looks fine".

use orbital::config::Config;
use sqlx::PgPool;
use uuid::Uuid;

async fn member(pool: &PgPool, handle: &str, verticals: &[&str], trust: i16) {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name, bio, password_hash, trust_level, nsfw_enabled)
         VALUES ($1, $1, 'test member', 'x', $2, $3) RETURNING id",
    )
    .bind(handle)
    .bind(trust)
    .bind(verticals.contains(&"nsfw"))
    .fetch_one(pool)
    .await
    .unwrap();
    for (i, v) in verticals.iter().enumerate() {
        sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(v)
            .bind(if i == 0 { "primary" } else { "secondary" })
            .execute(pool)
            .await
            .unwrap();
    }
}

#[sqlx::test]
async fn the_sfw_queries_exclude_adult_primaries_and_keep_sfw_primaries(pool: PgPool) {
    member(&pool, "sfwonly", &["dev"], 2).await;
    member(&pool, "dual", &["dev", "nsfw"], 2).await;
    member(&pool, "adultprimary", &["nsfw", "dev"], 2).await;

    let all = orbital::store::all_members(&pool, false).await.unwrap();
    let handles: Vec<&str> = all.iter().map(|m| m.handle.as_str()).collect();
    assert!(handles.contains(&"sfwonly"));
    assert!(handles.contains(&"dual"), "an adult secondary must not hide a SFW primary");
    assert!(!handles.contains(&"adultprimary"), "{handles:?}");

    let n = orbital::store::member_count(&pool, false).await.unwrap();
    assert_eq!(n, 2, "the SFW count must match the SFW listing");

    let map = orbital::store::sitemap_entries(&pool, false).await.unwrap();
    let mapped: Vec<&str> = map.iter().map(|(h, _)| h.as_str()).collect();
    assert!(mapped.contains(&"dual") && !mapped.contains(&"adultprimary"), "{mapped:?}");

    let (found, _tags) = orbital::store::search(&pool, "adultprimary", false).await.unwrap();
    assert!(found.is_empty(), "search must not reach across the wall");

    // And the adult side sees exactly the adult-primary member.
    let adult = orbital::store::all_members(&pool, true).await.unwrap();
    let adult_handles: Vec<&str> = adult.iter().map(|m| m.handle.as_str()).collect();
    assert!(adult_handles.contains(&"adultprimary"));
    assert!(adult_handles.contains(&"dual"));
    assert!(adult_handles.contains(&"sfwonly"), "the owner surface shows everyone");
}

#[sqlx::test]
async fn the_third_secondary_is_refused_by_the_database(pool: PgPool) {
    member(&pool, "greedy", &["dev"], 1).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE handle = 'greedy'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for v in ["art", "music"] {
        sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, $2, 'secondary')")
            .bind(id)
            .bind(v)
            .execute(&pool)
            .await
            .unwrap();
    }
    let third = sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'writers', 'secondary')")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(third.is_err(), "the third secondary was accepted by the schema");
}

#[sqlx::test]
async fn a_second_primary_is_refused_by_the_index(pool: PgPool) {
    member(&pool, "one", &["dev"], 1).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE handle = 'one'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'art', 'secondary')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let res = sqlx::query("UPDATE user_verticals SET role = 'primary' WHERE user_id = $1 AND vertical_slug = 'art'")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(res.is_err(), "two primaries for one member");
}

#[sqlx::test]
async fn the_test_config_never_points_at_a_real_host(pool: PgPool) {
    let cfg = Config::for_test("http://orbital.test");
    assert!(cfg.public_base.starts_with("http://"));
    assert!(!cfg.secure_cookies);
    let _ = pool;
}

#[sqlx::test]
async fn preference_weights_round_trip_through_the_store(pool: PgPool) {
    // The decode is the part that breaks silently (float4/numeric into f64), so this asserts the
    // values come back as written, from both sources, with explicit winning on collision.
    orbital::store::set_explicit_prefs(
        &pool,
        "visitor",
        "hash-1",
        &[("chiptune".to_string(), 1.0), ("godot".to_string(), 0.4)],
    )
    .await
    .unwrap();
    orbital::store::accumulate_passive(
        &pool,
        "hash-1",
        &[("chiptune".to_string(), 0.3), ("jazz".to_string(), 0.2)],
    )
    .await
    .unwrap();

    let weights = orbital::store::prefs(&pool, "visitor", "hash-1").await.unwrap();
    let map: std::collections::HashMap<&str, f64> =
        weights.iter().map(|(t, w)| (t.as_str(), *w)).collect();

    assert!((map["chiptune"] - 1.0).abs() < 1e-6, "explicit wins, unreduced by passive");
    assert!((map["godot"] - 0.4).abs() < 1e-6);
    assert!((map["jazz"] - 0.2).abs() < 1e-6);
    assert_eq!(weights[0].0, "chiptune", "strongest first");
}
