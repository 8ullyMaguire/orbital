//! The silence gate: a program that fails when a public surface names the instance's prior.
//!
//! D27 says the prior is applied and never named, and the only honest way to keep that true over time is
//! to make it a **test with a vocabulary**, not a reviewer's memory. This module is the vocabulary and the
//! matcher; the test that uses it walks every surface a stranger can reach.
//!
//! Three things count as naming it:
//!
//! * the **words** the implementation uses for it (`gravity`, `prior`, `taste`, `attuned`, `affinity`, …) —
//!   no page, field, sentence or API response may carry one;
//! * a **declared source's name**, whatever it is: an operator who names a source "the regulars" has named
//!   an editorial decision, and a visitor who reads it learns that someone chose who counts;
//! * a **field** that offers to report the prior — `prior`, `taste`, `gravity`, `affinity` as JSON keys.
//!
//! The matcher is deliberately dumb: case-insensitive, whole-word, substring anywhere. A clever matcher
//! would have an exemption list, and an exemption list is how a gate becomes decoration.

/// The vocabulary the prior is implemented in. Every one of these is a word an implementation uses and a
/// product must not say.
/// `folded` is deliberately **not** here: it is ordinary interface language for a panel that is closed,
/// and a vocabulary that fires on it would be switched off within a week. The gate keeps only the words
/// that nothing but the prior's implementation would ever say in public.
pub const PRIOR_WORDS: [&str; 7] = [
    "gravity",
    "damped",
    "exposure",
    "impression",
    "percentile",
    "prior",
    "taste",
    "attuned",
    "affinity",
    "taste-source",
    "instance lens",
];

/// Every place a needle occurs in `body`, case-insensitively, as a whole word. Whole-word because
/// `priority` is not `prior` and `gravityfed` is not a disclosure.
pub fn leaks(body: &str, extra: &[String]) -> Vec<String> {
    let haystack = body.to_lowercase();
    let needles = PRIOR_WORDS
        .iter()
        .map(|w| (*w).to_string())
        .chain(extra.iter().cloned());
    let mut found: Vec<String> = Vec::new();
    for needle in needles {
        let lowered = needle.to_lowercase();
        if lowered.trim().is_empty() || found.contains(&needle) {
            continue;
        }
        if occurs_as_a_word(&haystack, &lowered) {
            found.push(needle);
        }
    }
    found
}

/// A whole-word occurrence: the character before and after must not be alphanumeric. `priority` is not
/// `prior`, and a hyphen joins parts of one phrase rather than separating words.
fn occurs_as_a_word(haystack: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(at) = haystack[from..].find(needle) {
        let start = from + at;
        let end = start + needle.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
        let after_ok = haystack[end..]
            .chars()
            .next()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
        if from >= haystack.len() {
            break;
        }
    }
    false
}

/// True when nothing in `body` names the prior.
pub fn silent(body: &str, extra: &[String]) -> bool {
    leaks(body, extra).is_empty()
}

/// The words as JSON keys: a field called `prior` is a disclosure even when its value is `null`.
pub fn field_words() -> Vec<String> {
    PRIOR_WORDS.iter().map(|w| (*w).to_string()).collect()
}
