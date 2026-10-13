//! T-2-3 — every sentence a person can be shown about a proposed criterion, a
//! suggested check or an acceptance lives in the copy file, in two registers, in
//! Sam's words.
//!
//! `docs/loop/03-USER-LENS.md` rules 1, 2, 4, 5 and 6 are the authority:
//! everyday English on top, the record's own nouns one level down, no accusation,
//! nothing missing rendered as a pass, and every refusal saying what would fix
//! it. This file is the mechanical form of that, plus the thing that makes it
//! hold — the set of slugs the engine can emit is compared against the set the
//! copy file defines, so a new refusal cannot ship without a sentence and an old
//! sentence cannot linger without a refusal.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use thruline_core::voice::{self, PROPOSAL_LINES};

/// The nouns `03-USER-LENS.md` §3.2 forbids on a human surface, plus the three
/// this card adds: no "criterion id", no "ask", no "predicate".
///
/// "ask" is banned as a **substring**, which is the strictest reading and the
/// one that cannot be argued with — so the Plain register says "request" and
/// "what you wrote", and never "asked".
const BANNED_ON_A_HUMAN_SURFACE: &[&str] = &[
    "ask",
    "candidate",
    "applicability",
    "criterion id",
    "disposition",
    "event id",
    "ledger",
    "packet version",
    "predicate",
    "provenance",
    "session id",
    "sha256",
    "tree hash",
];

/// Every slug the proposer, the profile or the acceptance act can put in front
/// of a person. Pinned against the copy file below and against the module that
/// emits it, so the three cannot drift apart silently.
const EVERY_SLUG_THE_ENGINE_EMITS: &[&str] = &[
    "accept_edit_not_selected",
    "accept_is_not_approval",
    "accept_needs_a_criterion",
    "accept_needs_actor",
    "accept_nothing_new",
    "accept_unknown_criterion",
    "accept_version_limit",
    "accept_would_overwrite_your_words",
    "accept_would_weaken",
    "accept_written",
    "accepted_scope_none",
    "check_advisory",
    "check_mandatory",
    "check_needs_a_recipe",
    "check_not_available_here",
    "check_ready",
    "criterion_as_proposed",
    "criterion_not_observable_as_written",
    "criterion_your_words",
    "note_fragment",
    "note_no_condition",
    "note_no_outcome",
    "note_no_subject",
    "note_masked",
    "note_repeats_a_line_of_another_request",
    "note_repeats_an_earlier_line",
    "note_too_long",
    "note_too_short",
    "profile_is_not_evidence",
    "profile_no_paths",
    "proposal_none",
    "proposal_reasoning",
    "reproposal_changes_nothing",
    "reproposal_removed",
    "reproposal_restated",
    "reproposal_weakened",
    "scope_from_the_work",
    "scope_none_found",
    "scope_over_the_bound",
    "scope_versions_bounded",
    "type_authorization_sensitive",
    "type_backend_behaviour",
    "type_data_schema",
    "type_documentation",
    "type_interface_behaviour",
];

#[test]
fn the_copy_file_holds_exactly_the_sentences_the_engine_can_ask_for() {
    let defined: BTreeSet<&str> = PROPOSAL_LINES.iter().map(|l| l.slug).collect();
    let emitted: BTreeSet<&str> = EVERY_SLUG_THE_ENGINE_EMITS.iter().copied().collect();
    assert_eq!(
        defined, emitted,
        "a refusal without a sentence is a disabled control with no reason; a \
         sentence without a refusal is copy nobody can reach"
    );
}

/// The slug list is not a wish: every one of them appears in the module that
/// emits it, found by reading the source.
#[test]
fn every_slug_in_the_list_appears_in_the_module_that_emits_it() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/change_propose.rs"),
    )
    .expect("read change_propose.rs");
    for slug in EVERY_SLUG_THE_ENGINE_EMITS {
        assert!(
            source.contains(&format!("\"{slug}\"")),
            "{slug:?} is listed as emitted but `change_propose.rs` never names it"
        );
    }
}

