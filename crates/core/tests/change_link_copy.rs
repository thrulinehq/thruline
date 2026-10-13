//! T-2-2 — every sentence a person can be shown about a link lives in the copy
//! file, in two registers, in Sam's words.
//!
//! `docs/loop/03-USER-LENS.md` rule 1 and rule 2 are the authority: everyday
//! English on top, the record's own nouns one level down, and a fixed list of
//! engine nouns that may never appear on a human surface. This file is the
//! mechanical form of that, plus the thing that makes it hold — the set of
//! slugs the engine can emit is compared against the set the copy file
//! defines, so a new refusal cannot ship without a sentence and an old
//! sentence cannot linger without a refusal.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use thruline_core::voice::{self, ASSOCIATION_LINES};

/// The nouns `03-USER-LENS.md` §3.2 forbids on a human surface. "session id"
/// is banned; "session" is not — the card's own Plain copy says "This session
/// might belong to this Change".
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

/// Every slug the engine can put in front of a person. Derived by hand from
/// the three places a slug is produced, and pinned against the copy file
/// below, so the two cannot drift apart silently.
const EVERY_SLUG_THE_ENGINE_EMITS: &[&str] = &[
    "applies_examination",
    "applies_in_scope",
    "confirm_needs_actor",
    "link_none",
    "not_applicable_check_for_another_change",
    "not_applicable_no_changed_paths",
    "not_applicable_no_link",
    "not_applicable_no_scope",
    "not_applicable_only_suggested",
    "not_applicable_outside_scope",
    "not_applicable_rejected",
    "scope_pattern_unreadable",
    "scope_pattern_unreadable_summary",
    "session_undated",
    "session_undated_summary",
    "suggestion_filename",
    "suggestion_none",
    "suggestion_tie",
    "suggestion_timing",
];

#[test]
fn the_copy_file_holds_exactly_the_sentences_the_engine_can_ask_for() {
    let defined: BTreeSet<&str> = ASSOCIATION_LINES.iter().map(|l| l.slug).collect();
    let emitted: BTreeSet<&str> = EVERY_SLUG_THE_ENGINE_EMITS.iter().copied().collect();
    assert_eq!(
        defined, emitted,
        "a refusal without a sentence is a disabled control with no reason; a \
         sentence without a refusal is copy nobody can reach"
    );
}

/// The slug list above is not a hand-written wish: every one of these appears
/// in the module that emits it, found by reading the source.
#[test]
fn every_slug_in_the_list_appears_in_the_module_that_emits_it() {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/change_link.rs"))
            .expect("read change_link.rs");
    for slug in EVERY_SLUG_THE_ENGINE_EMITS {
        assert!(
            source.contains(&format!("\"{slug}\"")),
            "{slug:?} is listed as emitted but `change_link.rs` never names it"
        );
    }
}

#[test]
fn the_plain_register_is_in_sams_words() {
    for line in ASSOCIATION_LINES {
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

/// Never accuse. The record shows; Sam concludes. A refusal says what is not
/// counted and why — it does not say the agent lied, faked or hid anything.
#[test]
fn no_line_accuses_anyone() {
    for line in ASSOCIATION_LINES {
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
            "wrong",
        ] {
            assert!(
                !lower.contains(accusation),
                "{accusation:?} in {:?}",
                line.plain
            );
        }
    }
}

/// Both registers exist, both say something, and they are not the same string
/// — a "technical layer" that repeats the plain line is not a second level.
#[test]
fn both_registers_are_present_and_different() {
    for line in ASSOCIATION_LINES {
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

/// Placeholders are rendered, never shown. A person reading `{count}` learns
/// nothing and stops believing the rest of the screen.
#[test]
fn every_placeholder_is_filled_before_a_person_sees_it() {
    for line in ASSOCIATION_LINES {
        let (plain, technical) =
            voice::association_sentence(line.slug, 3, 2).expect("every slug renders");
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
    let (filename, _) =
        voice::association_sentence("suggestion_filename", 3, 0).expect("the suggestion sentence");
    assert_eq!(
        filename,
        "This session might belong to this Change: it changed 3 of the same files."
    );
    let (refusal, _) =
        voice::association_sentence("not_applicable_no_link", 0, 0).expect("the refusal sentence");
    assert_eq!(
        refusal,
        "Not counted for this Change: the session isn't confirmed as part of it."
    );
}

/// A count reaches a person with a noun attached, in the Technical register
/// too. "3" alone is a number with no sentence, which is Sam's first failure
/// mode.
#[test]
fn counts_carry_their_noun_and_agree_in_number() {
    let (_, one) = voice::association_sentence("suggestion_filename", 1, 0).expect("one");
    let (_, many) = voice::association_sentence("suggestion_filename", 4, 0).expect("many");
    assert!(one.contains("1 file "), "{one}");
    assert!(many.contains("4 files "), "{many}");
}

/// A slug nobody defined returns nothing rather than a half-built line.
#[test]
fn an_unknown_slug_renders_nothing_at_all() {
    assert!(voice::association_line("not_a_slug").is_none());
    assert!(voice::association_sentence("not_a_slug", 1, 1).is_none());
}

// ---------------------------------------------------------------------------
// The surfaces that print
// ---------------------------------------------------------------------------

/// No surface writes prose for a person. Every human-visible line the CLI's
/// Change verbs print comes through the copy file.
///
/// The walk is over the source of the one file that prints them, and it looks
/// for the shape a hard-coded sentence takes: a `println!` whose first
/// argument is a literal containing a space — that is, prose rather than a
/// format hole or an id.
#[test]
fn the_change_verbs_print_no_sentence_of_their_own() {
    let source =
        std::fs::read_to_string(cli_source().join("change_cmd.rs")).expect("read change_cmd.rs");
    let mut offenders = Vec::new();
    for line in source.lines() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("///") {
            continue;
        }
        let Some(rest) = t.strip_prefix("println!(\"") else {
            continue;
        };
        let Some(literal) = rest.split('"').next() else {
            continue;
        };
        // What is allowed: pure format holes and the indentation around them.
        let skeleton: String = literal
            .replace("{}", "")
            .replace("{count}", "")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let is_hole_only = literal
            .split_whitespace()
            .all(|w| w.starts_with('{') || w.is_empty());
        if !skeleton.is_empty() && !is_hole_only {
            offenders.push(literal.to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these lines write prose instead of reading the copy file: {offenders:?}"
    );
}

fn cli_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("crates/cli/src"))
        .expect("the cli crate's sources")
}
