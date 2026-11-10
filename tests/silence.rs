//! The gate itself, proven able to fail before it is trusted to pass.
//!
//! A test that cannot fail is a comment. Each case here plants a leak the gate must find, including the
//! ones that look innocent — a source named after an ordinary English word is still an editorial decision
//! a reader can see.

use orbital::silence::{leaks, silent, PRIOR_WORDS};

/// Every word in the vocabulary is found, in a sentence, in any case, and in a JSON key.
#[test]
fn the_gate_finds_each_word_of_the_vocabulary() {
    for word in PRIOR_WORDS {
        let body = format!(r#"<p>The page says {word} once.</p>"#);
        // `contains`, not equality: one vocabulary word can sit inside another ("taste-source" holds
        // "taste"), and reporting both is correct — the point is that neither passes.
        assert!(
            leaks(&body, &[]).contains(&word.to_string()),
            "the gate must find `{word}`, found {:?}",
            leaks(&body, &[])
        );
        let key = format!(r#"{{"{word}":null}}"#);
        assert!(!silent(&key, &[]), "a field named `{word}` is a disclosure");
    }

    let shouting = "This list shows your GRAVITY profile.";
    assert_eq!(leaks(shouting, &[]), vec!["gravity".to_string()]);
    assert_eq!(
        leaks("built on taste-source", &[]),
        vec!["taste".to_string(), "taste-source".to_string()],
        "the phrase reports both the word it is and the word inside it"
    );
}

/// Whole-word matching, because the alternative is a gate that fires on `priority` and gets switched off.
#[test]
fn ordinary_words_that_merely_contain_a_vocabulary_word_are_clean() {
    for body in [
        "Sort by priority, then by date.",
        "Priorities change; the list comes back.",
        "A gravityfed kiln is efficient.",
        "Reprioritise nothing.",
    ] {
        assert!(
            silent(body, &[]),
            "`{body}` names nothing, but the gate fired on {:?}",
            leaks(body, &[])
        );
    }
}

/// A declared source's own name is a leak, even when it is a plain English phrase — the operator chose it,
/// and a reader who sees it learns that someone decided who counts.
#[test]
fn a_declared_source_name_is_a_leak_whatever_it_is_called() {
    let extra = vec!["the regulars".to_string(), "high_trust".to_string()];
    assert_eq!(
        leaks("<p>Chosen by the regulars, mostly.</p>", &extra),
        vec!["the regulars".to_string()]
    );
    assert_eq!(leaks(r#"{"source":"high_trust"}"#, &extra), vec!["high_trust".to_string()]);
    assert!(silent("<p>Chosen by nobody in particular.</p>", &extra));
}

/// The gate reports each offender once, whatever the page does with it.
#[test]
fn an_offender_is_reported_once() {
    let body = "taste here, taste there, and Taste again";
    assert_eq!(leaks(body, &[]), vec!["taste".to_string()]);
}
