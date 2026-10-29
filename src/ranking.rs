//! Ranking, similarity and ring ordering. Pure functions over rows the store
//! hands back, so the maths is testable without a database.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::MemberStat;

pub fn trust_mult(level: i16) -> f64 {
    match level {
        0 => 0.5,
        1 => 0.8,
        2 => 1.0,
        _ => 1.1,
    }
}

pub fn age_days(created: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let secs = (now - created).num_seconds().max(0) as f64;
    secs / 86_400.0
}

/// Click velocity, normalised against the busiest member in the vertical,
/// with a logarithmic new-account boost and a trust damper.
///
/// A member with no visitors in the window scores 0 and is dropped by the
/// caller: a "rising" list padded with the unengaged is a recency list.
pub fn trending_score(visitors_7d: i64, max_7d: i64, age_days: f64, trust: i16) -> f64 {
    if visitors_7d <= 0 || max_7d <= 0 {
        return 0.0;
    }
    let velocity = visitors_7d as f64 / max_7d as f64;
    let boost = 1.0 + 0.15 / ((age_days + 2.0).log2());
    velocity * boost * trust_mult(trust)
}

/// Cosine similarity of two tag sets, weighted so a rare tag counts for more
/// than a tag everybody has. Falls back to 0 for an empty side.
pub fn cosine(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|t| b.contains(t)).count() as f64;
    shared / ((a.len() as f64).sqrt() * (b.len() as f64).sqrt())
}

/// How much of what the viewer asked for this member actually is.
///
/// `Σ(w[t] · has_tag[t]) / Σ w[t]` — a weighted share, so 1.0 means "every tag I care about is on
/// this profile" regardless of how many weights there are. Zero for a viewer with no weights:
/// not-applicable and not-relevant must not look the same, but both contribute nothing.
pub fn tag_relevance(tags: &[String], weights: &[(String, f64)]) -> f64 {
    let total: f64 = weights.iter().map(|(_, w)| w.max(0.0)).sum();
    if total <= 0.0 || tags.is_empty() {
        return 0.0;
    }
    let hit: f64 = weights
        .iter()
        .filter(|(t, w)| *w > 0.0 && tags.iter().any(|tag| tag == t))
        .map(|(_, w)| *w)
        .sum();
    (hit / total).clamp(0.0, 1.0)
}

/// The same weighted share, but a member's verticals count too.
///
/// A passive signal from browsing a category is stored as `v:<slug>`, and a member's tags never
/// contain that string — so matching only against tags made every vertical visit influence nothing.
/// A weight on `v:music` now means "show me people in music", which is what the visitor meant.
pub fn relevance(m: &MemberStat, weights: &[(String, f64)]) -> f64 {
    let total: f64 = weights.iter().map(|(_, w)| w.max(0.0)).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let hit: f64 = weights
        .iter()
        .filter(|(k, w)| {
            if *w <= 0.0 {
                return false;
            }
            if let Some(slug) = k.strip_prefix("v:") {
                m.verticals.iter().any(|v| v == slug)
            } else {
                m.tags.iter().any(|t| t == k)
            }
        })
        .map(|(_, w)| *w)
        .sum();
    (hit / total).clamp(0.0, 1.0)
}

pub fn recency(last_active: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let days = (now - last_active).num_seconds().max(0) as f64 / 86_400.0;
    0.5f64.powf(days / 30.0)
}

pub fn vertical_match(candidate_primary: Option<&str>, context: Option<&str>) -> f64 {
    match (candidate_primary, context) {
        (Some(c), Some(ctx)) if c == ctx => 1.0,
        (_, None) => 0.0,
        (None, _) => 0.0,
        _ => 0.0,
    }
}

pub fn vertical_match_full(candidate_verticals: &[String], context: Option<&str>) -> f64 {
    let Some(ctx) = context else { return 0.0 };
    match candidate_verticals.first().map(|s| s.as_str()) {
        Some(primary) if primary == ctx => 1.0,
        _ => {
            if candidate_verticals.iter().any(|v| v == ctx) {
                0.5
            } else {
                0.0
            }
        }
    }
}

