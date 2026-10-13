//! T-2-4 fix round 1 — every sentence `thruline change new` can say lives in
//! the copy file, in two registers, in Sam's words.
//!
//! The same fence T-2-2 and T-2-3 put around their own verbs, for the same
//! reason: the set of slugs the verb can emit is compared against the set the
//! copy file defines, so a refusal cannot ship without a sentence and a
//! sentence cannot linger without a refusal. The emitting module here is the
//! CLI's `change_cmd.rs`, which is where opening a Change is asked for.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use thruline_core::voice::{self, CHANGE_OPEN_LINES};

/// The nouns `03-USER-LENS.md` §3.2 forbids on a human surface, as T-2-2's
/// fence lists them.
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
];

/// Every slug `change new` can put in front of a person.
const EVERY_SLUG_THE_VERB_EMITS: &[&str] = &[
    "change_open_needs_requester",
    "change_open_needs_title",
    "change_open_no_projects",
    "change_open_project_needed",
    "change_open_unknown_project",
    "change_opened",
];

fn cli_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("crates/cli/src"))
        .expect("the cli crate's sources")
}

#[test]
fn the_copy_file_holds_exactly_the_sentences_the_verb_can_ask_for() {
    let defined: BTreeSet<&str> = CHANGE_OPEN_LINES.iter().map(|l| l.slug).collect();
    let emitted: BTreeSet<&str> = EVERY_SLUG_THE_VERB_EMITS.iter().copied().collect();
    assert_eq!(
        defined, emitted,
        "a refusal without a sentence is a disabled control with no reason; a \
         sentence without a refusal is copy nobody can reach"
    );
}

#[test]
fn every_slug_in_the_list_appears_in_the_surface_that_emits_it() {
    let source =
        std::fs::read_to_string(cli_source().join("change_cmd.rs")).expect("change_cmd.rs");
    for slug in EVERY_SLUG_THE_VERB_EMITS {
        assert!(
            source.contains(&format!("\"{slug}\"")),
            "{slug:?} is listed as emitted but `change_cmd.rs` never names it"
        );
    }
}

#[test]
fn the_plain_register_is_in_sams_words() {
    for line in CHANGE_OPEN_LINES {
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
fn both_registers_are_present_and_different() {
    for line in CHANGE_OPEN_LINES {
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

/// Every refusal says what would let the person through. A "no" with no next
/// step is the disabled control with no reason, in a terminal
/// (`03-USER-LENS.md` rule 6).
#[test]
fn every_refusal_says_what_would_let_the_person_through() {
    for line in CHANGE_OPEN_LINES {
        if line.slug == "change_opened" {
            continue;
        }
        let plain = line.plain;
        assert!(
            plain.contains("Say ")
                || plain.contains("say ")
                || plain.contains("first")
                || plain.contains("needs"),
            "{} refuses without saying what would work: {plain:?}",
            line.slug
        );
    }
}

#[test]
fn no_line_carries_an_unrendered_placeholder() {
    for line in CHANGE_OPEN_LINES {
        for text in [line.plain, line.technical] {
            assert!(
                !text.contains('{') && !text.contains('}'),
                "a placeholder nobody fills reaches a person: {text:?}"
            );
        }
    }
}

#[test]
fn an_unknown_slug_renders_nothing_at_all() {
    assert!(voice::change_open_line("not_a_slug").is_none());
    assert!(voice::change_open_sentence("not_a_slug").is_none());
}

/// The three copy tables share no slug: one sentence, one home.
#[test]
fn the_three_copy_tables_share_no_slug() {
    let opens: BTreeSet<&str> = CHANGE_OPEN_LINES.iter().map(|l| l.slug).collect();
    let associations: BTreeSet<&str> = voice::ASSOCIATION_LINES.iter().map(|l| l.slug).collect();
    let proposals: BTreeSet<&str> = voice::PROPOSAL_LINES.iter().map(|l| l.slug).collect();
    assert!(opens.is_disjoint(&associations));
    assert!(opens.is_disjoint(&proposals));
}
