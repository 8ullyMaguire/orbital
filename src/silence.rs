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
pub const PRIOR_WORDS: [&str; 8] = [
    "gravity",
    "prior",
    "taste",
    "attuned",
    "affinity",
    "folded",
    "taste-source",
    "instance lens",
];

/// Every place a needle occurs in `body`, case-insensitively, as a whole word. Whole-word because
/// `priority` is not `prior` and `gravityfed` is not a disclosure.
pub fn leaks(body: &str, extra: &[String]) -> Vec<String> {
    unimplemented!("RED: the matcher")
}

/// True when nothing in `body` names the prior.
pub fn silent(body: &str, extra: &[String]) -> bool {
    leaks(body, extra).is_empty()
}

/// The words as JSON keys: a field called `prior` is a disclosure even when its value is `null`.
pub fn field_words() -> Vec<String> {
    PRIOR_WORDS.iter().map(|w| (*w).to_string()).collect()
}
