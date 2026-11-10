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
) -> Vec<Shelf> {
    // `lens` is the reader's dial as a fraction of this surface's declared ceiling: 1.0 is the
    // ordering the site has always served, 0.0 is no lens at all.
    if members.is_empty() {
        return Vec::new();
    }

    let position = positions(members);
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
        let chosen = select(items, decl.per_shelf, decl.for_you_lambda, |a, b| {
            polaris_candidates::jaccard(a, b)
        });
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
        items: why_lines(
            members,
            weights,
            now,
            select(
                to_items(&by_rising, &tag_ids),
                decl.per_shelf,
                decl.ordered_lambda,
                |a, b| polaris_candidates::jaccard(a, b),
            ),
        ),
    });

    // `New`: one source, pure relevance — the New ranker's own order.
    shelves.push(Shelf {
        key: "new",
        title: "New to the directory",
        subtitle: "Everyone who joined recently, newest first. No threshold to clear.",
        items: why_lines(
            members,
            weights,
            now,
            select(
                to_items(&by_new, &tag_ids),
                decl.per_shelf,
                decl.ordered_lambda,
                |a, b| polaris_candidates::jaccard(a, b),
            ),
        ),
    });

    shelves
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
            clicks,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            verticals: vec!["dev".to_string()],
            endorse: 0.0,
        attention: clicks as f64,
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
        let shelves = build(&members, &[], Utc::now(), &Declaration::default(), crate::dial::Lens::full(), crate::attention::Config::raw());

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
        let shelves = build(&members, &weights, Utc::now(), &Declaration::default(), crate::dial::Lens::full(), crate::attention::Config::raw());
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
        let shelves = build(&members, &weights, Utc::now(), &Declaration::default(), crate::dial::Lens::full(), crate::attention::Config::raw());
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
