//! T-S-2 — the words. Plain first, technical one level down
//! (`03-USER-LENS.md` rules 1 and 2), through the copy file and nowhere else.

use std::collections::BTreeSet;

use thruline_core::evidence_applicability::ReasonClass;
use thruline_core::voice::{self, APPLICABILITY_LINES};

/// Words Sam does not know, from `03-USER-LENS.md` and the examined-inputs
/// card's own list, applied to this table too.
const BANNED_IN_PLAIN: &[&str] = &[
    "snapshot",
    "hash",
    "applicability",
    "predicate",
    "ledger",
    "criterion_evidence_id",
    "wsid",
    "null",
    "unknown",
    "stale",
    "recorded",
    "refused",
    "verified",
];

const EVERY_SLUG_THE_ENGINE_EMITS: &[&str] = &[
    "applicability_still_applies",
    /* T-F-1-F1 — the one slug the ENGINE never picks and a SURFACE asks for:
     * the same conclusion as `applicability_still_applies` for a record that
     * names neither the paths the rule compared nor the version it compared
     * against. It is listed here because the rule this test holds is "every
     * sentence a surface can ask for is in this table, and nothing else is",
     * and a line with no caller is a line nobody proof-read. */
    "applicability_still_applies_unnamed",
    "applicability_needs_a_recheck",
    "applicability_reason_source_changed",
    "applicability_reason_tests_changed",
    "applicability_reason_dependencies_changed",
    "applicability_reason_configuration_changed",
    "applicability_reason_criteria_changed",
];

#[test]
fn the_copy_file_holds_exactly_the_sentences_the_engine_can_ask_for() {
    let defined: BTreeSet<&str> = APPLICABILITY_LINES.iter().map(|l| l.slug).collect();
    let emitted: BTreeSet<&str> = EVERY_SLUG_THE_ENGINE_EMITS.iter().copied().collect();
    assert_eq!(
        defined, emitted,
        "a state without a sentence is a screen with a blank where an answer goes"
    );
}

#[test]
fn every_reason_class_has_a_slug_with_a_sentence_behind_it() {
    for class in [
        ReasonClass::SourceChanged,
        ReasonClass::TestsChanged,
        ReasonClass::DependenciesChanged,
        ReasonClass::ConfigurationChanged,
        ReasonClass::CriteriaChanged,
    ] {
        assert!(
            voice::applicability_line(class.slug()).is_some(),
            "{:?} has no sentence behind its slug {}",
            class,
            class.slug()
        );
    }
}

#[test]
fn no_plain_sentence_carries_engine_lexicon() {
    for line in APPLICABILITY_LINES {
        for banned in BANNED_IN_PLAIN {
            assert!(
                !line.plain.to_lowercase().contains(banned),
                "plain sentence for {:?} contains banned word {:?}: {:?}",
                line.slug,
                banned,
                line.plain
            );
        }
        assert!(
            voice::banned_in(line.plain).is_none(),
            "plain sentence for {:?} trips the §5v banned list: {:?}",
            line.slug,
            line.plain
        );
    }
}

#[test]
fn placeholders_render_with_and_without_a_path() {
    let (plain, _tech) = voice::applicability_sentence(
        "applicability_still_applies",
        Some("crates/core/src/a.rs"),
        Some("a1b2c3d"),
    )
    .expect("the slug is one of ours");
    assert_eq!(
        plain,
        "Still applies — crates/core/src/a.rs unchanged since a1b2c3d."
    );

    let (plain_no_path, _tech) =
        voice::applicability_sentence("applicability_reason_criteria_changed", None, None)
            .expect("the slug is one of ours");
    assert!(
        !plain_no_path.contains('{'),
        "an unfilled placeholder reached the sentence"
    );
}

#[test]
fn the_card_s_own_two_sentences_are_verbatim() {
    let needs = voice::applicability_line("applicability_needs_a_recheck").unwrap();
    assert_eq!(
        needs.plain,
        "Proven on the previous version — needs a re-check."
    );
    let source = voice::applicability_line("applicability_reason_source_changed").unwrap();
    assert_eq!(source.plain, "The affected code changed afterward.");
}
