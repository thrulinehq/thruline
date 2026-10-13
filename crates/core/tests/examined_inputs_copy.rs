//! T-3-2 — the words. Plain first, technical one level down, and the engine
//! nouns stay in the technical half (`03-USER-LENS.md` rules 1 and 2).
//!
//! Every sentence a person can be shown about what was examined lives in
//! `voice::INPUTS_LINES`, in two registers, and this file is the fence around
//! them: a banned word in the Plain half, a slug the engine can emit with no
//! sentence behind it, and an unfilled placeholder are each a failure here
//! rather than a surprise on a screen.

use std::collections::BTreeSet;

use thruline_core::examined_inputs::Fills;
use thruline_core::voice::{self, INPUTS_LINES};

/// Words Sam does not know, from `03-USER-LENS.md` §3 rule 2 and the card.
const BANNED_IN_PLAIN: &[&str] = &[
    "snapshot id",
    "snapshot",
    "tree hash",
    "pid",
    "worktree",
    "predicate",
    "applicability",
    "candidate",
    "receipt",
    "ledger",
    "disposition",
    "packet version",
    "session id",
    "identity",
    "wsid",
    "lsof",
    "sha256",
    "digest",
    "lockfile",
    "null",
];

/// Every slug the engine can put in front of a person, by hand and pinned.
const EVERY_SLUG_THE_ENGINE_EMITS: &[&str] = &[
    "inputs_applies",
    "inputs_missing",
    "inputs_no_app_named",
    "inputs_nothing_bound",
    "inputs_other_app",
    "inputs_other_database",
    "inputs_other_edits",
    "inputs_other_folder_state",
    "inputs_other_revision",
    "inputs_other_worktree",
    "inputs_record_count",
    "inputs_unknown_app",
    "inputs_unknown_is_not_a_value",
    "inputs_unknown_version",
    "inputs_unknown_worktree",
    "prepare_app_unknown",
    "prepare_base",
    "prepare_not_written_down",
    "prepare_nothing_recorded",
    "prepare_repository",
    "prepare_running_app",
    "prepare_tool_absent",
    "prepare_tool_present",
    "prepare_tool_version_unknown",
    "prepare_uncommitted_included",
    "prepare_uncommitted_none",
    "prepare_uncommitted_unknown",
];

#[test]
fn the_copy_file_holds_exactly_the_sentences_the_engine_can_ask_for() {
    let defined: BTreeSet<&str> = INPUTS_LINES.iter().map(|l| l.slug).collect();
    let emitted: BTreeSet<&str> = EVERY_SLUG_THE_ENGINE_EMITS.iter().copied().collect();
    assert_eq!(
        defined, emitted,
        "a state without a sentence is a screen with a blank where an answer goes"
    );
}

#[test]
fn the_plain_register_is_in_sams_words() {
    for line in INPUTS_LINES {
        let plain = line.plain.to_lowercase();
        for word in BANNED_IN_PLAIN {
            assert!(
                !plain.contains(word),
                "the plain register of {:?} says {word:?}: {}",
                line.slug,
                line.plain
            );
        }
    }
}

#[test]
fn both_registers_are_present_and_different() {
    for line in INPUTS_LINES {
        assert!(!line.plain.trim().is_empty(), "{} has no plain", line.slug);
        assert!(
            !line.technical.trim().is_empty(),
            "{} has no technical",
            line.slug
        );
        assert_ne!(
            line.plain, line.technical,
            "{} says the same thing twice, so one of the two registers is missing",
            line.slug
        );
    }
}

#[test]
fn every_placeholder_is_filled_before_a_person_sees_it() {
    let fills = Fills {
        port: Some(45781),
        app: Some("serve".to_string()),
        tool: Some("cargo".to_string()),
        value: Some("a value".to_string()),
        count: Some(2),
    };
    for line in INPUTS_LINES {
        let (plain, technical) = voice::inputs_sentence(line.slug, &fills).expect("a sentence");
        for text in [&plain, &technical] {
            assert!(
                !text.contains('{') && !text.contains('}'),
                "{} left a placeholder unfilled: {text}",
                line.slug
            );
        }
    }
}

