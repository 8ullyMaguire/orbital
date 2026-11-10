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

/// A declared weight is applied against the share the ranker already uses, so declaring the number the
/// code already applies changes no ordering — and declaring a smaller one scales exactly.
#[test]
fn a_declared_weight_scales_the_factor_from_a_default_of_one() {
    let built_in = Config::default();
    assert!(
        (built_in.factor(Surface::Home, None) - 1.0).abs() < 1e-12,
        "the defaults are a multiplication by one, byte for byte"
    );

    let quarter = Config::from_json(r#"{"surfaces":[{"name":"vertical","strength":0.40,"weight":0.10}]}"#)
        .expect("a valid declaration");
    assert!((quarter.factor(Surface::Vertical, None) - 0.25).abs() < 1e-12);
    assert!(
        (quarter.factor_ordered(Surface::Vertical, None) - 1.0).abs() < 1e-12,
        "a relevance weight must not move the velocity order"
    );

    let silent = Config::from_json(r#"{"surfaces":[{"name":"vertical","strength":0.40,"weight":0.0}]}"#)
        .expect("zero is a legal weight, not a typo");
    assert_eq!(silent.factor(Surface::Vertical, None), 0.0, "weight 0 removes the lens term");
}

/// The declaration is per surface, and a surface that was not named keeps its own number.
#[test]
fn a_surface_reads_only_its_own_line_of_the_file() {
    let cfg = Config::from_json(r#"{"surfaces":[{"name":"vertical","strength":0.40,"weight":0.10}]}"#)
        .expect("a valid declaration");
    for other in [Surface::Home, Surface::Entity, Surface::Ring] {
        assert!(
            (cfg.factor(other, None) - 1.0).abs() < 1e-12,
            "{} must not read the vertical's line",
            other.name()
        );
    }
}

/// A weight is a share: 1.5 is a typo, and clamping it would hide the typo forever.
#[test]
fn a_weight_outside_zero_and_one_is_refused_by_name() {
    let err = Config::from_json(r#"{"surfaces":[{"name":"vertical","strength":0.40,"weight":1.5}]}"#)
        .expect_err("1.5 is not a share");
    let text = format!("{err:#}");
    assert!(text.contains("weight"), "the refusal names the field: {text}");
    assert!(text.contains("vertical"), "and the surface: {text}");
}
