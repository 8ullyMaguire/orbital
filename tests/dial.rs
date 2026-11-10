//! The reader's dial against a real row: stored, read back, and clamped the moment the operator
//! lowers the ceiling.

use orbital::dial::{Config, Surface};
use orbital::store;
use sqlx::PgPool;

#[sqlx::test]
async fn a_stored_dial_narrows_the_ceiling_and_is_clamped_after_the_operator_lowers_it(pool: PgPool) {
    let visitor = "0123456789abcdef0123456789abcdef";

    store::set_gravity(&pool, visitor, Some(0.40))
        .await
        .expect("the visitor profile row does not exist yet; the write must create it");
    assert_eq!(
        store::gravity_of(&pool, visitor).await.expect("read back"),
        Some(0.40),
        "what the reader chose is what the row holds"
    );

    let before = Config::default();
    assert_eq!(before.share(Surface::Home, Some(0.40)), 0.40);

    // The operator lowers the home ceiling. The same stored row now reads as the new number — the
    // gate's second request after the config change.
    let after = Config::from_json(r#"{"surfaces":[{"name":"home","strength":0.10}]}"#)
        .expect("a legal ceiling");
    assert_eq!(
        after.share(Surface::Home, Some(0.40)),
        0.10,
        "a value written under the old ceiling must not survive it"
    );

    // Clearing is different from zero: None means "the surface decides", 0.0 means "no lens".
    store::set_gravity(&pool, visitor, None).await.expect("clear");
    assert_eq!(store::gravity_of(&pool, visitor).await.expect("read back"), None);
    assert_eq!(
        after.share(Surface::Home, None),
        0.10,
        "no choice means the surface's own number"
    );
}
