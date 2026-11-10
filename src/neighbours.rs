//! "Members like this one": tag overlap, and an answer when there is no overlap.
//!
//! This is the surface D48 named: a *new* list has no previous order to regress, so the pool-level IDF weighting
//! (`overlap::Overlap::from_sets`) is on from the first row instead of waiting behind `orbital eval`. Sharing a tag
//! only two people carry is worth more than sharing one everybody carries, which is the difference between "this
//! person also lists rust" and "this person is like you".
//!
//! Two rules the shape enforces:
//!
//! 1. **Absence is a reason, not a shorter list.** A member with no tags is not similar to nobody — the question
//!    cannot be asked. `Reason::NoTags`, `Reason::NothingShared` and `Reason::Alone` are three different facts, and
//!    the page says which one it is instead of rendering an empty box that looks like a bug.
//! 2. **Determinism is the promise.** Order by overlap, ties by handle; shared tags by rarity, ties alphabetical.
//!    No random tiebreak, no clock, no visitor: twenty samples computed twice from the same data are the same
//!    answer, which `twenty_samples_are_stable_across_a_rebuild` proves by rebuilding the pool in reverse order.
//!
//! The `Option` contract from `polaris_candidates::Signal` is what makes the first rule expressible: `None` means
//! "this candidate has nothing to say about overlap", and it is not the same as "overlaps by zero".

use crate::models::MemberStat;
use crate::overlap::Overlap;

/// Why a neighbourhood is empty — or that it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// There are neighbours below this one.
    Shared,
    /// The member has no tags, so there is nothing to compare.
    NoTags,
    /// The member has tags and nobody else shares any of them.
    NothingShared,
    /// There is no one else in the directory.
    Alone,
}

impl Reason {
    /// The sentence a reader sees. Never a number, never a vocabulary word: what the page can say is why the list
    /// is not there, and that is all it says.
    pub fn sentence(self) -> &'static str {
        match self {
            Reason::Shared => "",
            Reason::NoTags => "No tags yet, so there is nothing to compare with anyone.",
            Reason::NothingShared => "Nothing in common with anyone here yet.",
            Reason::Alone => "The only profile here so far.",
        }
    }
}

/// One member who shares tags with the target.
#[derive(Debug, Clone)]
pub struct Neighbour {
    pub handle: String,
    pub display_name: String,
    pub shared_tags: Vec<String>,
    pub overlap: f64,
}

/// The whole answer: who, or why nobody.
#[derive(Debug, Clone)]
pub struct Neighbourhood {
    pub reason: Reason,
    pub members: Vec<Neighbour>,
}

/// The neighbours of `target` among `members`, at most `n` of them.
///
/// `members` is the candidate pool and should include the target; it is skipped by id. Tags of the whole pool set
/// the IDF weights, so the same pair of members can overlap identically in counts and differently in meaning
/// depending on how rare their shared tags are.
pub fn similar(members: &[MemberStat], target: &MemberStat, n: usize) -> Neighbourhood {
    if target.tags.is_empty() {
        return Neighbourhood {
            reason: Reason::NoTags,
            members: Vec::new(),
        };
    }
    let others: Vec<&MemberStat> = members
        .iter()
        .filter(|m| m.id != target.id && !m.tags.is_empty())
        .collect();
    if others.is_empty() {
        return Neighbourhood {
            reason: Reason::Alone,
            members: Vec::new(),
        };
    }

    let ov = Overlap::from_sets(members.iter().map(|m| m.tags.as_slice()));
    let mut scored: Vec<Neighbour> = others
        .into_iter()
        .filter_map(|m| {
            // The `Option` contract, live: `None` is "this candidate says nothing about overlap", which is a
            // filter, not a zero score. A zero score would sort as "the least similar" and pad the list.
            let overlap = ov.pair(&target.tags, &m.tags)?;
            if overlap <= 0.0 {
                return None;
            }
            let mut shared: Vec<String> = target
                .tags
                .iter()
                .filter(|t| m.tags.contains(t))
                .cloned()
                .collect();
            shared.sort_by(|a, b| {
                let ra = ov.rarity(a);
                let rb = ov.rarity(b);
                rb.partial_cmp(&ra)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(b))
            });
            Some(Neighbour {
                handle: m.handle.clone(),
                display_name: m.display_name.clone(),
                shared_tags: shared,
                overlap,
            })
        })
        .collect();

    if scored.is_empty() {
        return Neighbourhood {
            reason: Reason::NothingShared,
            members: Vec::new(),
        };
    }

    scored.sort_by(|a, b| {
        b.overlap
            .partial_cmp(&a.overlap)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.handle.cmp(&b.handle))
    });
    scored.truncate(n);
    Neighbourhood {
        reason: Reason::Shared,
        members: scored,
    }
}
