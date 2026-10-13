//! T-3-1 — every sentence a person can be shown about what bears on a thing
//! that counts as done lives in the copy file, in two registers, in Sam's
//! words.
//!
//! The same sweep `change_link_copy.rs` runs over the link layer, run over the
//! coverage layer, plus the one thing this card adds: the Plain register may
//! not say "criterion", "mapping" or "run id", and the sentence
//! `03-USER-LENS.md` rule 5 names by hand — "no test ran for this" — is
//! asserted word for word.

use std::collections::BTreeSet;
use std::path::Path;

use thruline_core::change_evidence::{CoverageState, LinkBasis};
use thruline_core::voice::{self, COVERAGE_LINES};

/// The nouns `03-USER-LENS.md` §3.2 forbids on a human surface, plus the three
/// this card adds by name.
const BANNED_ON_A_HUMAN_SURFACE: &[&str] = &[
    "candidate",
    "applicability",
    "disposition",
    "packet version",
    "ledger",
    "session id",
    "tree hash",
    "predicate",
    "provenance",
    "append-only",
    "sha256",
    "criterion",
    "criteria",
    "mapping",
    "run id",
];

/// Every slug the engine can put in front of a person, by hand and pinned.
const EVERY_SLUG_THE_ENGINE_EMITS: &[&str] = &[
    "basis_declared",
    "basis_manual",
    "basis_named",
    "basis_not_bearing",
    "coverage_evidence",
    "coverage_link_rows",
    "coverage_no_test_ran",
    // T-3-2 added a fifth coverage state: something ran, and not on this
    // version. A result about another version is not a smaller result.
    "coverage_other_version",
    "coverage_ran_checked_nothing",
    "coverage_tool_absent",
    "item_checked_nothing",
    "item_out_of_reach",
    "link_needs_actor",
    "link_no_criteria",
    "link_not_in_reach",
    "link_unknown_criterion",
];

#[test]
fn the_copy_file_holds_exactly_the_sentences_the_engine_can_ask_for() {
    let defined: BTreeSet<&str> = COVERAGE_LINES.iter().map(|l| l.slug).collect();
    let emitted: BTreeSet<&str> = EVERY_SLUG_THE_ENGINE_EMITS.iter().copied().collect();
    assert_eq!(
        defined, emitted,
        "a state without a sentence is a screen with a blank where an answer goes"
    );
}

/// Every slug the four coverage states and the four bases can produce is in the
/// list above — read off the types rather than off a memory of them.
#[test]
fn every_state_and_basis_has_a_sentence() {
    for state in [
        CoverageState::Evidence,
        CoverageState::None,
        CoverageState::AbsentTool,
        CoverageState::RanNothing,
        CoverageState::OtherVersion,
    ] {
        assert!(
            voice::coverage_line(state.slug()).is_some(),
            "no sentence for coverage state {:?}",
            state.as_str()
        );
    }
    for basis in [
        LinkBasis::Declared,
        LinkBasis::Named,
        LinkBasis::Manual,
        LinkBasis::NotBearing,
    ] {
        assert!(
            voice::coverage_line(basis.label_slug()).is_some(),
            "no sentence for basis {:?}",
            basis.as_str()
        );
    }
}

/// The slug list is not a wish: each one appears in the module that emits it.
#[test]
fn every_slug_in_the_list_appears_in_the_module_that_emits_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources = [
        std::fs::read_to_string(root.join("src/change_evidence.rs")).expect("change_evidence.rs")
    ]
    .concat();
    for slug in EVERY_SLUG_THE_ENGINE_EMITS {
        // `coverage_link_rows` is served as a count with its noun by the
        // surface, not emitted by the engine as a slug; every other one is.
        if *slug == "coverage_link_rows" {
            continue;
        }
        assert!(
            sources.contains(&format!("\"{slug}\"")),
            "{slug:?} is listed as emitted but no engine module names it"
        );
    }
}

#[test]
fn the_plain_register_is_in_sams_words() {
    for line in COVERAGE_LINES {
        let lower = line.plain.to_ascii_lowercase();
        for banned in BANNED_ON_A_HUMAN_SURFACE {
            assert!(
                !lower.contains(banned),
                "engine noun {banned:?} in Plain copy: {:?}",
                line.plain
            );
        }
        assert!(
            voice::banned_in(line.plain).is_none(),
            "engine lexicon in {:?}: {:?}",
            line.plain,
            voice::banned_in(line.plain)
        );
    }
}

#[test]
fn no_line_accuses_anyone() {
    for line in COVERAGE_LINES {
        let lower = line.plain.to_ascii_lowercase();
        for accusation in [
            "lie",
            "lied",
            "fake",
            "faked",
            "hid ",
            "hidden",
            "dishonest",
            "false claim",
        ] {
            assert!(
                !lower.contains(accusation),
                "{accusation:?} in {:?}",
                line.plain
            );
        }
    }
}

/// No coverage sentence may read as a pass. "Proven", "passed", "all clear",
/// "green", "verified" and "ready" are the words that would turn an absence of
/// evidence into a badge.
#[test]
fn no_coverage_sentence_reads_as_a_pass() {
    for line in COVERAGE_LINES {
        let lower = line.plain.to_ascii_lowercase();
        for word in ["all clear", "passed", "green", "verified", "ready", "✓"] {
            assert!(!lower.contains(word), "{word:?} in {:?}", line.plain);
        }
    }
}

#[test]
fn both_registers_are_present_and_different() {
    for line in COVERAGE_LINES {
        assert!(!line.plain.trim().is_empty(), "{} has no Plain", line.slug);
        assert!(
            !line.technical.trim().is_empty(),
            "{} has no Technical",
            line.slug
        );
        assert_ne!(
            line.plain, line.technical,
            "{} says the same thing twice",
            line.slug
        );
    }
}

#[test]
fn every_placeholder_is_filled_before_a_person_sees_it() {
    for line in COVERAGE_LINES {
        let (plain, technical) =
            voice::coverage_sentence(line.slug, "cargo", 3).expect("every slug renders");
        for rendered in [&plain, &technical] {
            assert!(
                !rendered.contains('{') && !rendered.contains('}'),
                "unfilled placeholder in {rendered:?}"
            );
        }
    }
}

/// The two sentences the card writes out, word for word.
#[test]
fn the_cards_own_sentences_are_what_the_copy_file_says() {
    let (none, _) = voice::coverage_sentence("coverage_no_test_ran", "", 0).expect("the sentence");
    assert_eq!(none, "No test ran for this.");
    let (absent, _) =
        voice::coverage_sentence("coverage_tool_absent", "playwright", 0).expect("the sentence");
    assert_eq!(
        absent,
        "The check this needs isn't available here: playwright."
    );
}

#[test]
fn a_count_reaches_a_person_with_its_noun() {
    let (one, _) = voice::coverage_sentence("coverage_link_rows", "", 1).expect("one");
    let (many, _) = voice::coverage_sentence("coverage_link_rows", "", 4).expect("many");
    assert_eq!(one, "1 link in the record behind this.");
    assert_eq!(many, "4 links in the record behind this.");
}

#[test]
fn an_unknown_slug_renders_nothing_at_all() {
    assert!(voice::coverage_line("not_a_slug").is_none());
    assert!(voice::coverage_sentence("not_a_slug", "x", 1).is_none());
}
