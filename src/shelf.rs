//! The home page's shelves — composed, budgeted, diversified, and capped by the estate's arithmetic.
//!
//! Before this module the home page called `ranking::rank_trending`, `rank_new` and `curated_any` and
//! truncated each with its own `.take(6)`: three lists, three ad-hoc caps, no record of which source
//! proposed a card, and no way to answer "why is this here" except by reading the ranker. That is
//! exactly the part `polaris-candidates` already does for kindred and darkhold — named sources, a
//! per-source budget, provenance, MMR with the normalisation that stops a score's *scale* from becoming
//! a ranking decision, and a capped prior blend.
//!
//! What this module owns is the part the crate must not know: which sources orbital has, what a shelf
//! is called, and which observable facts a card may cite in its `why` line.
//!
//! ## The rules encoded here
//!
//! * **A shelf is a composition, not a sort.** `For you` unions the viewer's tag relevance with raw
//!   clicks; `Rising` and `New` each have one source today, which is the degenerate case of the same
//!   call — when a second source earns its place, the budget is already declared.
//! * **Diversity is a declared number, not a vibe.** `For you` runs MMR at `for_you_lambda`; the other
//!   two run at 1.0, which is *pure relevance* — that is what makes their order byte-identical to the
//!   rankers they replaced, and it is the property the step's gate measures.
//! * **A `why` line cites only observable facts** — the viewer's own tags, clicks, visitors, age. It
//!   may never cite the instance's prior (DECISIONS D27): the whole point of a silent prior is that a
//!   card's explanation is the same for a cold visitor and a tuned one, minus the viewer's own signal.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use polaris_candidates::{compose, select, Budget, Source};

use crate::models::MemberStat;
use crate::ranking;

/// The cut that makes a member "undiscovered" (D46): below this percentile of the pool's exposure. A
/// constant rather than a declaration because it belongs to one surface; the day a second surface wants a
/// different number is the day it moves into a config file.
pub const UNDISCOVERED_PERCENTILE: f64 = 0.60;

/// The smallest audience a member needs before a conversion rate means anything (D50). Five visitors is not
/// statistics; it is the point below which the rate is 0, 1/2, 1/3 — a coin toss dressed as a ratio.
pub const GEM_MIN_VISITORS: i64 = 5;

/// How many cards one shelf shows.
pub const PER_SHELF: usize = 6;

/// How many candidates a shelf may consider before selection.
pub const CANDIDATE_CAP: usize = 24;

/// The numbers a shelf's behaviour is made of. A declaration, not a constant, because roadmap step 5
/// moves these into the config file and step 3 gives each surface its own ceiling.
#[derive(Debug, Clone, Copy)]
pub struct Declaration {
    /// MMR lambda for `For you`. Below 1.0 the shelf spends some relevance on diversity, which is what
    /// surfaces a card the click ranking would never show.
    pub for_you_lambda: f64,
    /// MMR lambda for `Rising` and `New`. 1.0 = pure relevance = the ranker's own order.
    pub ordered_lambda: f64,
    pub per_shelf: usize,
}

impl Default for Declaration {
    fn default() -> Self {
        Self {
            // 0.85 is a *lean* toward relevance with room for novelty; the crate caps a prior at 0.30
            // of a score, and diversity here is a selection criterion rather than a prior, so it is
            // held to the same spirit: the last slots may differ, the top of the shelf may not.
            for_you_lambda: 0.85,
            ordered_lambda: 1.0,
            per_shelf: PER_SHELF,
        }
    }
}

/// One selected candidate, with the sentence a reader is allowed to see.
#[derive(Debug, Clone)]
pub struct Ranked {
    /// Index into the `members` slice the shelf was built from.
    pub index: usize,
    pub why: String,
}

#[derive(Debug, Clone)]
pub struct Shelf {
    /// `for_you`, `rising`, `new` — a stable key so the surface can style and test each one.
    pub key: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub items: Vec<Ranked>,
}

