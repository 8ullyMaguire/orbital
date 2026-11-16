//! M6a's gate: the evaluator can see entities, the paired floor still refuses a thin sample, and a
//! golden set that shrinks is counted rather than quietly improving the number.

use orbital::eval::{self, Golden, MIN_CASES};
use sqlx::PgPool;
use uuid::Uuid;

async fn seed(pool: &PgPool) -> (Uuid, Uuid) {
    sqlx::query("INSERT INTO categories (slug, label) VALUES ('dev', 'Makers') ON CONFLICT DO NOTHING")
        .execute(pool)
        .await
        .expect("a category");
    let old_user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name) VALUES ('old', 'Old') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("an old subject");
    let new_user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name) VALUES ('new', 'New') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("a new subject");
    let old: Uuid = sqlx::query_scalar(
        "INSERT INTO entities (slug, kind, canonical_key, title, category_slug, created_at)
         VALUES ('old-entity', 'person', 'entity:old', 'Old Entity', 'dev',
                 now() - interval '300 days') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("an old entity");
    let new: Uuid = sqlx::query_scalar(
        "INSERT INTO entities (slug, kind, canonical_key, title, category_slug, created_at)
         VALUES ('new-entity', 'person', 'entity:new', 'New Entity', 'dev', now()) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("a new entity");
    for (entity, user, tag) in [(old, old_user, "rust"), (new, new_user, "gardening")] {
        sqlx::query("INSERT INTO entity_people (entity_id, user_id, role) VALUES ($1, $2, 'subject')")
            .bind(entity)
            .bind(user)
            .execute(pool)
            .await
            .expect("a subject link");
        sqlx::query("INSERT INTO entity_tags (entity_id, tag_slug) VALUES ($1, $2)")
            .bind(entity)
            .bind(tag)
            .execute(pool)
            .await
            .expect("an entity tag");
        let session = if tag == "rust" { "session-entity" } else { "other-session" };
        sqlx::query(
            "INSERT INTO source_hits (user_id, surface, vertical_context, session_hash, viewer_hash)
             VALUES ($1, 'home', 'dev', $2, $3)",
        )
        .bind(user)
        .bind(session)
        .bind("viewer")
        .execute(pool)
        .await
        .expect("a click");
    }
    sqlx::query(
        "INSERT INTO preference_weights (owner_kind, owner_id, tag_slug, weight, source)
         VALUES ('visitor', 'viewer', 'rust', 1.0, 'passive')",
    )
    .execute(pool)
    .await
    .expect("a lens weight");
    (old, new)
}

#[sqlx::test]
async fn entity_eval_sees_entities_and_refuses_a_thin_sample(pool: PgPool) {
    let (old, _) = seed(&pool).await;

    let golden = eval::emit_entity_seeds(&pool, 200)
        .await
        .expect("seeds from entity clicks");
    assert_eq!(golden.seeds.len(), 2);
    assert!(golden.seeds.contains(&"session-entity".to_string()));
    assert_eq!(golden.rule.min_cases, MIN_CASES);

    let (cases, missing) = eval::collect_entities(&pool, &golden.seeds)
        .await
        .expect("entity cases");
    assert_eq!(missing, 0);
    assert_eq!(cases.len(), 2);
    let case = cases
        .iter()
        .find(|c| c.session == "session-entity")
        .expect("the session we care about");
    assert_eq!(case.entities.len(), 2, "the pool is entities, not member rows");
    assert_eq!(case.clicked, vec![old]);

    // The old lens-favoured entity is buried by recency and lifted by the candidate arm.
    let (a, b) = eval::score_entities(std::slice::from_ref(case));
    assert!(b[&0] > a[&0], "candidate {b:?} must beat recency {a:?} on this case");

    let report = eval::run_entities(&pool, &golden)
        .await
        .expect("a below-floor run");
    assert_eq!(report.cases, 2);
    assert!(report.sentence().starts_with("NO VERDICT"), "{}", report.sentence());
    assert_eq!(report.exit_code(), 1, "NO VERDICT blocks the ranking change");

    // A stale seed must be reported, not dropped so the average looks better.
    let mut drifting = Golden {
        seeds: vec!["a-session-that-never-existed".into()],
        ..golden
    };
    drifting.seeds.push("session-entity".into());
    let report = eval::run_entities(&pool, &drifting).await.expect("a drifting run");
    assert_eq!(report.missing, 1);
    assert_eq!(report.cases, 1);
    assert!(report.sentence().starts_with("NO VERDICT"));
}