#[test]
fn a_count_reaches_a_person_with_its_noun() {
    let one = voice::inputs_sentence(
        "prepare_uncommitted_included",
        &Fills {
            count: Some(1),
            ..Fills::default()
        },
    )
    .expect("sentence");
    assert!(one.0.contains("1 file"), "{}", one.0);
    let many = voice::inputs_sentence(
        "prepare_uncommitted_included",
        &Fills {
            count: Some(3),
            ..Fills::default()
        },
    )
    .expect("sentence");
    assert!(many.0.contains("3 files"), "{}", many.0);

    // A different line's count carries a different noun, and the caller does
    // not get to choose it.
    let records = voice::inputs_sentence(
        "inputs_record_count",
        &Fills {
            count: Some(2),
            ..Fills::default()
        },
    )
    .expect("sentence");
    assert!(records.0.contains("2 records"), "{}", records.0);
}

#[test]
fn an_unknown_slug_renders_nothing_at_all() {
    assert!(voice::inputs_line("not_a_slug").is_none());
    assert!(voice::inputs_sentence("not_a_slug", &Fills::default()).is_none());
}

/// No sentence here accuses anybody. The record shows; Sam concludes.
#[test]
fn no_line_accuses_anyone() {
    const ACCUSATIONS: &[&str] = &[
        "lie",
        "lied",
        "lying",
        "fake",
        "faked",
        "dishonest",
        "cheat",
        "fraud",
        "you failed",
        "wrong of",
    ];
    for line in INPUTS_LINES {
        let text = format!("{} {}", line.plain, line.technical).to_lowercase();
        for word in ACCUSATIONS {
            assert!(!text.contains(word), "{} accuses: {text}", line.slug);
        }
    }
}

/// No sentence about a mismatch reads as a pass, and none reads as a failure of
/// the work either: an examination of another version says nothing about this
/// one, which is exactly what the sentence must say.
#[test]
fn no_mismatch_sentence_reads_as_a_pass_or_as_a_verdict() {
    for slug in [
        "inputs_missing",
        "inputs_unknown_version",
        "inputs_other_revision",
        "inputs_other_edits",
        "inputs_other_folder_state",
        "inputs_other_worktree",
        "inputs_no_app_named",
        "inputs_unknown_app",
        "inputs_other_app",
        "inputs_other_database",
    ] {
        let line = voice::inputs_line(slug).expect("line");
        let plain = line.plain.to_lowercase();
        for word in [
            "passed",
            "pass",
            "all clear",
            "fine",
            "ok",
            "broken",
            "failed",
        ] {
            assert!(
                !plain
                    .split_whitespace()
                    .any(|w| w.trim_matches(|c: char| !c.is_alphanumeric()) == word),
                "{slug} reads as a verdict: {}",
                line.plain
            );
        }
    }
}

/// Every slug in the list above appears in the module that emits it, so the
/// list cannot drift into a catalogue of sentences nothing can reach.
#[test]
fn every_slug_in_the_list_appears_in_the_module_that_emits_it() {
    let sources = [
        include_str!("../src/examined_inputs.rs"),
        include_str!("../src/change_evidence.rs"),
    ];
    for slug in EVERY_SLUG_THE_ENGINE_EMITS {
        assert!(
            sources.iter().any(|s| s.contains(slug)),
            "{slug} is in the copy file and nothing emits it"
        );
    }
}

/// T-3-2b r1 — one screen, one meaning for "version".
///
/// The Change screen already spends that word on the version of the work bound
/// to the Change (`change.version.*`). These sentences are about a different
/// record — what an examination actually looked at — and while they also said
/// "version", the Verify view could show "No version of the work is bound to
/// this Change yet" above an item reading "Checked on this version."
///
/// The only lines allowed to keep the word are the two that name a *tool's*
/// version, and they must name the tool in the same sentence, so nobody can
/// read them as a claim about the work.
#[test]
fn the_plain_register_never_spends_the_word_version_on_what_was_examined() {
    const MAY_NAME_A_TOOLS_VERSION: &[&str] =
        &["prepare_tool_present", "prepare_tool_version_unknown"];
    for line in INPUTS_LINES {
        if !line.plain.to_lowercase().contains("version") {
            continue;
        }
        assert!(
            MAY_NAME_A_TOOLS_VERSION.contains(&line.slug),
            "{} says \"version\" in the plain register, and on a Change screen that word \
             already names the version of the work: {}",
            line.slug,
            line.plain
        );
        assert!(
            line.plain.contains("{tool}"),
            "{} says \"version\" without naming the tool it belongs to: {}",
            line.slug,
            line.plain
        );
    }
}

/// The same fence around the one coverage-level sentence that is about the
/// examined code rather than about the work: it renders on the same rows.
#[test]
fn the_coverage_sentence_about_examined_code_never_says_version_either() {
    let line = voice::coverage_line("coverage_other_version").expect("line");
    assert!(
        !line.plain.to_lowercase().contains("version"),
        "the coverage sentence about another examined state says \"version\": {}",
        line.plain
    );
}

