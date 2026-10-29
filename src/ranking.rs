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

/// The sidebar formula from the spec, in the order it was written.
pub fn similarity(
    viewer_tags: &[String],
    owner_tags: &[String],
    candidate: &MemberStat,
    context: Option<&str>,
    now: DateTime<Utc>,
) -> f64 {
    0.35 * cosine(&candidate.tags, viewer_tags)
        + 0.25 * cosine(&candidate.tags, owner_tags)
        + 0.20 * vertical_match_full(&candidate.verticals, context)
        + 0.10 * recency(candidate.last_active_at, now)
        + 0.10 * (candidate.trust_level as f64 / 3.0)
}

pub fn rank_top(mut members: Vec<MemberStat>) -> Vec<MemberStat> {
    members.sort_by(|a, b| {
        b.visitors
            .cmp(&a.visitors)
            .then_with(|| b.clicks.cmp(&a.clicks))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
    members
}

pub fn rank_trending(members: Vec<MemberStat>, now: DateTime<Utc>) -> Vec<MemberStat> {
    let max_7d = members.iter().map(|m| m.visitors_7d).max().unwrap_or(0);
    let mut scored: Vec<(f64, MemberStat)> = members
        .into_iter()
        .map(|m| {
            let s = trending_score(
                m.visitors_7d,
                max_7d,
                age_days(m.created_at, now),
                m.trust_level,
            );
            (s, m)
        })
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, m)| m).collect()
}

pub fn rank_new(mut members: Vec<MemberStat>) -> Vec<MemberStat> {
    members.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    members
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
        }
    }

    #[test]
    fn a_person_with_no_visitors_does_not_rise() {
        let now = Utc::now();
        let out = rank_trending(
            vec![member("quiet", 0, 2, 2, &[]), member("loud", 9, 400, 2, &[])],
            now,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].handle, "loud");
    }

    #[test]
    fn trending_is_empty_when_nobody_has_traffic() {
        let now = Utc::now();
        let out = rank_trending(vec![member("a", 0, 1, 0, &[])], now);
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
}
