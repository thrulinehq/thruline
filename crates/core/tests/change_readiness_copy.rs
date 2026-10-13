//! T-3-5 — the readiness copy, swept mechanically.
//!
//! "Ready for approval" is the strongest thing the tool says (lens rule 5), so
//! the words it may use are fenced here rather than reviewed by eye: no colour
//! word anywhere, no percentage, no "pass" or "all clear" in the Plain
//! register, and the two waiver lines end by saying nobody checked.

use thruline_core::change_readiness::{Unsatisfied, WaiverKind};
use thruline_core::voice::{readiness_line, READINESS_LINES};

/// Words that would turn the answer into a badge. `green` is named by the card.
const BANNED_ANYWHERE: &[&str] = &["green", "amber", "red light", "traffic light"];

/// Words that would turn a Plain sentence into a result it has not got.
const BANNED_IN_PLAIN: &[&str] = &[
    "pass",
    "passed",
    "passing",
    "all clear",
    "all good",
    "everything checks out",
    "100%",
];

/// Engine nouns the Plain register may never use on a primary surface
/// (lens rule 2, and the card names these three).
const BANNED_NOUNS: &[&str] = &["obligation", "disposition", "waiver", "predicate", "ledger"];

fn lower(s: &str) -> String {
    s.to_lowercase()
}

#[test]
fn no_colour_word_and_no_percentage_anywhere() {
    for line in READINESS_LINES {
        for half in [line.plain, line.technical] {
            let text = lower(half);
            for banned in BANNED_ANYWHERE {
                assert!(
                    !text.contains(banned),
                    "{}: a readiness sentence used the colour word {banned:?}",
                    line.slug
                );
            }
            assert!(
                !half.contains('%'),
                "{}: a proportion of a proof is not a thing a person can act on",
                line.slug
            );
        }
    }
}

#[test]
fn the_plain_register_never_claims_a_pass_and_never_uses_an_engine_noun() {
    for line in READINESS_LINES {
        let text = lower(line.plain);
        for banned in BANNED_IN_PLAIN {
            assert!(
                !text.contains(banned),
                "{}: the Plain half said {banned:?}",
                line.slug
            );
        }
        for noun in BANNED_NOUNS {
            assert!(
                !text.contains(noun),
                "{}: the Plain half used the engine noun {noun:?}",
                line.slug
            );
        }
    }
}

#[test]
fn a_waiver_never_reads_like_a_result() {
    // `B7.9`–`B7.11`: both waiver sentences say, in Sam's words, that
    // nothing was checked. A reader who skims one of them must not come away
    // believing a check happened.
    for kind in [WaiverKind::Waived, WaiverKind::NotApplicable] {
        let line = readiness_line(kind.slug()).expect("every waiver has a sentence");
        assert!(
            line.plain.contains("Nobody checked this"),
            "{}: a waiver has to say nobody checked",
            line.slug
        );
        assert!(
            line.plain.contains("{who}") && line.plain.contains("{why}"),
            "{}: it names the person and the reason",
            line.slug
        );
        assert!(
            line.plain.contains("{authority}"),
            "{}: it names what it rests on",
            line.slug
        );
    }
}

#[test]
fn the_ready_sentence_names_the_version_and_promises_the_list() {
    let line = readiness_line("readiness_ready").expect("the ready sentence exists");
    assert!(
        line.plain.contains("{version}"),
        "ready is always about ONE named version"
    );
    assert!(
        line.plain.trim_end().ends_with("rests on:"),
        "the sentence ends in the list it rests on, so the two cannot be separated"
    );
}

#[test]
fn every_unsatisfying_class_has_a_sentence_in_both_registers() {
    for class in [
        Unsatisfied::NothingLooked,
        Unsatisfied::SettledNothing,
        Unsatisfied::NotProven,
        Unsatisfied::ToolAbsent,
        Unsatisfied::OtherVersion,
        Unsatisfied::Stale,
        Unsatisfied::Stopped,
        Unsatisfied::Unknown,
        Unsatisfied::InputsMissing,
        Unsatisfied::ReviewMissing,
    ] {
        let line = readiness_line(class.slug())
            .unwrap_or_else(|| panic!("{} has no sentence", class.slug()));
        assert!(!line.plain.trim().is_empty());
        assert!(!line.technical.trim().is_empty());
    }
}

#[test]
fn the_screen_copy_mirrors_this_table_word_for_word() {
    // The same fence `run_execution_copy.rs` puts on the execution lines: the
    // UI's copy file holds these sentences, and a wording change here that is
    // not made there fails the suite rather than producing two voices.
    let copy = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/workspace/hq-copy.js"),
    )
    .expect("the copy file must be readable");
    for line in READINESS_LINES {
        // The blocked pair is rendered by the disabled-control component under
        // its own keys; everything else is a sentence the model looks up.
        let escaped = line.plain.replace('\'', "\\'");
        assert!(
            copy.contains(line.plain) || copy.contains(&escaped),
            "{}: the screen copy does not carry this sentence",
            line.slug
        );
    }
}