#[test]
fn the_plain_register_is_in_sams_words() {
    for line in PROPOSAL_LINES {
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

/// The four words the card requires in the Plain register, each present
/// somewhere in it.
#[test]
fn the_plain_register_uses_the_words_the_lens_names() {
    let all: String = PROPOSAL_LINES
        .iter()
        .map(|l| l.plain.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    for required in [
        "what counts as done",
        "proposed",
        "accepted",
        "still needs evidence",
    ] {
        assert!(
            all.contains(required),
            "{required:?} appears in no Plain sentence"
        );
    }
}

/// Never accuse. The record shows; Sam concludes.
#[test]
fn no_line_accuses_anyone() {
    for line in PROPOSAL_LINES {
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
            "sloppy",
            "lazy",
        ] {
            assert!(
                !lower.contains(accusation),
                "{accusation:?} in {:?}",
                line.plain
            );
        }
    }
}

/// Both registers exist, both say something, and they are not the same string.
#[test]
fn both_registers_are_present_and_different() {
    for line in PROPOSAL_LINES {
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

/// Placeholders are rendered, never shown.
#[test]
fn every_placeholder_is_filled_before_a_person_sees_it() {
    for line in PROPOSAL_LINES {
        let (plain, technical) =
            voice::proposal_sentence(line.slug, 3, 2, "semgrep").expect("every slug renders");
        for rendered in [&plain, &technical] {
            assert!(
                !rendered.contains('{') && !rendered.contains('}'),
                "unfilled placeholder in {rendered:?}"
            );
        }
    }
}

/// A count reaches a person with a noun attached, and the noun agrees in number.
#[test]
fn counts_carry_their_noun_and_agree_in_number() {
    let (one, _) = voice::proposal_sentence("accept_written", 1, 4, "").expect("one");
    let (many, _) = voice::proposal_sentence("accept_written", 5, 4, "").expect("many");
    assert!(one.contains("1 thing to check"), "{one}");
    assert!(many.contains("5 things to check"), "{many}");
    let (one_list, _) = voice::proposal_sentence("scope_versions_bounded", 1, 20, "").expect("one");
    let (many_lists, _) =
        voice::proposal_sentence("scope_versions_bounded", 3, 20, "").expect("many");
    assert!(one_list.starts_with("1 list "), "{one_list}");
    assert!(many_lists.starts_with("3 lists "), "{many_lists}");
    // Found by hand: the first cut of this refusal read "1 lines you wrote
    // yourself". A count with the wrong noun is the paper cut §4 exists against.
    let (one_line, _) =
        voice::proposal_sentence("accept_would_overwrite_your_words", 1, 2, "").expect("one");
    let (two_lines, _) =
        voice::proposal_sentence("accept_would_overwrite_your_words", 2, 2, "").expect("two");
    assert!(one_line.contains("over 1 line you"), "{one_line}");
    assert!(two_lines.contains("over 2 lines you"), "{two_lines}");
}

/// No number reaches a person without a noun beside it.
///
/// The walk is over every Plain sentence, rendered with a count of one and of
/// two: a fill that produced "1 lines" or a bare "2" would be caught here rather
/// than on somebody's screen.
#[test]
fn no_number_arrives_without_its_noun() {
    for line in PROPOSAL_LINES {
        for count in [1u64, 2] {
            let (plain, _) =
                voice::proposal_sentence(line.slug, count, 20, "semgrep").expect("renders");
            let words: Vec<&str> = plain.split_whitespace().collect();
            for (index, word) in words.iter().enumerate() {
                if word.trim_matches(|c: char| !c.is_ascii_digit()) != count.to_string() {
                    continue;
                }
                let next = words.get(index + 1).copied().unwrap_or_default();
                let noun = next.trim_matches(|c: char| !c.is_alphabetic());
                assert!(
                    !noun.is_empty(),
                    "{}: a number with nothing after it in {plain:?}",
                    line.slug
                );
                // Plural agreement, on the nouns these sentences actually count.
                if ["line", "list", "thing"].contains(&noun) {
                    assert_eq!(
                        count, 1,
                        "{}: singular noun for {count} in {plain:?}",
                        line.slug
                    );
                }
                if ["lines", "lists", "things"].contains(&noun) {
                    assert_ne!(count, 1, "{}: plural noun for 1 in {plain:?}", line.slug);
                }
            }
        }
    }
}

/// Every refusal says what would fix it. Rule 6: a control that says no without
/// saying what would make it yes is the disabled button Sam gives up on.
#[test]
fn every_refusal_says_what_would_fix_it() {
    // (slug, a phrase the Plain sentence must contain to be actionable)
    let expected: &[(&str, &str)] = &[
        ("accept_needs_actor", "say who"),
        ("accept_needs_a_criterion", "nothing to accept"),
        ("accept_edit_not_selected", "choose it"),
        ("accept_version_limit", "open a new change"),
        ("accept_nothing_new", "nothing new"),
        (
            "accept_would_weaken",
            "say plainly that you are replacing them",
        ),
        (
            "accept_would_overwrite_your_words",
            "say plainly that you are replacing them",
        ),
        ("check_not_available_here", "not available here"),
        ("check_needs_a_recipe", "cannot run"),
    ];
    for (slug, phrase) in expected {
        let (plain, _) = voice::proposal_sentence(slug, 2, 20, "semgrep").expect("the sentence");
        assert!(
            plain.to_ascii_lowercase().contains(phrase),
            "{slug}: {plain:?} does not say {phrase:?}"
        );
    }
}

/// Nothing missing is ever rendered as a pass. Every sentence about a check that
/// has not run says so, and none of them says anything passed.
#[test]
fn a_check_that_has_not_run_never_reads_as_a_result() {
    for slug in [
        "check_ready",
        "check_mandatory",
        "check_advisory",
        "check_not_available_here",
        "check_needs_a_recipe",
        "profile_is_not_evidence",
    ] {
        let (plain, _) = voice::proposal_sentence(slug, 0, 0, "semgrep").expect("the sentence");
        let lower = plain.to_ascii_lowercase();
        for claim in [
            "passed",
            "passes",
            "all clear",
            "looks good",
            "is fine",
            "ok",
        ] {
            assert!(!lower.contains(claim), "{slug}: {plain:?} claims {claim:?}");
        }
    }
    let (plain, _) = voice::proposal_sentence("profile_is_not_evidence", 0, 0, "").expect("line");
    assert!(plain.contains("still needs evidence"), "{plain}");
}

/// The sentence the card writes out, word for word.
#[test]
fn the_cards_own_sentence_is_what_the_copy_file_says() {
    let (plain, _) =
        voice::proposal_sentence("check_not_available_here", 0, 0, "semgrep").expect("the line");
    assert_eq!(plain, "Not available here: semgrep.");
}

/// Accepting is not approving, said on the same screen as the acceptance.
#[test]
fn the_acceptance_sentence_says_it_is_not_an_approval() {
    let (plain, technical) =
        voice::proposal_sentence("accept_is_not_approval", 0, 0, "").expect("the line");
    assert!(
        plain.to_ascii_lowercase().contains("not approving"),
        "{plain}"
    );
    assert!(
        plain.to_ascii_lowercase().contains("nobody has approved"),
        "{plain}"
    );
    assert!(technical.contains("B5.7"), "{technical}");
}

/// A slug nobody defined renders nothing at all.
#[test]
fn an_unknown_slug_renders_nothing_at_all() {
    assert!(voice::proposal_line("not_a_slug").is_none());
    assert!(voice::proposal_sentence("not_a_slug", 1, 1, "x").is_none());
}

/// The two tables do not collide: a slug means one sentence, whichever table it
/// is in. Two tables with one shared slug would be two answers to one question.
#[test]
fn the_two_copy_tables_share_no_slug() {
    let links: BTreeSet<&str> = voice::ASSOCIATION_LINES.iter().map(|l| l.slug).collect();
    let proposals: BTreeSet<&str> = PROPOSAL_LINES.iter().map(|l| l.slug).collect();
    let shared: Vec<&&str> = links.intersection(&proposals).collect();
    assert!(shared.is_empty(), "the tables share {shared:?}");
}

// ---------------------------------------------------------------------------
// The surfaces that print
// ---------------------------------------------------------------------------

/// No surface writes prose for a person. Every human-visible line the CLI's
/// Change verbs print comes through the copy file.
///
/// The same walk `change_link_copy.rs` runs, re-run here because this card adds
/// two verbs to the same file and a guard that is not re-run against new code is
/// not a guard.
#[test]
fn the_change_verbs_print_no_sentence_of_their_own() {
    let source =
        std::fs::read_to_string(cli_source().join("change_cmd.rs")).expect("read change_cmd.rs");
    let mut offenders = Vec::new();
    for line in source.lines() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        let Some(rest) = t.strip_prefix("println!(\"") else {
            continue;
        };
        let Some(literal) = rest.split('"').next() else {
            continue;
        };
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