/// The sidebar formula from v3: the viewer's stated taste dominates, the page owner gives context,
/// and the hub owner's pull is a light hand (0.15, down from 0.35).
///
/// The viewer term is their **weights** when they have any, and their own tags when they do not —
/// a signed-in member who never opened the tuner still gets a sidebar about them rather than a
/// sidebar about whoever happens to be popular. The six terms sum to 1 in both cases, so a score
/// means the same thing whether or not the viewer has tuned anything.
pub fn similarity(
    viewer_tags: &[String],
    owner_tags: &[String],
    hub_tags: &[String],
    viewer_weights: &[(String, f64)],
    candidate: &MemberStat,
    context: Option<&str>,
    now: DateTime<Utc>,
) -> f64 {
    let relevance = if viewer_weights.is_empty() {
        cosine(&candidate.tags, viewer_tags)
    } else {
        relevance(candidate, viewer_weights)
    };
    0.40 * relevance
        + 0.25 * cosine(&candidate.tags, owner_tags)
        + 0.15 * cosine(&candidate.tags, hub_tags)
        + 0.10 * vertical_match_full(&candidate.verticals, context)
        + 0.05 * recency(candidate.last_active_at, now)
        + 0.05 * (candidate.trust_level as f64 / 3.0)
}

/// Top: 60% all-time attention, 40% "is this what the viewer asked for".
///
/// With no viewer weights the second term is absent and the ordering is exactly the click ranking
/// it has always been — the regression this function is tested against.
pub fn rank_top(members: Vec<MemberStat>, weights: &[(String, f64)]) -> Vec<MemberStat> {
    if weights.is_empty() {
        let mut members = members;
        members.sort_by(|a, b| {
            b.visitors
                .cmp(&a.visitors)
                .then_with(|| b.clicks.cmp(&a.clicks))
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
        return members;
    }
    let max_clicks = members.iter().map(|m| m.clicks).max().unwrap_or(0).max(1) as f64;
    let mut scored: Vec<(f64, MemberStat)> = members
        .into_iter()
        .map(|m| {
            let clicks = m.clicks as f64 / max_clicks;
            let rel = relevance(&m, weights);
            // Endorsements are a separate term, not a bigger `rel`: relevance saturates at 1.0 for a
            // member who holds every tag in the lens, so adding inside it did nothing for exactly
            // the members it was meant to reward. 0.4 x 0.2 is the same +0.08 the relevance term
            // would have given, and it cannot swamp clicks: 0.6 is six times its ceiling.
            let endorse = crate::governance::ENDORSE_BOOST * m.endorse.clamp(0.0, 1.0);
            (0.6 * clicks + 0.4 * rel + 0.4 * endorse, m)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.handle.cmp(&b.1.handle))
    });
    scored.into_iter().map(|(_, m)| m).collect()
}

pub fn rank_trending(
    members: Vec<MemberStat>,
    now: DateTime<Utc>,
    weights: &[(String, f64)],
) -> Vec<MemberStat> {
    let max_7d = members.iter().map(|m| m.visitors_7d).max().unwrap_or(0);
    let mut scored: Vec<(f64, MemberStat)> = members
        .into_iter()
        .map(|m| {
            let s = if weights.is_empty() {
                trending_score(
                    m.visitors_7d,
                    max_7d,
                    age_days(m.created_at, now),
                    m.trust_level,
                )
            } else {
                let velocity = if max_7d > 0 {
                    0.5 * (m.visitors_7d as f64 / max_7d as f64)
                } else {
                    0.0
                };
                let boost = 0.1 * (1.0 + 0.15 / ((age_days(m.created_at, now) + 2.0).log2()));
                0.3 * relevance(&m, weights) + velocity + boost + 0.1 * trust_mult(m.trust_level)
            };
            (s, m)
        })
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, m)| m).collect()
}

/// New: recency first, but a fresh profile matching the viewer rises above one that does not.
pub fn rank_new(
    members: Vec<MemberStat>,
    now: DateTime<Utc>,
    weights: &[(String, f64)],
) -> Vec<MemberStat> {
    if weights.is_empty() {
        let mut members = members;
        members.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        return members;
    }
    let mut scored: Vec<(f64, MemberStat)> = members
        .into_iter()
        .map(|m| {
            let rec = 0.5f64.powf(age_days(m.created_at, now) / 14.0);
            (0.7 * rec + 0.3 * relevance(&m, weights), m)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.created_at.cmp(&a.1.created_at))
    });
    scored.into_iter().map(|(_, m)| m).collect()
}