/// T-3-2b — the engine and the screen say the same sentence, or this fails.
///
/// Two copies of these words exist for a reason the project already accepted:
/// the engine serves a slug and the browser renders the sentence, so the
/// browser needs its own table. Two copies with no test between them is how
/// one of them goes stale, and a stale one here is a false explanation on the
/// one surface whose whole job is explaining a refusal. The JS golden
/// (`ui/src/workspace/hq-copy.golden.json`, itself checked byte-for-byte
/// against the renderer by `hq-copy.test.js`) is the fixed point; every
/// `inputs_*` line in [`INPUTS_LINES`] must equal its entry there.
#[test]
fn the_plain_sentences_match_the_browsers_copy_file_word_for_word() {
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/workspace/hq-copy.golden.json");
    let raw = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("{}: {e}", golden_path.display()));
    let golden: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&raw).expect("the golden is JSON");

    let mut checked = 0usize;
    for line in INPUTS_LINES {
        // Only the per-item applicability sentences are rendered by the
        // browser under this prefix; the `prepare_*` lines live elsewhere.
        let Some(value) = golden.get(&format!("change.coverage.applies.{}", line.slug)) else {
            continue;
        };
        assert_eq!(
            value.as_str(),
            Some(line.plain),
            "the engine and the browser disagree about {}",
            line.slug
        );
        checked += 1;
    }
    assert!(
        checked >= 11,
        "only {checked} applicability sentences were compared; the prefix moved \
         and this test stopped reading anything"
    );

    // T-S-3 — the freshness sentences, both registers. The browser prints
    // these on the Change screen, the Inbox row and the packet page, from one
    // derived view-model; the engine authors them. A re-wording on either side
    // alone is a failure here rather than two surfaces disagreeing in front of
    // a person.
    //
    // The golden is the browser's copy RENDERED with its own sample fills, so
    // the comparison fills the engine's template with the same two values
    // rather than comparing a template to a sentence. `hq-copy.test.js`'s
    // `SAMPLE` is where the two come from, and a change there that this file
    // does not follow shows up here as a mismatch, not as silence.
    const SAMPLE_PATH: &str = "src/app/main.rs";
    const SAMPLE_VERSION: &str = "2";
    let mut freshness = 0usize;
    for line in voice::APPLICABILITY_LINES {
        let (plain, technical) =
            voice::applicability_sentence(line.slug, Some(SAMPLE_PATH), Some(SAMPLE_VERSION))
                .expect("every applicability line resolves by its own slug");
        assert_eq!(
            golden
                .get(&format!("change.coverage.freshness.{}.plain", line.slug))
                .and_then(|v| v.as_str()),
            Some(plain.as_str()),
            "the engine and the browser disagree about {} (plain)",
            line.slug
        );
        assert_eq!(
            golden
                .get(&format!(
                    "change.coverage.freshness.{}.technical",
                    line.slug
                ))
                .and_then(|v| v.as_str()),
            Some(technical.as_str()),
            "the engine and the browser disagree about {} (technical)",
            line.slug
        );
        freshness += 1;
    }
    assert_eq!(
        freshness,
        voice::APPLICABILITY_LINES.len(),
        "every applicability line the engine can emit must have a browser copy"
    );

    // The one the sprint is about, spelled out so a silent re-wording of both
    // copies at once still has to face the sentence the card named.
    assert_eq!(
        golden
            .get("change.coverage.applies.inputs_other_worktree")
            .and_then(|v| v.as_str()),
        Some(
            "This is a second checkout of the same commit. ThruLine records one checkout per \
             Change — pick the one you're working in."
        )
    );
}

/// Acceptance 3, at the copy layer: only the line that is *about* uncommitted
/// changes may name them.
#[test]
fn only_the_edits_line_says_unsaved_changes() {
    let owners: &[&str] = &[
        "inputs_other_edits",
        "prepare_uncommitted_included",
        "prepare_uncommitted_none",
        "prepare_uncommitted_unknown",
    ];
    for line in INPUTS_LINES {
        if line.plain.to_lowercase().contains("unsaved changes") {
            assert!(
                owners.contains(&line.slug),
                "{} says \"unsaved changes\", and it is reachable with both sides clean: {}",
                line.slug,
                line.plain
            );
        }
    }
    let edits = voice::inputs_line("inputs_other_edits").expect("line");
    assert!(edits.plain.contains("unsaved changes"), "{}", edits.plain);
}
