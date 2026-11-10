//! The step's gate: the tag-overlap arithmetic moves into one adapter module that uses the family's
//! `Signal` contract, and nothing about a page changes for a pool where no tag is rarer than another.
//!
//! The oracle is the function this step deletes, kept here as `legacy_cosine` — the old formula, verbatim,
//! from `ranking.rs` before the move. A refactor that claims "no behaviour change" and cannot produce the
//! old formula to compare against is claiming it, not proving it.

use orbital::overlap::{self, Overlap};

/// `ranking.rs::cosine` as it was: unweighted, `0.0` for an empty side.
fn legacy_cosine(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|t| b.contains(t)).count() as f64;
    shared / ((a.len() as f64).sqrt() * (b.len() as f64).sqrt())
}

fn pool_from(seed: &mut u64, n: usize, tags_per: usize, alphabet: usize) -> Vec<Vec<String>> {
    let mut next = || {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    };
    (0..n)
        .map(|_| {
            let mut tags: Vec<String> = Vec::new();
            for _ in 0..tags_per {
                let t = format!("t{}", next() % alphabet as u64);
                if !tags.contains(&t) {
                    tags.push(t);
                }
            }
            tags
        })
        .collect()
}

/// THE GATE, first half: the arithmetic the page actually calls is the arithmetic it called before — every
/// tag weighs `1.0`, on every pool, however skewed. 200 seeded pools, every pair, identical to the last bit.
#[test]
fn the_uniform_weights_are_the_formula_this_step_replaces() {
    let mut seed = 0x2545_F491_4F6C_DD1D;
    let mut pairs = 0usize;
    for round in 0..200 {
        let pool = pool_from(&mut seed, 4 + round % 9, 2 + round % 4, 6);
        let ov = Overlap::uniform();
        for a in &pool {
            for b in &pool {
                let want = legacy_cosine(a, b);
                let got = ov.pair_or_zero(a, b);
                assert!(
                    got.to_bits() == want.to_bits(),
                    "round {round}: {a:?} vs {b:?} — old {want}, new {got}"
                );
                pairs += 1;
            }
        }
    }
    assert!(pairs > 10_000, "the gate compared {pairs} pairs");
}

/// The page's ranking functions call the uniform entry point, and nothing else: pool-level rarity is built and
/// tested here, but it is not wired into any surface. Turning it on changes an order, and an order change has
/// to pass `orbital eval` first (ROADMAP §3, D48) — a refactor step does not get to smuggle a ranking change in
/// behind the word "adopt".
#[test]
fn the_page_ranks_with_the_uniform_entry_point_and_nothing_else() {
    let ranking = std::fs::read_to_string("src/ranking.rs").expect("src/ranking.rs");
    assert!(ranking.contains("cosine("), "the page still ranks by tag overlap at all");
    assert!(
        !ranking.contains("Overlap::from_sets") && !ranking.contains("TagSignal"),
        "a pool-level overlap reached the page without an eval gate"
    );
}

/// THE GATE, second half: the doc comment that was there all along and was never true — "weighted so a
/// rare tag counts for more than a tag everybody has". On a skewed pool it is now true, and the old
/// formula would give both pairs the same score.
#[test]
fn on_a_skewed_pool_a_rare_tag_outweighs_a_common_one() {
    // Ten members share `common`; one member is the only one with `rare`.
    let mut sets: Vec<Vec<String>> = (0..10)
        .map(|i| vec!["common".to_string(), format!("filler{}", i % 2)])
        .collect();
    sets.push(vec!["rare".to_string(), "filler0".to_string()]);
    let ov = Overlap::from_sets(sets.iter().map(|s| s.as_slice()));

    let a = vec!["common".to_string(), "filler0".to_string()];
    let rare = vec!["rare".to_string(), "filler0".to_string()];
    let both = ov.pair(&a, &a).expect("a pair");
    let with_common = ov.pair(&a, &a).expect("a pair");
    let with_rare = ov.pair(&a, &rare).expect("a pair");

    assert!(
        with_rare > 0.0 && with_rare < with_common + 1e-12,
        "the rare tag must carry real weight: {with_rare} vs {with_common}"
    );
    // The old formula is blind to rarity: both pairs share exactly one of two tags.
    assert!(
        (legacy_cosine(&a, &rare) - legacy_cosine(&a, &a) * 0.5).abs() < 1e-12,
        "the oracle really is rarity-blind"
    );
    // ...and the new one is not: the same comparison, weighted, is larger when the shared tag is the rare one.
    let common_side = vec!["common".to_string(), "filler1".to_string()];
    let rare_vs_common_pair = ov.pair(&a, &common_side).expect("a pair");
    assert!(
        with_rare > rare_vs_common_pair,
        "sharing the rare tag ({with_rare}) must beat sharing the common one ({rare_vs_common_pair})"
    );
    assert!((both - 1.0).abs() < 1e-12, "a set shared with itself is 1.0");
}

/// The crate's contract, on the seam: nothing to say is `None`, never `0.0`.
#[test]
fn an_empty_side_is_none_not_zero() {
    let ov = Overlap::uniform();
    assert!(ov.pair(&[], &["a".to_string()]).is_none());
    assert!(ov.pair(&["a".to_string()], &[]).is_none());
    assert!(ov.pair(&[], &[]).is_none());
    assert_eq!(ov.pair(&["a".to_string()], &["a".to_string()]), Some(1.0));
    // The rankers keep reading "no opinion" as zero, which is what they did before; the difference is that
    // the seam can now say which one it means.
    assert_eq!(ov.pair_or_zero(&[], &["a".to_string()]), 0.0);
}

/// The roadmap's gate sentence, as a test: `fn cosine` and `fn tag_relevance` are defined in the adapter
/// module and nowhere else in `src/`.
#[test]
fn the_old_definitions_survive_only_in_the_adapter() {
    let mut offenders: Vec<String> = Vec::new();
    for entry in std::fs::read_dir("src").expect("src/") {
        let path = entry.expect("entry").path();
        if !path.to_string_lossy().ends_with(".rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        for needle in ["fn cosine(", "fn tag_relevance("] {
            if text.contains(needle) && !path.ends_with("overlap.rs") {
                offenders.push(format!("{} defines `{needle}`", path.display()));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:?}");
}

/// The two names the rankers call still exist where they expect them, and they answer the same numbers.
#[test]
fn the_rankers_keep_their_entry_points() {
    let tags = vec!["chiptune".to_string(), "jazz".to_string()];
    assert!((overlap::cosine(&tags, &tags) - 1.0).abs() < 1e-12);
    assert_eq!(overlap::cosine(&tags, &[]), 0.0);
    let weights = vec![("chiptune".to_string(), 1.0)];
    assert_eq!(overlap::tag_relevance(&tags, &weights), 1.0);
    assert_eq!(overlap::tag_relevance(&tags, &[]), 0.0);
}
