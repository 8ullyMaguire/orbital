//! The floors gate: a tag a reader or the instance ranks *away* moves a member down by the declared amount,
//! never off the page, and a floor that matches nothing says so instead of looking like it worked.
//!
//! The arithmetic and both caps come from `polaris_candidates::floor` (multiplicative, `MAX_FLOOR` per rule,
//! `MAX_FLOOR_TOTAL` together). This module owns what the crate deliberately does not: whether a name is a tag or a
//! category, and what "matched nothing" reads like to a person.

use chrono::{Duration, Utc};
use orbital::floors::{Floors, MAX_FLOOR};
use orbital::models::MemberStat;

fn member(handle: &str, tags: &[&str], verticals: &[&str]) -> MemberStat {
    let now = Utc::now();
    MemberStat {
        id: uuid::Uuid::new_v4(),
        handle: handle.to_string(),
        display_name: handle.to_string(),
        bio: String::new(),
        accent: String::new(),
        avatar_url: None,
        trust_level: 1,
        bsky_handle: None,
        nsfw_enabled: false,
        created_at: now - Duration::days(30),
        last_active_at: now,
        role: "member".into(),
        visitors: 10,
        visitors_7d: 2,
        visitors_prev: 2,
        clicks: 10,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        verticals: verticals.iter().map(|v| v.to_string()).collect(),
        endorse: 0.0,
        attention: 10.0,
        recent: 10.0,
    }
}

/// THE GATE, first clause: multiplicative, so a floored member keeps its place among other floored members and is
/// still on the page (the caps stop 0.6 from becoming a filter).
#[test]
fn a_floor_takes_the_declared_share_and_never_zeroes() {
    let floors = Floors::parse_list("rust:0.25").expect("a valid list");
    let m = member("rusty", &["rust", "cli"], &["dev"]);
    let (score, applied) = floors.apply(&m, 1.0);
    assert!((score - 0.75).abs() < 1e-12, "a quarter taken: {score}");
    assert_eq!(applied.matched, vec!["rust".to_string()]);

    // Two rules: the total is capped, and the factor never reaches zero.
    let floors = Floors::parse_list("rust:0.5,cli:0.5").expect("a valid list");
    let (score, applied) = floors.apply(&m, 1.0);
    assert_eq!(applied.matched.len(), 2);
    assert!(score > 0.0, "a floor is never a filter: {score}");
    assert!((score - 0.4).abs() < 1e-12, "the total cap is 0.6, so 0.4 remains: {score}");

    // A member that carries nothing is untouched, score and report alike.
    let (untouched, applied) = floors.apply(&member("plain", &["garden"], &["dev"]), 0.8);
    assert!((untouched - 0.8).abs() < 1e-12);
    assert!(applied.is_empty());
}

/// A category floor matches a member's vertical, which is stored as `v:<slug>` and is what a reader clicking a
/// category means.
#[test]
fn a_category_floor_matches_the_vertical_and_not_the_tags() {
    let floors = Floors::parse_list("v:music").expect("a valid list");
    let musician = member("muso", &["chiptune"], &["music"]);
    let dev = member("dever", &["rust"], &["dev"]);
    assert!(floors.apply(&musician, 1.0).1.matched == vec!["v:music".to_string()]);
    assert!(floors.apply(&dev, 1.0).1.is_empty());
}

/// THE GATE, second clause: a floor that matched nothing is named. A typo is the common failure, and a floor that
/// silently did nothing is worse than no floor at all.
#[test]
fn a_floor_that_matches_nothing_is_named_in_the_report() {
    let floors = Floors::parse_list("rust,nonsense:0.2").expect("a valid list");
    let members = vec![
        member("rusty", &["rust"], &["dev"]),
        member("gardener", &["garden"], &["dev"]),
    ];
    let notes = floors.report(&members);
    let by_name = |n: &str| notes.iter().find(|note| note.name == n).cloned();
    let rust = by_name("rust").expect("rust is in the report");
    assert_eq!(rust.matched, 1);
    assert!(rust.sentence(&members).contains("1 profile"), "{}", rust.sentence(&members));
    let none = by_name("nonsense").expect("a floor that matched nothing is still in the report");
    assert_eq!(none.matched, 0);
    assert!(
        none.sentence(&members).contains("matches nothing"),
        "the unapplied floor says so: {}",
        none.sentence(&members)
    );
}

/// Declarations are refused by name rather than clamped: a weight above the crate's cap is a typo, and a typo that
/// silently becomes 0.5 is a ranking change nobody asked for.
#[test]
fn an_out_of_range_weight_is_refused_by_name() {
    let err = Floors::parse_list("rust:0.9").expect_err("0.9 is above the cap").to_string();
    assert!(err.contains("rust"), "the refusal names the rule: {err}");
    assert!(err.contains(&format!("{MAX_FLOOR}")), "and the cap: {err}");
    // Parsing also refuses a name that is not a tag or a category, so a typo cannot become a silent no-op.
    assert!(Floors::parse_list("").is_err(), "an empty list is not a declaration");
    assert!(Floors::parse_list("rust:0.2.1").is_err());
    // Valid input keeps the declared order and the declared weights.
    let floors = Floors::parse_list("rust:0.4,v:music:0.1").expect("a valid list");
    assert_eq!(floors.rules().len(), 2);
    assert!((floors.rules()[0].weight - 0.4).abs() < 1e-12);
    assert!((floors.rules()[1].weight - 0.1).abs() < 1e-12);
}

/// The default weight, when a reader writes a bare name: low enough to be a nudge, and named in the docs so it is
/// not a number that appeared from nowhere.
#[test]
fn a_bare_name_gets_the_declared_default_weight() {
    let floors = Floors::parse_list("rust").expect("a valid list");
    assert!((floors.rules()[0].weight - orbital::floors::READER_FLOOR_WEIGHT).abs() < 1e-12);
    assert!(orbital::floors::READER_FLOOR_WEIGHT <= MAX_FLOOR);
}