/// Build the three home shelves over one member pool.
///
/// `weights` is the viewer's own lens (`tag -> weight`, possibly with `v:slug` vertical keys); it is
/// the only input that makes two visitors see different shelves, and it is the viewer's data, not the
/// instance's.
pub fn build(
    members: &[MemberStat],
    weights: &[(String, f64)],
    now: DateTime<Utc>,
    decl: &Declaration,
    lens: crate::dial::Lens,
    attention: crate::attention::Config,
    caps: &crate::exposure::Config,
    ceiling: &crate::ceiling::Config,
) -> Vec<Shelf> {
    // `lens` is the reader's dial as a fraction of this surface's declared ceiling: 1.0 is the
    // ordering the site has always served, 0.0 is no lens at all.
    if members.is_empty() {
        return Vec::new();
    }

    let position = positions(members);
    // Exposure is the same 90-day weighted attention `Top` ranks by (D42), so the cohort rule needs no
    // second instrument: a member the page shows a lot of is a member who has been read a lot.
    let exposure: Vec<f64> = members.iter().map(|m| m.recent).collect();
    // Members already placed on this page view: the cross-shelf rule (D43) lives here, in one set that
    // every shelf adds to, because a rule each shelf enforced alone would be no rule at all.
    let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let tag_ids = tag_ids(members);

    // The three source lists, each produced by a ranker that already exists. Reusing them is what
    // makes the ordered shelves byte-identical to the lists they replaced: the arithmetic that turns
    // clicks, visitors and age into an order is not re-implemented here.
    let by_relevance = ids(&position, &ranking::rank_top(members.to_vec(), weights, lens.relevance, attention, ranking::Window::Recent));
    let by_clicks = {
        let mut idx: Vec<usize> = (0..members.len()).collect();
        idx.sort_by(|a, b| {
            members[*b]
                .clicks
                .cmp(&members[*a].clicks)
                .then(members[*a].handle.cmp(&members[*b].handle))
        });
        idx
    };
    let by_rising = ids(&position, &ranking::rank_trending(members.to_vec(), now, weights, lens.ordered));
    let by_new = ids(&position, &ranking::rank_new(members.to_vec(), now, weights, lens.ordered));

    let mut shelves = Vec::new();

    // `For you`: the viewer's tags and raw clicks, unioned with a budget, then diversified.
    let budget = Budget::new()
        .declare(Source::Tags, CANDIDATE_CAP / 2)
        .expect("a non-zero budget")
        .declare(Source::Related, CANDIDATE_CAP / 2)
        .expect("a non-zero budget");
    let lists = [
        (Source::Tags, as_rows(&by_relevance)),
        (Source::Related, as_rows(&by_clicks)),
    ];
    if let Ok(pool) = compose(-1, &lists, &budget, CANDIDATE_CAP) {
        // `compose` speaks in `Id` (i64); the shelf speaks in pool indices. The cast is the boundary
        // between the crate's vocabulary and this repo's, and it stays in one place.
        let candidates: Vec<usize> = pool.ids().iter().map(|id| *id as usize).collect();
        let items = to_items(&candidates, &tag_ids);
        // The full MMR ordering, then the caps: `enforce` needs somewhere to look for a tail slot, and a
        // selection that stopped at `per_shelf` would have thrown away the candidates it needs.
        let order = select(items.clone(), items.len(), decl.for_you_lambda, |a, b| {
            polaris_candidates::jaccard(a, b)
        });
        let chosen = crate::exposure::enforce(&order, decl.per_shelf, &mut used, &exposure, caps);
        // The last slots rotate daily (D45): the top of the shelf is the ranker's judgement and stays put,
        // the tail is refilled from the same candidate order by a hash of (date, member).
        let chosen = crate::ceiling::rotate(
            &chosen,
            &order,
            &member_ids(members),
            crate::ceiling::day_key(crate::ceiling::day_of(now)),
            ceiling,
        );
        shelves.push(Shelf {
            key: "for_you",
            title: "For you",
            subtitle: "Your tags and what people actually click. The last slots make room for something new.",
            items: why_lines(members, weights, now, chosen),
        });
    }

    // `Rising`: one source, pure relevance — the Trending ranker's own order.
    shelves.push(Shelf {
        key: "rising",
        title: "Rising this week",
        subtitle: "Distinct visitors in the last seven days, damped for brand-new accounts.",
        items: {
            let items = to_items(&by_rising, &tag_ids);
            let order = select(items.clone(), items.len(), decl.ordered_lambda, |a, b| {
                polaris_candidates::jaccard(a, b)
            });
            let chosen = crate::exposure::enforce(&order, decl.per_shelf, &mut used, &exposure, caps);
        // The last slots rotate daily (D45): the top of the shelf is the ranker's judgement and stays put,
        // the tail is refilled from the same candidate order by a hash of (date, member).
        let chosen = crate::ceiling::rotate(
            &chosen,
            &order,
            &member_ids(members),
            crate::ceiling::day_key(crate::ceiling::day_of(now)),
            ceiling,
        );
            why_lines(members, weights, now, chosen)
        },
    });

    // `Undiscovered` (D46): only the members below the sixtieth percentile of exposure — the part of the
    // catalogue a page is least likely to show — ranked by momentum, which is this week against their own
    // trailing three-week average. A shelf of the already-visible would be a fourth copy of the first three;
    // a shelf of the invisible, ordered by growth, is the only one that can surprise a regular reader.
    //
    // Empty is a legitimate answer: when every member sits at or above the cut (an all-flat pool, or one
    // member on its own) the shelf is not pushed at all, and the template renders nothing rather than a
    // padded list. The percentile is a constant here on purpose: it is one surface's threshold, and a second
    // surface wanting a different number is the moment it becomes a declaration.
    let cut = crate::exposure::percentile(&exposure, UNDISCOVERED_PERCENTILE);
    let mut tail: Vec<usize> = (0..members.len()).filter(|i| exposure[*i] < cut).collect();
    tail.sort_by(|a, b| {
        ranking::momentum(&members[*b])
            .partial_cmp(&ranking::momentum(&members[*a]))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                exposure[*a]
                    .partial_cmp(&exposure[*b])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| members[*a].handle.cmp(&members[*b].handle))
    });
    if !tail.is_empty() {
        let chosen = crate::exposure::enforce(&tail, decl.per_shelf, &mut used, &exposure, caps);
        let chosen = crate::ceiling::rotate(
            &chosen,
            &tail,
            &member_ids(members),
            crate::ceiling::day_key(crate::ceiling::day_of(now)),
            ceiling,
        );
        if !chosen.is_empty() {
            let items = chosen
                .into_iter()
                .map(|index| {
                    let m = &members[index];
                    let base = (m.visitors_prev as f64 / 3.0).round() as i64;
                    let why = if m.visitors_7d == 0 {
                        "quiet this week".to_string()
                    } else if base == 0 {
                        format!("{} visitors this week, none the three before", m.visitors_7d)
                    } else if m.visitors_7d > base {
                        format!("{} visitors this week, up from {}", m.visitors_7d, base)
                    } else {
                        format!("{} visitors this week, level with {}", m.visitors_7d, base)
                    };
                    Ranked { index, why }
                })
                .collect();
            shelves.push(Shelf {
                key: "undiscovered",
                title: "Undiscovered",
                subtitle: "Below the sixtieth percentile of exposure, ranked by how fast they are growing.",
                items,
            });
        }
    }

    // `Hidden gems` (D50): ranked *against* clicks — the members the click count has overlooked, ordered by how
    // well they convert the visitors they do get. A gem is not "small" (that is `undiscovered`) and not "old" (that
    // is the monument): it is a profile people click through when they land on it, that the leaderboards rarely
    // surface. Every number is already in the stats fragment; nothing here is a judgement.
    //
    // Two rules keep it from being a list of one-page wonders: a minimum sample (`visitors >= 5`), and eligibility
    // only for members *below* the median clicks — a page that already ranks high by clicks is not overlooked.
    let eligible: Vec<usize> = (0..members.len())
        .filter(|i| members[*i].visitors >= GEM_MIN_VISITORS)
        .collect();
    if !eligible.is_empty() {
        let mut click_counts: Vec<i64> = eligible.iter().map(|i| members[*i].clicks).collect();
        click_counts.sort_unstable();
        let median_clicks = click_counts[click_counts.len() / 2];
        let mut gems: Vec<usize> = eligible
            .into_iter()
            .filter(|i| members[*i].clicks <= median_clicks)
            .collect();
        gems.sort_by(|a, b| {
            let ca = members[*a].clicks as f64 / members[*a].visitors.max(1) as f64;
            let cb = members[*b].clicks as f64 / members[*b].visitors.max(1) as f64;
            cb.partial_cmp(&ca)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| members[*a].handle.cmp(&members[*b].handle))
        });
        if !gems.is_empty() {
            let chosen = crate::exposure::enforce(&gems, decl.per_shelf, &mut used, &exposure, caps);
            // The novelty slot fills what the cohort rule would leave empty (ROADMAP: "filled by the novelty
            // slot"): the same daily rotation the other shelves use, drawn from the gems order, so an eligible
            // member is not permanently off the page because the caps filled up first.
            let mut chosen = chosen;
            if chosen.len() < decl.per_shelf {
                let rotated = crate::ceiling::rotate(
                    &gems,
                    &gems,
                    &member_ids(members),
                    crate::ceiling::day_key(crate::ceiling::day_of(now)),
                    ceiling,
                );
                for i in rotated {
                    if chosen.len() >= decl.per_shelf {
                        break;
                    }
                    if !chosen.contains(&i) && !used.contains(&i) {
                        used.insert(i);
                        chosen.push(i);
                    }
                }
            }
            if !chosen.is_empty() {
                let items = chosen
                    .into_iter()
                    .map(|index| {
                        let m = &members[index];
                        let rate = m.clicks as f64 / m.visitors.max(1) as f64;
                        let why = format!(
                            "{} click{} from {} visitor{}",
                            m.clicks,
                            if m.clicks == 1 { "" } else { "s" },
                            m.visitors,
                            if m.visitors == 1 { "" } else { "s" }
                        );
                        let _ = rate;
                        Ranked { index, why }
                    })
                    .collect();
                shelves.push(Shelf {
                    key: "gems",
                    title: "Hidden gems",
                    subtitle: "Read to the end when people find them, and rarely on the leaderboards.",
                    items,
                });
            }
        }
    }

    // `New`: one source, pure relevance — the New ranker's own order. Last, because recency is the least    // `New`: one source, pure relevance — the New ranker's own order. Last, because recency is the least
    // informative of the four and the cross-shelf rule (D43) means the last shelf gets what is left.    // `New`: one source, pure relevance — the New ranker's own order. Last, because recency is the least
    // informative of the four and the cross-shelf rule (D43) means the last shelf gets what is left.
    shelves.push(Shelf {
        key: "new",
        title: "New to the directory",
        subtitle: "Everyone who joined recently, newest first. No threshold to clear.",
        items: {
            let items = to_items(&by_new, &tag_ids);
            let order = select(items.clone(), items.len(), decl.ordered_lambda, |a, b| {
                polaris_candidates::jaccard(a, b)
            });
            let chosen = crate::exposure::enforce(&order, decl.per_shelf, &mut used, &exposure, caps);
        // The last slots rotate daily (D45): the top of the shelf is the ranker's judgement and stays put,
        // the tail is refilled from the same candidate order by a hash of (date, member).
        let chosen = crate::ceiling::rotate(
            &chosen,
            &order,
            &member_ids(members),
            crate::ceiling::day_key(crate::ceiling::day_of(now)),
            ceiling,
        );
            why_lines(members, weights, now, chosen)
        },
    });

    shelves
}

