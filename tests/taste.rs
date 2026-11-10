//! The instance's taste: declared sources, folded over real rows, reported to the operator alone.
//!
//! The point of these tests is not that folding works (the crate has 21 of those). It is that the
//! three things orbital must get right are right: an invalid declaration stops the process and names
//! the field, a source that admitted nobody is `absent` rather than a weight of zero, and the report
//! exists in `orbital doctor` while nothing public mentions it.

use orbital::taste::{self, TasteConfig};
use sqlx::PgPool;

/// A declaration the operator could plausibly type, one keystroke wrong.
const WEIGHT_TYPO: &str = r#"{
  "sources": [
    {"name": "criticalnexus", "selector": {"kind": "explicit", "ids": ["criticalnexus"]},
     "weight": 30.0, "kinds": ["like"]}
  ]
}"#;

#[sqlx::test]
async fn a_weight_of_thirty_is_refused_naming_the_field_and_the_source(_pool: PgPool) {
    let err = TasteConfig::from_json(WEIGHT_TYPO)
        .expect_err("a weight of 30 is a typo, not a ceiling");
    let msg = format!("{err:#}");
    assert!(msg.contains("weight"), "the field must be named: {msg}");
    assert!(msg.contains("criticalnexus"), "the source must be named: {msg}");
    assert!(
        msg.contains("0.0..=1.0"),
        "the message must say what the range is: {msg}"
    );
}

#[sqlx::test]
async fn an_unknown_kind_names_the_word_instead_of_being_dropped_quietly(_pool: PgPool) {
    let raw = r#"{
      "sources": [
        {"name": "criticalnexus", "selector": {"kind": "explicit", "ids": ["criticalnexus"]},
         "weight": 0.3, "kinds": ["favourite"]}
      ]
    }"#;
    let err = TasteConfig::from_json(raw).expect_err("`favourite` is not in the vocabulary");
    let msg = format!("{err:#}");
    assert!(msg.contains("favourite"), "the word must be named: {msg}");
    assert!(msg.contains("kinds"), "the field must be named: {msg}");
}

#[sqlx::test]
async fn the_built_in_declaration_is_the_one_d30_declares(_pool: PgPool) {
    let decl = taste::default_declaration();
    decl.validate().expect("the shipped declaration must be valid");

    let weight = |name: &str| {
        decl.sources
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("source `{name}` must be declared"))
            .weight
    };
    assert_eq!(weight("criticalnexus"), 0.3);
    assert_eq!(weight("steeltitan"), 0.7);
    assert!(
        !decl.sources.iter().any(|s| matches!(s.selector, polaris_taste_source::Selector::TrustLevel { .. })),
        "D30 has the trust selector off; declaring it would only add a line that admits nobody"
    );
    assert!(
        decl.sources.iter().any(|s| matches!(
            s.selector,
            polaris_taste_source::Selector::Affinity { min_evidence, .. } if min_evidence >= 20
        )),
        "an affinity source must carry an evidence floor, not a bare threshold"
    );
    // Declaration order is the crate's first-match-wins rule: a named account must never be read as
    // `everyone`, so `everyone` is last.
    assert_eq!(
        decl.sources.last().map(|s| s.name.as_str()),
        Some("everyone")
    );
}

#[sqlx::test]
async fn a_member_with_no_endorsements_leaves_every_source_absent_and_folds_no_zero(pool: PgPool) {
    sqlx::query(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ('criticalnexus', 'Critical Nexus', 'x')",
    )
    .execute(&pool)
    .await
    .expect("a real member row");

    let decl = taste::default_declaration();
    let folded = taste::load(&pool, &decl).await.expect("folding is not an error");

    assert!(
        folded.profile.is_empty(),
        "no endorsements exist, so the profile is empty, not a map of zeros: {:?}",
        folded.weights()
    );
    for entry in &folded.profile {
        assert!(entry.weight > 0.0, "a zero-weight entry is a lie: {entry:?}");
    }

    let lines = folded.report_lines();
    assert_eq!(
        lines.len(),
        decl.sources.len(),
        "one line per declared source, even the ones that admitted nobody"
    );
    let nexus = lines
        .iter()
        .find(|(what, _)| what.contains("criticalnexus"))
        .expect("the declared account gets a line");
    assert!(
        nexus.1.contains("absent"),
        "an admitted contributor with no evidence is absent, not active: {nexus:?}"
    );
    let titan = lines
        .iter()
        .find(|(what, _)| what.contains("steeltitan"))
        .expect("a declared account that has no row yet still gets a line");
    assert!(
        titan.1.contains("absent") && titan.1.contains("admitted 0"),
        "a source that admits nobody must say so: {titan:?}"
    );
}

#[sqlx::test]
async fn an_endorsement_by_a_declared_account_folds_in_under_its_own_name(pool: PgPool) {
    let id: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ('criticalnexus', 'Critical Nexus', 'x') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("a real member row");
    sqlx::query("INSERT INTO tag_endorsements (tag_slug, user_id) VALUES ('self-hosting', $1)")
        .bind(id.0)
        .execute(&pool)
        .await
        .expect("a real endorsement row");

    let folded = taste::load(&pool, &taste::default_declaration())
        .await
        .expect("folding is not an error");

    let entry = folded
        .weights()
        .get("self-hosting")
        .copied()
        .expect("the endorsed tag is in the profile");
    assert!(
        (entry - 0.3).abs() < 0.01,
        "a fresh endorsement by the 0.3 account folds to ~0.3, got {entry}"
    );
    let provenance = folded
        .profile
        .iter()
        .find(|e| e.subject == "self-hosting")
        .expect("the entry carries its own provenance");
    assert_eq!(provenance.sources, vec!["criticalnexus".to_string()]);
    assert!(
        folded
            .report_lines()
            .iter()
            .any(|(what, detail)| what.contains("criticalnexus") && detail.contains("active")),
        "a source that contributed is active: {:?}",
        folded.report_lines()
    );
}
