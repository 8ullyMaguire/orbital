//! The entity substrate (SPEC §5b, D52). A migration is a claim about what the database will refuse, so every
//! invariant here is tested by trying to break it and reading the error.
//!
//! The gate has three clauses: `cargo test` green, a duplicate `canonical_key` refused with the constraint error
//! visible, and normalisation driven from a table (`www.`, trailing slash, fragment, uppercase host).

use sqlx::PgPool;
use uuid::Uuid;

async fn insert_entity(pool: &PgPool, key: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO entities (slug, kind, canonical_key) VALUES ($1, 'site', $2) RETURNING id",
    )
    .bind(format!("e-{}", Uuid::new_v4()))
    .bind(key)
    .fetch_one(pool)
    .await
}

/// THE GATE, second clause: the key is unique, and the error is a unique violation, not a 500-shaped silence.
#[sqlx::test]
async fn the_same_canonical_key_cannot_be_stored_twice(pool: PgPool) {
    let first = insert_entity(&pool, "url:example.com/thing").await.expect("the first row");
    let second = insert_entity(&pool, "url:example.com/thing").await;
    let err = second.expect_err("the second row must be refused");
    assert!(
        err.to_string().contains("entities_canonical_key_key"),
        "the refusal names the unique constraint, got: {err}"
    );
    // And the first row is untouched: a refused write is not a partial write.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM entities WHERE id = $1")
        .bind(first)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1);
}

/// THE GATE, third clause, table-driven: the four rewrites the schema promises, one row per variant, all of which
/// must land on the same key — and a fifth that must not (a different path is a different page).
#[sqlx::test]
async fn normalisation_is_table_driven(pool: PgPool) {
    let cases: Vec<(&str, &str)> = vec![
        // raw input                                   what it must canonicalise to
        ("https://Example.COM/thing", "url:https://example.com/thing"),
        ("https://www.example.com/thing", "url:https://example.com/thing"),
        ("https://example.com/thing/", "url:https://example.com/thing"),
        ("https://example.com/thing#section-2", "url:https://example.com/thing"),
        ("  https://EXAMPLE.com/thing/#frag  ", "url:https://example.com/thing"),
    ];
    for (raw, want) in &cases {
        let key: String = sqlx::query_scalar("SELECT canonical_key($1)")
            .bind(raw)
            .fetch_one(&pool)
            .await
            .expect("the function answers");
        assert_eq!(&key, want, "{raw:?} canonicalised to {key:?}, wanted {want:?}");
    }

    // Every variant of the same page collides in the table, which is the point of the rewrite: one row, not five.
    let first = insert_entity(&pool, "https://example.com/thing").await.expect("first");
    for raw in ["https://www.example.com/thing", "https://EXAMPLE.com/thing/", "https://example.com/thing#x"] {
        let err = insert_entity(&pool, raw).await.expect_err("a variant of the same page is the same row");
        assert!(err.to_string().contains("entities_canonical_key_key"), "{raw}: {err}");
    }

    // A different path is a different thing, and a query string is part of the address (a search page is not the
    // page it searches for). Both must survive as distinct rows rather than being "normalised" into each other.
    insert_entity(&pool, "https://example.com/other").await.expect("a different path");
    insert_entity(&pool, "https://example.com/thing?page=2").await.expect("a query is not noise");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM entities")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 3, "one page, its neighbour, and a paginated view: {n}");

    // An identifier key is passed through and only lowercased: `isbn:` and `imdb:` are keys, not URLs.
    let key: String = sqlx::query_scalar("SELECT canonical_key('ISBN:978-0-306-40615-7')")
        .fetch_one(&pool)
        .await
        .expect("the function answers");
    assert_eq!(key, "isbn:978-0-306-40615-7", "the scheme word is lowercased, the value is not touched");
}

/// The invariants SPEC §5b says the migration enforces rather than the caller.
#[sqlx::test]
async fn the_schema_enforces_what_the_spec_promises(pool: PgPool) {
    // `merged_into` is set, never destructive: a merged entity still exists and points at its survivor.
    let a = insert_entity(&pool, "https://example.com/a").await.expect("a");
    let b = insert_entity(&pool, "https://example.com/b").await.expect("b");
    sqlx::query("UPDATE entities SET merged_into = $1 WHERE id = $2")
        .bind(a)
        .bind(b)
        .execute(&pool)
        .await
        .expect("a merge is an update, not a delete");
    // An entity cannot merge into itself — that is a cycle of one, and it has no survivor.
    let self_merge = sqlx::query("UPDATE entities SET merged_into = id WHERE id = $1")
        .bind(a)
        .execute(&pool)
        .await;
    assert!(self_merge.is_err(), "merging an entity into itself must be refused");

    // A review that arrives twice from the same remote activity is one review.
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ('reviewer', 'Reviewer', 'x') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("a user");
    for _ in 0..2 {
        let _ = sqlx::query(
            "INSERT INTO reviews (entity_id, user_id, rating, body, ap_id) VALUES ($1, $2, 3, 'ok', 'https://remote/1')",
        )
        .bind(b)
        .bind(user)
        .execute(&pool)
        .await;
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reviews")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 1, "redelivery of the same activity is idempotent: {n}");

    // Nothing cascades from users to entities: a person leaving leaves the catalogue intact.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .expect("delete the user");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM entities")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 2, "the entities survived their contributor: {n}");
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM reviews")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(reviews, 0, "erasing an account erases its words, not the catalogue");

    // A member can be a shell row: no password, because an actor can exist without a way to log in.
    sqlx::query("UPDATE users SET password_hash = NULL WHERE handle = 'reviewer'")
        .execute(&pool)
        .await
        .expect("a shell row is legal");
}
