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

#[sqlx::test]
async fn a_visitors_lens_survives_signing_up(pool: PgPool) {
    // The tuner is the front door for people without accounts; if the weights vanish at signup, the
    // funnel taught them that tuning was pointless.
    let visitor = orbital::prefs::visitor_hash("cookie-abc");
    orbital::store::set_explicit_prefs(
        &pool,
        "visitor",
        &visitor,
        &[("chiptune".to_string(), 0.9)],
    )
    .await
    .unwrap();
    let user = orbital::store::create_user(
        &pool,
        orbital::store::NewUser {
            handle: "tuned",
            display_name: "Tuned",
            bio: "",
            accent: "#000000",
            password_hash: "x",
            nsfw_enabled: false,
        },
    )
    .await
    .unwrap();

    orbital::store::adopt_visitor_prefs(&pool, &visitor, user.id)
        .await
        .unwrap();

    let mine = orbital::store::prefs(&pool, "user", &user.id.to_string())
        .await
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].0, "chiptune");
    assert!((mine[0].1 - 0.9).abs() < 1e-6);

    let theirs = orbital::store::prefs(&pool, "visitor", &visitor).await.unwrap();
    assert!(theirs.is_empty(), "the anonymous rows are gone, not duplicated");
}

#[sqlx::test]
async fn pruning_forgets_only_visitors_nobody_has_seen(pool: PgPool) {
    sqlx::query("INSERT INTO visitor_profiles (visitor_hash, pages_visited, last_seen_at) VALUES ('old', 3, now() - interval '400 days')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO visitor_profiles (visitor_hash, pages_visited) VALUES ('fresh', 3)")
        .execute(&pool)
        .await
        .unwrap();
    for who in ["old", "fresh"] {
        orbital::store::set_explicit_prefs(&pool, "visitor", who, &[("chiptune".to_string(), 0.5)])
            .await
            .unwrap();
    }

    let (visitors, weights) = orbital::store::prune_visitors(&pool, 180).await.unwrap();
    assert_eq!(visitors, 1);
    assert_eq!(weights, 1);

    let left: Vec<(String,)> = sqlx::query_as("SELECT visitor_hash FROM visitor_profiles")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec![("fresh".to_string(),)]);
    assert_eq!(
        orbital::store::prefs(&pool, "visitor", "fresh").await.unwrap().len(),
        1
    );
}