/// The pool's ids, in pool order: the rotation hashes members, not slots, so the same member keeps the same
/// daily draw whichever shelf it lands on.
fn member_ids(members: &[MemberStat]) -> Vec<uuid::Uuid> {
    members.iter().map(|m| m.id).collect()
}

/// A member's position in the pool, so a ranker's output can be turned back into indices.
fn positions(members: &[MemberStat]) -> HashMap<uuid::Uuid, usize> {
    members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id, i))
        .collect()
}

fn ids(position: &HashMap<uuid::Uuid, usize>, ranked: &[MemberStat]) -> Vec<usize> {
    ranked
        .iter()
        .filter_map(|m| position.get(&m.id).copied())
        .collect()
}

fn as_rows(order: &[usize]) -> Vec<(i64, u32)> {
    order.iter().map(|i| (*i as i64, 0u32)).collect()
}

/// Stable per-run tag ids for the similarity closure. The crate takes `i64` tag ids and does not know
/// their names — that is the boundary the crate documents, and it is why the mapping lives here.
fn tag_ids(members: &[MemberStat]) -> HashMap<usize, Vec<i64>> {
    let mut names: HashMap<&str, i64> = HashMap::new();
    let mut next = 0i64;
    let mut out = HashMap::new();
    for (i, m) in members.iter().enumerate() {
        let mut ids = Vec::with_capacity(m.tags.len());
        for t in &m.tags {
            let id = *names.entry(t.as_str()).or_insert_with(|| {
                next += 1;
                next
            });
            ids.push(id);
        }
        out.insert(i, ids);
    }
    out
}

