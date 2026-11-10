//! The neighbourhood gate: "members like this one" is tag overlap, absence is a reason and not a fabricated
//! list, and the answer does not move between two runs of the same data.
//!
//! This is the surface D48 named as the place pool-level IDF becomes live: a *new* list has no previous order to
//! regress, so the rarity weighting can be on from the first row instead of waiting behind `orbital eval`.

use chrono::{Duration, Utc};
use orbital::models::MemberStat;
use orbital::neighbours::{similar, Reason};

fn member(handle: &str, tags: &[&str]) -> MemberStat {
    let now = Utc::now();
    MemberStat {
        id: uuid::Uuid::new_v4(),
        handle: handle.to_string(),
        display_name: handle.to_string(),
        bio: String::new(),
        accent: "#000000".into(),
        avatar_url: None,
        trust_level: 1,
        bsky_handle: None,
        nsfw_enabled: false,
        created_at: now - Duration::days(30),
        last_active_at: now,
        role: "member".into(),
        visitors: 0,
        visitors_7d: 0,
        visitors_prev: 0,
        clicks: 0,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        verticals: vec!["dev".to_string()],
        endorse: 0.0,
        attention: 0.0,
        recent: 0.0,
    }
}

/// THE GATE, first clause: a member with no tags answers nothing, and says why.
#[test]
fn a_member_with_no_tags_gets_a_reason_and_no_list() {
    let pool = vec![member("bare", &[]), member("tagged", &["rust", "cli"])];
    let hood = similar(&pool, &pool[0], 5);
    assert!(hood.members.is_empty(), "nothing to compare, so nothing is listed: {:?}", hood.members);
    assert_eq!(hood.reason, Reason::NoTags);
    assert!(
        hood.reason.sentence().contains("tag"),
        "the reason is a sentence a reader can act on: {}",
        hood.reason.sentence()
    );

    // A member whose tags nobody else shares is a different answer from "has no tags".
    let pool = vec![member("lonely", &["obscure"]), member("other", &["rust"])];
    let hood = similar(&pool, &pool[0], 5);
    assert!(hood.members.is_empty());
    assert_eq!(hood.reason, Reason::NothingShared);
    assert_ne!(Reason::NothingShared.sentence(), Reason::NoTags.sentence());

    // And the only member in the directory is neither of those.
    let pool = vec![member("only", &["rust"])];
    let hood = similar(&pool, &pool[0], 5);
    assert!(hood.members.is_empty());
    assert_eq!(hood.reason, Reason::Alone);
}

/// THE GATE, second clause: twenty samples, computed twice from freshly built pools, are identical — same order,
/// same members, same shared tags. Determinism is the whole promise of a list nobody can tune.
#[test]
fn twenty_samples_are_stable_across_a_rebuild() {
    let handles: Vec<String> = (0..20).map(|i| format!("m{i:02}")).collect();
    let tag_sets: Vec<Vec<&str>> = vec![
        vec!["rust", "cli"],
        vec!["rust", "axum"],
        vec!["rust", "cli", "tui"],
        vec!["go", "cli"],
        vec!["rust"],
        vec!["axum", "sqlx"],
        vec!["rust", "sqlx"],
        vec!["tui", "cli"],
        vec!["go"],
        vec!["rust", "go"],
        vec!["axum"],
        vec!["cli", "tui", "go"],
        vec!["rust", "cli", "axum"],
        vec!["sqlx"],
        vec!["rust", "tui"],
        vec!["go", "sqlx"],
        vec!["axum", "tui"],
        vec!["rust", "cli", "sqlx"],
        vec!["cli"],
        vec!["rust", "axum", "cli", "tui"],
    ];
    let build = || -> Vec<MemberStat> {
        handles
            .iter()
            .zip(tag_sets.iter())
            .map(|(h, t)| member(h, t))
            .collect()
    };
    let first_pool = build();
    let first: Vec<String> = first_pool
        .iter()
        .map(|m| {
            let hood = similar(&first_pool, m, 5);
            format!(
                "{}->{:?}",
                m.handle,
                hood.members
                    .iter()
                    .map(|n| format!("{}:{:?}", n.handle, n.shared_tags))
                    .collect::<Vec<_>>()
            )
        })
        .collect();

    // A rebuild: same data, different structs, different iteration order into the pool.
    let mut second_pool = build();
    second_pool.reverse();
    let second: Vec<String> = first_pool
        .iter()
        .map(|m| {
            let target = second_pool
                .iter()
                .find(|x| x.handle == m.handle)
                .expect("same handles");
            let hood = similar(&second_pool, target, 5);
            format!(
                "{}->{:?}",
                m.handle,
                hood
                    .members
                    .iter()
                    .map(|n| format!("{}:{:?}", n.handle, n.shared_tags))
                    .collect::<Vec<_>>()
            )
        })
        .collect();

    assert_eq!(first.len(), 20);
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a, b, "the neighbourhood moved between two builds of the same data");
    }
}

/// IDF is live on this surface: sharing a tag only two members carry beats sharing one everybody has.
#[test]
fn a_rare_shared_tag_outranks_a_common_one() {
    let mut pool: Vec<MemberStat> = (0..8)
        .map(|i| {
            if i == 0 {
                member("me", &["common", "rare"])
            } else {
                member(&format!("c{i}"), &["common", "filler"])
            }
        })
        .collect();
    pool.push(member("rare_peer", &["common", "rare"]));
    let hood = similar(&pool, &pool[0], 5);
    assert_eq!(
        hood.members.first().map(|n| n.handle.as_str()),
        Some("rare_peer"),
        "the member who shares the rare tag comes first: {:?}",
        hood.members.iter().map(|n| (&n.handle, n.overlap)).collect::<Vec<_>>()
    );
    assert!(hood.members[0].shared_tags.contains(&"rare".to_string()));
    // The common tag is shared with everybody, and everybody else is ranked below.
    assert!(hood.members.len() > 1);
    assert!(hood.members[1].overlap < hood.members[0].overlap);
}

/// The target never lists itself, and the count is capped.
#[test]
fn a_member_is_not_its_own_neighbour() {
    let pool: Vec<MemberStat> = (0..12)
        .map(|i| member(&format!("n{i:02}"), &["rust", "cli"]))
        .collect();
    let hood = similar(&pool, &pool[0], 5);
    assert_eq!(hood.members.len(), 5, "capped at the caller's number");
    assert!(hood.members.iter().all(|n| n.handle != "n00"));
    assert_eq!(hood.reason, Reason::Shared);
}