/// Deterministic 0..1 from a string. Used for the ring's daily noise, so the
/// order is stable within a day and reshuffles on the next one.
pub fn hash01(s: &str) -> f64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    (h.finish() % 10_000) as f64 / 10_000.0
}

/// Ring order: similarity to the hub owner plus noise. Nothing is permanently
/// first, and nothing is permanently last.
pub fn ring<'a>(
    members: &'a [MemberStat],
    owner_tags: &[String],
    owner_verticals: &[String],
    day: i64,
) -> Vec<&'a MemberStat> {
    let mut scored: Vec<(f64, &MemberStat)> = members
        .iter()
        .map(|m| {
            let affinity = 0.7 * cosine(&m.tags, owner_tags)
                + 0.3 * vertical_match_full(&m.verticals, owner_verticals.first().map(|s| s.as_str()));
            let noise = hash01(&format!("{}:{}", m.id, day)) * 0.35;
            (affinity + noise, m)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.handle.cmp(&b.1.handle))
    });
    scored.into_iter().map(|(_, m)| m).collect()
}

/// The preference-aware ring step: where `next` goes for *this* viewer.
///
/// 0.6 what the viewer asked for, 0.3 continuity from where they are now, 0.1 novelty (an
/// unvisited member). With no weights it returns `None` and the caller falls back to ring order.
pub fn ring_next<'a>(
    members: &'a [MemberStat],
    current: Uuid,
    weights: &[(String, f64)],
    visited: &std::collections::HashSet<Uuid>,
) -> Option<&'a MemberStat> {
    if weights.is_empty() {
        return None;
    }
    let anchor = members.iter().find(|m| m.id == current)?;
    let best = members
        .iter()
        .filter(|m| m.id != current)
        .map(|m| {
            let novelty = if visited.contains(&m.id) { 0.0 } else { 1.0 };
            let score = 0.6 * relevance(m, weights)
                + 0.3 * cosine(&m.tags, &anchor.tags)
                + 0.1 * novelty;
            (score, m)
        })
        .max_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Deterministic tie-break: a ring that reshuffles on equal scores feels random.
                .then_with(|| b.1.handle.cmp(&a.1.handle))
        });
    best.map(|(_, m)| m)
}