fn to_items(order: &[usize], tag_ids: &HashMap<usize, Vec<i64>>) -> Vec<(usize, f64, Vec<i64>)> {
    let n = order.len().max(1) as f64;
    order
        .iter()
        .enumerate()
        // A strictly decreasing score in the ranker's own order: the crate normalises by the top score,
        // so only the ratios matter, and this keeps them identical for a ranking that was already sorted.
        .map(|(pos, idx)| (*idx, 1.0 - (pos as f64 / n), tag_ids.get(idx).cloned().unwrap_or_default()))
        .collect()
}

/// The observable facts a card may cite. Never the prior: D27 forbids a surface from naming it, and a
/// `why` that differed between two visitors beyond their own tags would be a leak with a sentence on it.
fn why_lines(
    members: &[MemberStat],
    weights: &[(String, f64)],
    now: DateTime<Utc>,
    chosen: Vec<usize>,
) -> Vec<Ranked> {
    chosen
        .into_iter()
        .map(|index| {
            let m = &members[index];
            let matched = weights
                .iter()
                .filter(|(tag, w)| *w > 0.0 && !tag.starts_with("v:") && m.tags.iter().any(|t| t == tag))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(tag, _)| tag.clone());
            let why = if let Some(tag) = matched {
                format!("matches your #{tag}")
            } else if m.clicks > 0 {
                format!("{} clicks", m.clicks)
            } else {
                let age = ranking::age_days(m.created_at, now);
                if age < 7.0 {
                    "joined this week".to_string()
                } else if m.visitors_7d > 0 {
                    format!("{} visitors this week", m.visitors_7d)
                } else {
                    "listed".to_string()
                }
            };
            Ranked { index, why }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn member(handle: &str, clicks: i64, visitors_7d: i64, age_days: i64, tags: &[&str]) -> MemberStat {
        MemberStat {
            id: uuid::Uuid::new_v4(),
            handle: handle.to_string(),
            display_name: handle.to_string(),
            bio: String::new(),
            accent: String::new(),
            avatar_url: None,
            trust_level: 0,
            bsky_handle: None,
            nsfw_enabled: false,
            created_at: Utc::now() - Duration::days(age_days),
            last_active_at: Utc::now() - Duration::days(age_days.min(3)),
            role: "member".to_string(),
            visitors: clicks * 2,
            visitors_7d,
            visitors_prev: 0,
            clicks,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            verticals: vec!["dev".to_string()],
            endorse: 0.0,
        attention: clicks as f64,
        recent: clicks as f64,
        }
    }

    fn handles(members: &[MemberStat], shelf: &Shelf) -> Vec<String> {
        shelf
            .items
            .iter()
            .map(|r| members[r.index].handle.clone())
            .collect()
    }

    #[test]
    fn the_ordered_shelves_keep_the_rankers_own_order() {
        // The gate this step is judged by: where the declaration says the weights are unchanged, the
        // order must be the order the previous implementation produced. lambda = 1.0 is pure relevance,
        // so a mismatch here means this module changed a ranking it promised not to touch.
        let members = vec![
            member("quiet", 0, 0, 90, &["rust"]),
            member("busy", 40, 30, 60, &["rust", "atproto"]),
            member("fresh", 2, 4, 2, &["dev"]),
            member("steady", 10, 12, 40, &["music"]),
        ];
        let shelves = build(&members, &[], Utc::now(), &Declaration::default(), crate::dial::Lens::full(), crate::attention::Config::raw(), &crate::exposure::Config::off(), &crate::ceiling::Config::off());

        let rising = shelves.iter().find(|s| s.key == "rising").unwrap();
        let expected_rising: Vec<String> = ranking::rank_trending(members.clone(), Utc::now(), &[], 1.0)
            .iter()
            .take(PER_SHELF)
            .map(|m| m.handle.clone())
            .collect();
        assert_eq!(handles(&members, rising), expected_rising, "Rising must be the Trending ranker's order");

        let new = shelves.iter().find(|s| s.key == "new").unwrap();
        let expected_new: Vec<String> = ranking::rank_new(members.clone(), Utc::now(), &[], 1.0)
            .iter()
            .take(PER_SHELF)
            .map(|m| m.handle.clone())
            .collect();
        assert_eq!(handles(&members, new), expected_new, "New must be the New ranker's order");
    }

    #[test]
    fn for_you_diversifies_but_keeps_the_top_of_the_shelf() {
        // Three members share every tag; a fourth shares none. At lambda < 1 the shared-tag pile must
        // not be able to fill the shelf, and the first slot must still be the top relevance.
        let members = vec![
            member("a", 100, 10, 30, &["rust", "atproto"]),
            member("b", 90, 9, 30, &["rust", "atproto"]),
            member("c", 80, 8, 30, &["rust", "atproto"]),
            member("odd", 1, 1, 30, &["music"]),
        ];
        let weights = vec![("rust".to_string(), 1.0)];
        let shelves = build(&members, &weights, Utc::now(), &Declaration::default(), crate::dial::Lens::full(), crate::attention::Config::raw(), &crate::exposure::Config::default(), &crate::ceiling::Config::off());
        let for_you = shelves.iter().find(|s| s.key == "for_you").unwrap();
        let order = handles(&members, for_you);
        assert_eq!(order.first().map(String::as_str), Some("a"), "the top stays the top: {order:?}");
        assert!(order.contains(&"odd".to_string()), "novelty must reach the shelf: {order:?}");
    }

    #[test]
    fn a_why_line_cites_only_observable_facts() {
        // D27 is a boundary: a sentence that says "curated", "taste", "prior" or names the operator
        // would disclose the instance's lean on a surface a stranger reads.
        let members = vec![
            member("tagged", 5, 2, 30, &["rust"]),
            member("clicked", 12, 0, 30, &["music"]),
            member("fresh", 0, 0, 1, &["dev"]),
        ];
        let weights = vec![("rust".to_string(), 0.9)];
        let shelves = build(&members, &weights, Utc::now(), &Declaration::default(), crate::dial::Lens::full(), crate::attention::Config::raw(), &crate::exposure::Config::default(), &crate::ceiling::Config::off());
        let forbidden = ["taste", "prior", "gravity", "curated", "editor", "operator", "orbit"];
        for shelf in &shelves {
            for item in &shelf.items {
                let why = item.why.to_lowercase();
                for word in forbidden {
                    assert!(!why.contains(word), "`{why}` names the prior ({word})");
                }
                assert!(!item.why.is_empty(), "every card carries a reason");
            }
        }
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    use chrono::Duration;

    #[test]
    fn two_members_never_fill_two_slots_each() {
        let mk = |h: &str, clicks: i64, v7: i64| MemberStat {
            id: uuid::Uuid::new_v4(),
            handle: h.to_string(),
            display_name: h.to_string(),
            bio: String::new(),
            accent: String::new(),
            avatar_url: None,
            trust_level: 0,
            bsky_handle: None,
            nsfw_enabled: false,
            created_at: Utc::now() - Duration::days(30),
            last_active_at: Utc::now(),
            role: "member".to_string(),
            visitors: clicks,
            visitors_7d: v7,
            visitors_prev: 0,
            clicks,
            tags: vec![],
            verticals: vec!["dev".to_string()],
            endorse: 0.0,
            attention: clicks as f64,
            recent: clicks as f64,
        };
        let members = vec![mk("a", 76, 5), mk("b", 110, 5)];
        let shelves = build(&members, &[], Utc::now(), &Declaration::default(), crate::dial::Lens::full(), crate::attention::Config::default(), &crate::exposure::Config::default(), &crate::ceiling::Config::off());
        for s in &shelves {
            let hs: Vec<&str> = s.items.iter().map(|r| members[r.index].handle.as_str()).collect();
            println!("{} -> {:?}", s.key, hs);
            let mut uniq = hs.clone();
            uniq.sort();
            uniq.dedup();
            assert_eq!(uniq.len(), hs.len(), "{} repeats a member: {:?}", s.key, hs);
        }
    }
}

#[cfg(test)]
mod undiscovered {
    use super::*;
    use chrono::Duration;

    fn m(handle: &str, clicks: i64, recent: f64, week: i64, prev: i64) -> MemberStat {
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
            created_at: Utc::now() - Duration::days(60),
            last_active_at: Utc::now(),
            role: "member".to_string(),
            visitors: clicks,
            visitors_7d: week,
            visitors_prev: prev,
            clicks,
            tags: vec![],
            verticals: vec!["dev".to_string()],
            endorse: 0.0,
            attention: recent,
            recent,
        }
    }

    fn shelf_of<'a>(shelves: &'a [Shelf], key: &str) -> Option<&'a Shelf> {
        shelves.iter().find(|s| s.key == key)
    }

    /// THE GATE, first half: the shelf is the long tail, and momentum outranks mass — the famous member who
    /// is flat is not on it at all, and the growing small member is ahead of the flat small one.
    #[test]
    fn the_undiscovered_shelf_is_the_tail_ranked_by_momentum() {
        let members = vec![
            // Three visible members with flat traffic: high exposure, momentum 1.0.
            m("famous_flat", 900, 900.0, 300, 900),
            m("famous_steady", 800, 700.0, 200, 600),
            m("famous_quiet", 700, 600.0, 100, 300),
            // Three barely-visible members: one growing hard, one flat, one silent.
            m("small_growing", 3, 1.0, 9, 0),
            m("small_flat", 2, 2.0, 3, 9),
            m("small_silent", 1, 3.0, 0, 0),
        ];
        let shelves = build(
            &members,
            &[],
            Utc::now(),
            &Declaration::default(),
            crate::dial::Lens::full(),
            crate::attention::Config::raw(),
            &crate::exposure::Config::off(),
            &crate::ceiling::Config::off(),
        );
        let shelf = shelf_of(&shelves, "undiscovered").expect("a real long tail must render the shelf");
        let order: Vec<String> = shelf.items.iter().map(|r| members[r.index].handle.clone()).collect();

        for famous in ["famous_flat", "famous_steady", "famous_quiet"] {
            assert!(!order.contains(&famous.to_string()), "{famous} is not undiscovered: {order:?}");
        }
        assert_eq!(
            order.first().map(String::as_str),
            Some("small_growing"),
            "momentum outranks mass: {order:?}"
        );
        assert!(order.contains(&"small_flat".to_string()), "{order:?}");

        let why = &shelf.items[0].why;
        assert!(why.contains("9 visitors this week"), "the reason cites the measurement: {why}");
    }

    /// THE GATE, second half: an all-flat pool has no long tail, and the shelf is not rendered at all — no
    /// empty heading, no padded list.
    #[test]
    fn an_all_flat_pool_renders_no_undiscovered_shelf() {
        let members = vec![
            m("alpha_one", 10, 10.0, 5, 15),
            m("bravo_two", 10, 10.0, 5, 15),
            m("charlie_three", 10, 10.0, 5, 15),
        ];
        let shelves = build(
            &members,
            &[],
            Utc::now(),
            &Declaration::default(),
            crate::dial::Lens::full(),
            crate::attention::Config::raw(),
            &crate::exposure::Config::off(),
            &crate::ceiling::Config::off(),
        );
        assert!(
            shelf_of(&shelves, "undiscovered").map(|s| s.items.is_empty()).unwrap_or(true),
            "every member is at the sixtieth percentile, so nobody is undiscovered"
        );
    }

    #[test]
    fn momentum_is_this_week_against_the_members_own_baseline() {
        assert!((ranking::momentum(&m("new", 0, 0.0, 2, 0)) - 3.0).abs() < 1e-9, "two visitors, no history");
        assert!((ranking::momentum(&m("flat", 0, 0.0, 100, 300)) - 1.0).abs() < 1e-9, "a hundred a week, flat");
        assert!(ranking::momentum(&m("grow", 0, 0.0, 100, 30)) > 2.0, "growing from ten a week");
        assert!(ranking::momentum(&m("fall", 0, 0.0, 10, 300)) < 0.5, "falling from a hundred a week");
    }
}