pub fn neighbours<'a>(
    ring: &[&'a MemberStat],
    id: Uuid,
) -> (Option<&'a MemberStat>, Option<&'a MemberStat>) {
    let Some(pos) = ring.iter().position(|m| m.id == id) else {
        return (None, None);
    };
    if ring.len() < 2 {
        return (None, None);
    }
    let prev = ring[(pos + ring.len() - 1) % ring.len()];
    let next = ring[(pos + 1) % ring.len()];
    (Some(prev), Some(next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn member(handle: &str, visitors_7d: i64, age: i64, trust: i16, tags: &[&str]) -> MemberStat {
        let now = Utc::now();
        MemberStat {
            id: Uuid::new_v4(),
            handle: handle.to_string(),
            display_name: handle.to_string(),
            bio: String::new(),
            accent: "#000000".into(),
            avatar_url: None,
            trust_level: trust,
            bsky_handle: None,
            nsfw_enabled: false,
            created_at: now - Duration::days(age),
            last_active_at: now - Duration::days(1),
            role: "primary".into(),
            visitors: visitors_7d,
            visitors_7d,
            clicks: visitors_7d,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            verticals: vec!["dev".into()],
            endorse: 0.0,
        }
    }

    #[test]
    fn a_person_with_no_visitors_does_not_rise() {
        let now = Utc::now();
        let out = rank_trending(
            vec![member("quiet", 0, 2, 2, &[]), member("loud", 9, 400, 2, &[])],
            now,
            &[],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].handle, "loud");
    }

    #[test]
    fn trending_is_empty_when_nobody_has_traffic() {
        let now = Utc::now();
        let out = rank_trending(vec![member("a", 0, 1, 0, &[])], now, &[]);
        assert!(out.is_empty(), "a zero-traffic vertical must hide the tab");
    }

    #[test]
    fn newer_accounts_get_a_bigger_boost_than_old_ones() {
        let new = trending_score(5, 10, 1.0, 2);
        let old = trending_score(5, 10, 400.0, 2);
        assert!(new > old, "{new} should beat {old}");
        // It stays modest: the boost is 1.0..1.15 of the velocity term, so it
        // can reorder near-ties but never rewrite the leaderboard.
        let ratio = new / old;
        assert!(ratio > 1.0 && ratio < 1.10, "boost ratio {ratio}");
    }

    #[test]
    fn trust_dampens_but_does_not_erase() {
        assert!(trending_score(5, 10, 100.0, 0) < trending_score(5, 10, 100.0, 3));
        assert!(trending_score(5, 10, 100.0, 0) > 0.0);
    }

    #[test]
    fn cosine_is_set_overlap() {
        let a = vec!["rust".to_string(), "cli".to_string()];
        let b = vec!["rust".to_string(), "cli".to_string()];
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-9);
        let c = vec!["rust".to_string()];
        assert!((cosine(&a, &c) - 0.7071).abs() < 1e-3);
        assert_eq!(cosine(&a, &[]), 0.0);
    }

    #[test]
    fn vertical_match_rewards_primary_then_secondary() {
        assert_eq!(vertical_match_full(&["dev".into()], Some("dev")), 1.0);
        assert_eq!(
            vertical_match_full(&["dev".into(), "art".into()], Some("art")),
            0.5
        );
        assert_eq!(vertical_match_full(&["dev".into()], Some("art")), 0.0);
        assert_eq!(vertical_match_full(&["dev".into()], None), 0.0);
    }

    #[test]
    fn the_ring_is_stable_within_a_day_and_moves_between_days() {
        let members: Vec<MemberStat> = (0..6)
            .map(|i| member(&format!("m{i}"), 0, 10, 1, &["x"]))
            .collect();
        let tags = vec!["x".to_string()];
        let day1 = ring(&members, &tags, &["dev".to_string()], 20_000);
        let day1b = ring(&members, &tags, &["dev".to_string()], 20_000);
        let day2 = ring(&members, &tags, &["dev".to_string()], 20_001);
        let a: Vec<_> = day1.iter().map(|m| m.handle.clone()).collect();
        let b: Vec<_> = day1b.iter().map(|m| m.handle.clone()).collect();
        let c: Vec<_> = day2.iter().map(|m| m.handle.clone()).collect();
        assert_eq!(a, b);
        assert_ne!(a, c, "the noise must actually move day to day");
        assert_eq!(a.len(), 6);
    }

    #[test]
    fn neighbours_wrap_around() {
        let members: Vec<MemberStat> = (0..3).map(|i| member(&format!("m{i}"), 0, 1, 0, &[])).collect();
        let order = ring(&members, &[], &[], 1);
        let (prev, next) = neighbours(&order, order[0].id);
        assert_eq!(prev.unwrap().id, order[2].id);
        assert_eq!(next.unwrap().id, order[1].id);
        let (prev, next) = neighbours(&order, Uuid::new_v4());
        assert!(prev.is_none() && next.is_none());
    }

    #[test]
    fn recency_halves_each_month() {
        let now = Utc.with_ymd_and_hms(2026, 1, 31, 0, 0, 0).unwrap();
        let thirty = now - Duration::days(30);
        assert!((recency(thirty, now) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn relevance_is_a_weighted_share_and_zero_without_weights() {
        let tags = vec!["chiptune".to_string(), "godot".to_string()];
        assert_eq!(tag_relevance(&tags, &[]), 0.0, "no weights, no opinion");
        assert_eq!(
            tag_relevance(&tags, &[("chiptune".into(), 1.0)]),
            1.0,
            "one asked-for tag, present"
        );
        assert_eq!(
            tag_relevance(&tags, &[("jazz".into(), 1.0)]),
            0.0,
            "asked for something they do not have"
        );
        // Half the weight is on a tag the member has.
        let half = tag_relevance(&tags, &[("chiptune".into(), 0.5), ("jazz".into(), 0.5)]);
        assert!((half - 0.5).abs() < 1e-9, "got {half}");
    }

    #[test]
    fn the_tuner_lifts_a_matching_member_above_a_busier_one() {
        // Clicks are 60% of the score, so the lens wins only where the attention gap is close —
        // which is the honest claim. A member with a tenth of the clicks is still tenth.
        let mut a = member("matching", 1, 40, 2, &["chiptune"]);
        a.clicks = 70;
        let mut b = member("busier", 1, 40, 2, &["podcast"]);
        b.clicks = 100;
        let weights = vec![("chiptune".to_string(), 1.0)];

        let plain = rank_top(vec![a.clone(), b.clone()], &[]);
        assert_eq!(plain[0].handle, "busier", "clicks decide without a lens");

        let tuned = rank_top(vec![a.clone(), b.clone()], &weights);
        assert_eq!(tuned[0].handle, "matching", "a full weight on a held tag flips a narrow lead");

        // ...and it cannot flip a wide one.
        let mut far = member("quiet", 1, 40, 2, &["chiptune"]);
        far.clicks = 1;
        let mut loud = member("loud", 1, 40, 2, &["podcast"]);
        loud.clicks = 100;
        let tuned = rank_top(vec![far, loud], &weights);
        assert_eq!(tuned[0].handle, "loud", "relevance is 40%, not a veto");
    }

    #[test]
    fn no_weights_reproduces_the_old_ordering_exactly() {
        // visitors, then clicks, then the older account: `c` is older than `a`, both have 5 clicks.
        let mut a = member("a", 10, 1, 2, &["x"]);
        a.clicks = 5;
        let mut b = member("b", 10, 5, 2, &["x"]);
        b.clicks = 90;
        let mut c = member("c", 10, 9, 2, &["x"]);
        c.clicks = 5;
        let order = rank_top(vec![a, b, c], &[]);
        assert_eq!(
            order.iter().map(|m| m.handle.as_str()).collect::<Vec<_>>(),
            vec!["b", "c", "a"],
            "clicks first, then the older account, as before"
        );
    }

    #[test]
    fn an_endorsed_tag_is_worth_a_little_more() {
        // Same clicks, so the relevance term decides; the handles are ordered so that a tie would
        // go to `aaa`. Three endorsements (endorse = 1.0) must be enough to flip it, and the cap
        // means they cannot flip much more than that.
        let mut plain = member("aaa", 1, 40, 2, &["chiptune"]);
        plain.clicks = 70;
        let mut liked = member("zzz", 1, 40, 2, &["chiptune"]);
        liked.clicks = 70;
        liked.endorse = 1.0;
        let weights = vec![("chiptune".to_string(), 1.0)];
        // Both hold the tag, so relevance is saturated for both: the endorsement is the only
        // difference, which is exactly the case the additive term exists for.

        let order = rank_top(vec![plain.clone(), liked.clone()], &weights);
        assert_eq!(order[0].handle, "zzz", "an endorsed tag lifts its member");

        // Without the endorsement the same pair falls back to the tie-break.
        let mut liked_plain = liked.clone();
        liked_plain.endorse = 0.0;
        let order = rank_top(vec![plain, liked_plain], &weights);
        assert_eq!(order[0].handle, "aaa", "no endorsement, no lift");
    }

    #[test]
    fn a_vertical_weight_matches_members_of_that_vertical() {
        // A passive signal from browsing /v/music is stored as `v:music`, and a member's tags never
        // contain that string - so matching only tags made every vertical visit influence nothing.
        let mut dev = member("dever", 0, 5, 2, &["rust"]);
        dev.verticals = vec!["dev".into()];
        let mut muso = member("muso", 0, 5, 2, &["chiptune"]);
        muso.verticals = vec!["music".into()];
        let weights = vec![("v:music".to_string(), 1.0)];

        assert_eq!(relevance(&dev, &weights), 0.0);
        assert_eq!(relevance(&muso, &weights), 1.0);
        let order = rank_top(vec![dev, muso], &weights);
        assert_eq!(order[0].handle, "muso");
    }

    #[test]
    fn the_ring_step_prefers_what_the_viewer_asked_for() {
        let start = member("start", 0, 40, 2, &["chiptune"]);
        let podcast = member("podcast-person", 0, 40, 2, &["podcast"]);
        let chiptune = member("chiptune-person", 0, 40, 2, &["chiptune"]);
        let all = vec![start.clone(), podcast.clone(), chiptune.clone()];

        // No weights: no opinion, and the caller keeps its ring order.
        assert!(ring_next(&all, start.id, &[], &Default::default()).is_none());

        let next = ring_next(
            &all,
            start.id,
            &[("chiptune".to_string(), 1.0)],
            &Default::default(),
        )
        .expect("a personalised step");
        assert_eq!(next.handle, "chiptune-person");
    }
}
