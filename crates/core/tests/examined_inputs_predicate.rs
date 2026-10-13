//! T-3-2 — one predicate, five ways of being about something else, and the
//! census that keeps it the only one (`B6.3`, `B6.4`, `Q4`).
//!
//! The failure this file exists to stop: a green run from yesterday's server,
//! or from a sibling worktree, shown as evidence for today's candidate. Two
//! things have to be true for "a successful result from an old server must not
//! support the current candidate" to hold in a codebase rather than in a
//! document:
//!
//! 1. the predicate refuses every way an examination can be about something
//!    else, and says which in the copy file's words;
//! 2. **it is the only predicate**, so a second reader cannot grow its own idea
//!    of what "applies" means. The census at the bottom re-derives every call
//!    site from the sources and fails naming any new one.
//!
//! Every id is synthetic and no fixture here holds private content.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use thruline_core::examined_inputs::{
    observation_applies, ExaminedInputs, Input, SubjectKind, Target, Uncommitted,
};

const REV_A: &str = "1111111111111111111111111111111111111111";
const REV_B: &str = "2222222222222222222222222222222222222222";
const WSID_A: &str = "wsid1:aaaa";
const WSID_B: &str = "wsid1:bbbb";
const TREE_A: &str = "/tmp/synthetic-project";
const TREE_B: &str = "/tmp/synthetic-project-sibling";

/// The candidate: revision A, one unsaved change, one worktree, one running
/// app on one port with one database.
fn intended() -> ExaminedInputs {
    let mut inputs = ExaminedInputs::unknown_for(SubjectKind::Intended, "c1");
    inputs.base_revision = Input::known(REV_A);
    inputs.source_identity = Input::known(WSID_A);
    inputs.worktree_path = Input::known(TREE_A);
    inputs.uncommitted = Uncommitted::Present {
        files: 1,
        diff_sha256: Input::known("dd"),
    };
    inputs.target = Target::RunningApp {
        port: 45781,
        owner_pid: Input::known("101"),
        owner_exe: Input::known("/tmp/synthetic-project/target/debug/serve"),
        database_path: Input::known("/tmp/synthetic-project/data/outbox.sqlite3"),
    };
    inputs
}

/// An examination that looked at exactly the candidate.
fn examined_matching() -> ExaminedInputs {
    let mut inputs = intended();
    inputs.subject_kind = SubjectKind::CheckRun;
    inputs.subject_id = "eb1".to_string();
    inputs
}

fn slug_of(examined: &ExaminedInputs) -> &'static str {
    observation_applies(examined, &intended()).slug()
}

// ---------------------------------------------------------------------------
// The one case that applies
// ---------------------------------------------------------------------------

#[test]
fn an_examination_of_the_candidate_itself_applies() {
    let verdict = observation_applies(&examined_matching(), &intended());
    assert!(verdict.applies(), "{verdict:?}");
    assert_eq!(verdict.slug(), "inputs_applies");
}

// ---------------------------------------------------------------------------
// The five mismatch classes the card names
// ---------------------------------------------------------------------------

#[test]
fn a_different_version_of_the_code_does_not_apply() {
    let mut examined = examined_matching();
    examined.base_revision = Input::known(REV_B);
    assert_eq!(slug_of(&examined), "inputs_other_revision");
    assert!(!observation_applies(&examined, &intended()).applies());
}

#[test]
fn the_same_commit_with_different_unsaved_changes_does_not_apply() {
    let mut examined = examined_matching();
    examined.source_identity = Input::known(WSID_B);
    examined.uncommitted = Uncommitted::Present {
        files: 3,
        diff_sha256: Input::known("ee"),
    };
    assert_eq!(
        slug_of(&examined),
        "inputs_other_edits",
        "the revisions are equal; only the unsaved changes differ"
    );
}

#[test]
fn a_different_copy_of_the_project_folder_does_not_apply() {
    let mut examined = examined_matching();
    examined.worktree_path = Input::known(TREE_B);
    assert_eq!(slug_of(&examined), "inputs_other_worktree");
}

// ---------------------------------------------------------------------------
// T-3-2b — the clean sibling checkout, and the sentence it gets
// ---------------------------------------------------------------------------

/// A Change bound to a clean tree: same commit as the sibling, nothing
/// uncommitted on either side. This is the shape a real `git worktree add`
/// produces, and it is the shape the r3 adjudication found misreported.
fn intended_clean() -> ExaminedInputs {
    let mut inputs = ExaminedInputs::unknown_for(SubjectKind::Intended, "c1");
    inputs.base_revision = Input::known(REV_A);
    // `wsid1` opens its preimage with the root and the git common dir, so two
    // clean checkouts of one commit necessarily carry different identities.
    inputs.source_identity = Input::known(WSID_A);
    inputs.worktree_path = Input::known(TREE_A);
    inputs.uncommitted = Uncommitted::Clean;
    inputs
}

/// A check run from a clean sibling worktree of the same commit.
fn examined_clean_sibling() -> ExaminedInputs {
    let mut inputs = intended_clean();
    inputs.subject_kind = SubjectKind::CheckRun;
    inputs.subject_id = "eb-sibling".to_string();
    inputs.source_identity = Input::known(WSID_B);
    inputs.worktree_path = Input::known(TREE_B);
    inputs
}

/// Acceptance 2. The refusal is unchanged; the reason is the second-checkout
/// reason, in the copy file's exact words.
#[test]
fn a_clean_second_checkout_of_the_same_commit_is_refused_as_a_second_checkout() {
    let verdict = observation_applies(&examined_clean_sibling(), &intended_clean());
    assert!(
        !verdict.applies(),
        "the sibling checkout must stay refused: {verdict:?}"
    );
    assert_eq!(
        verdict.slug(),
        "inputs_other_worktree",
        "a worktree-only difference used to be reported as an edits difference"
    );
    let line = thruline_core::voice::inputs_line(verdict.slug()).expect("a sentence");
    assert_eq!(
        line.plain,
        "This is a second checkout of the same commit. ThruLine records one checkout per Change \
         — pick the one you're working in."
    );
}

/// Acceptance 3. The words are said about a dirty side and about nothing else.
#[test]
fn unsaved_changes_are_never_blamed_when_both_sides_are_clean() {
    // Every way two clean records can differ, and the sentence each one gets.
    let both_clean: &[(&str, ExaminedInputs)] = &[
        ("a second checkout", examined_clean_sibling()),
        ("the same folder, differently set up", {
            let mut same_folder = examined_clean_sibling();
            same_folder.worktree_path = Input::known(TREE_A);
            same_folder
        }),
    ];
    for (what, examined) in both_clean {
        let verdict = observation_applies(examined, &intended_clean());
        assert!(!verdict.applies(), "{what}: must stay refused");
        let line = thruline_core::voice::inputs_line(verdict.slug()).expect("a sentence");
        assert!(
            !line.plain.contains("unsaved changes"),
            "{what}: both sides are clean and the sentence blames unsaved changes: {}",
            line.plain
        );
    }
    assert_eq!(
        slug_of_against(&examined_clean_sibling(), &intended_clean()),
        "inputs_other_worktree"
    );
    let mut same_folder = examined_clean_sibling();
    same_folder.worktree_path = Input::known(TREE_A);
    assert_eq!(
        slug_of_against(&same_folder, &intended_clean()),
        "inputs_other_folder_state"
    );

    // And they are still said when one side genuinely carries some.
    let mut dirty_examined = same_folder.clone();
    dirty_examined.uncommitted = Uncommitted::Present {
        files: 2,
        diff_sha256: Input::known("ee"),
    };
    assert_eq!(
        slug_of_against(&dirty_examined, &intended_clean()),
        "inputs_other_edits"
    );
    let line = thruline_core::voice::inputs_line("inputs_other_edits").expect("a sentence");
    assert!(line.plain.contains("unsaved changes"), "{}", line.plain);

    // Dirty on the Change's side alone is the same answer.
    let mut dirty_intended = intended_clean();
    dirty_intended.uncommitted = Uncommitted::Present {
        files: 1,
        diff_sha256: Input::known("dd"),
    };
    assert_eq!(
        slug_of_against(&same_folder, &dirty_intended),
        "inputs_other_edits"
    );
}

/// A different commit is still answered as a different commit: the reorder did
/// not move the revision test, and nothing new is accepted.
#[test]
fn the_reorder_accepts_nothing_it_did_not_accept_before() {
    let mut other_commit = examined_clean_sibling();
    other_commit.base_revision = Input::known(REV_B);
    assert_eq!(
        slug_of_against(&other_commit, &intended_clean()),
        "inputs_other_revision"
    );

    // An unknown folder on either side is refused before the identity is read.
    let mut no_folder = examined_clean_sibling();
    no_folder.worktree_path = Input::Unknown;
    assert_eq!(
        slug_of_against(&no_folder, &intended_clean()),
        "inputs_unknown_worktree"
    );

    // The only thing that applies is still the thing itself.
    let mut itself = intended_clean();
    itself.subject_kind = SubjectKind::CheckRun;
    itself.subject_id = "eb-self".to_string();
    assert!(observation_applies(&itself, &intended_clean()).applies());
}

fn slug_of_against(examined: &ExaminedInputs, intended: &ExaminedInputs) -> &'static str {
    observation_applies(examined, intended).slug()
}

#[test]
fn a_different_running_copy_of_the_app_does_not_apply() {
    // Q4's own case: the same source, a server started from a sibling worktree.
    let mut examined = examined_matching();
    examined.target = Target::RunningApp {
        port: 45782,
        owner_pid: Input::known("202"),
        owner_exe: Input::known("/tmp/synthetic-project-sibling/target/debug/serve"),
        database_path: Input::known("/tmp/synthetic-project-sibling/data/outbox.sqlite3"),
    };
    assert_eq!(slug_of(&examined), "inputs_other_app");

    // The port alone is enough: the same program, listening somewhere else.
    let mut same_program = examined_matching();
    same_program.target = Target::RunningApp {
        port: 45782,
        owner_pid: Input::known("202"),
        owner_exe: Input::known("/tmp/synthetic-project/target/debug/serve"),
        database_path: Input::known("/tmp/synthetic-project/data/outbox.sqlite3"),
    };
    assert_eq!(slug_of(&same_program), "inputs_other_app");
}

#[test]
fn the_same_app_on_a_different_database_does_not_apply() {
    let mut examined = examined_matching();
    examined.target = Target::RunningApp {
        port: 45781,
        owner_pid: Input::known("101"),
        owner_exe: Input::known("/tmp/synthetic-project/target/debug/serve"),
        database_path: Input::known("/tmp/other/outbox.sqlite3"),
    };
    assert_eq!(slug_of(&examined), "inputs_other_database");
}

// ---------------------------------------------------------------------------
// Unknown is not a match, ever
// ---------------------------------------------------------------------------

#[test]
fn unknown_never_becomes_a_match() {
    // Two probes that both failed did not agree about anything.
    let mut examined = examined_matching();
    examined.base_revision = Input::Unknown;
    let mut candidate = intended();
    candidate.base_revision = Input::Unknown;
    let verdict = observation_applies(&examined, &candidate);
    assert!(
        !verdict.applies(),
        "two unknowns must not compare equal: {verdict:?}"
    );
    assert_eq!(verdict.slug(), "inputs_unknown_version");

    assert!(!Input::Unknown.matches(&Input::Unknown));
    assert!(!Input::Unknown.matches(&Input::known("x")));
    assert!(!Input::known("x").matches(&Input::Unknown));
    assert!(Input::known("x").matches(&Input::known("x")));
}

#[test]
fn the_word_unknown_is_not_the_state_unknown() {
    // A field whose real value happens to be the word is a KNOWN value, and it
    // still does not match an unknown.
    let literal = Input::known("unknown");
    assert!(literal.is_known());
    assert!(!literal.matches(&Input::Unknown));
    assert_eq!(literal.value(), Some("unknown"));

    // An empty probe answer is the state, not a value.
    assert_eq!(Input::known("   "), Input::Unknown);
    assert_eq!(Input::from_option(None), Input::Unknown);
    assert_eq!(Input::from_option(Some(String::new())), Input::Unknown);
}

#[test]
fn an_unreadable_app_probe_is_not_a_match() {
    let mut examined = examined_matching();
    examined.target = Target::RunningApp {
        port: 45781,
        owner_pid: Input::Unknown,
        owner_exe: Input::Unknown,
        database_path: Input::Unknown,
    };
    assert_eq!(slug_of(&examined), "inputs_unknown_app");
}

#[test]
fn a_running_app_result_on_a_change_that_names_no_app_does_not_apply() {
    let mut candidate = intended();
    candidate.target = Target::None;
    let verdict = observation_applies(&examined_matching(), &candidate);
    assert_eq!(verdict.slug(), "inputs_no_app_named");
    assert!(!verdict.applies());
}

#[test]
fn an_examination_that_looked_at_no_running_app_is_judged_on_its_source_alone() {
    // A `cargo test` run is not a browser examination; it is not refused for
    // failing to identify a server it never spoke to.
    let mut examined = examined_matching();
    examined.target = Target::None;
    assert!(observation_applies(&examined, &intended()).applies());
}

#[test]
fn every_refusal_has_a_sentence_and_the_two_the_card_names_are_these_words() {
    for slug in [
        "inputs_applies",
        "inputs_missing",
        "inputs_unknown_version",
        "inputs_other_revision",
        "inputs_other_edits",
        "inputs_other_folder_state",
        "inputs_unknown_worktree",
        "inputs_other_worktree",
        "inputs_no_app_named",
        "inputs_unknown_app",
        "inputs_other_app",
        "inputs_other_database",
    ] {
        assert!(
            thruline_core::voice::inputs_line(slug).is_some(),
            "no sentence for {slug}"
        );
    }
    let line = thruline_core::voice::inputs_line("inputs_other_app").expect("line");
    assert_eq!(
        line.plain,
        "This came from a different running copy of the app."
    );
    let line = thruline_core::voice::inputs_line("inputs_other_revision").expect("line");
    assert_eq!(
        line.plain,
        "This was checked on different code than this Change is set up against."
    );
}

// ---------------------------------------------------------------------------
// The census: one predicate, enumerated from the sources
// ---------------------------------------------------------------------------

/// Every non-test call of the predicate, re-derived from every workspace
/// member's sources, must be exactly this list.
///
/// The reason each one may decide whether an observation applies:
///
/// * `change_evidence.rs::applicability_of` — the one place a linked piece of
///   evidence is asked whether it is about the version this Change is bound
///   to. It calls the predicate, carries the slug out for the surface to look
///   up, and decides nothing itself.
/// * `finding_standing.rs::recheck_saw_changed_code` (ADV-review-truth-1) —
///   asks whether the code a re-check examined differs from the code a review
///   finding was raised on, so a new version id on unchanged code cannot mark
///   the finding fixed. It reads only two slugs (`inputs_other_revision`,
///   `inputs_other_edits`) as "the code moved" and refuses on every other
///   answer; it attaches nothing to a Change and decides no verdict.
///
/// The test below is named for the one caller it was written with; what it
/// asserts is that the callers are exactly this list.
const PREDICATE_CALL_SITES: &[(&str, &str)] = &[
    ("change_evidence.rs", "applicability_of"),
    ("finding_standing.rs", "recheck_saw_changed_code"),
];

#[test]
fn the_predicate_has_exactly_one_caller_and_it_is_named_here() {
    let found = predicate_call_sites(None);
    let expected: BTreeSet<(String, String)> = PREDICATE_CALL_SITES
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    assert_eq!(
        found,
        expected,
        "the callers of `{}` drifted.\n unlisted: {:?}\n listed but absent: {:?}\n\
         A new caller must be added here deliberately, with the reason it may \
         decide whether a result is about this version.",
        thruline_core::examined_inputs::THE_PREDICATE,
        found.difference(&expected).collect::<Vec<_>>(),
        expected.difference(&found).collect::<Vec<_>>()
    );
}

/// The census's own negative control: a second caller, injected into a real
/// source file, must be found. Without this the walk could be reading nothing
/// and the census above would pass forever.
#[test]
fn the_census_finds_a_second_caller_when_one_is_injected() {
    let found = predicate_call_sites(Some((
        "change.rs",
        "\nfn a_second_reader_that_decides_for_itself() {\n    \
         let _ = crate::examined_inputs::observation_applies(todo!(), todo!());\n}\n",
    )));
    assert!(
        found.contains(&(
            "change.rs".to_string(),
            "a_second_reader_that_decides_for_itself".to_string()
        )),
        "the walk did not see an injected caller, so it is not reading the sources: {found:?}"
    );
}

/// The predicate gates attachment to a Change, never a verdict. No module on
/// the verdict path may even mention it.
#[test]
fn the_verdict_path_does_not_read_the_examined_inputs_layer() {
    let forbidden = [
        "premise.rs",
        "nli.rs",
        "chain.rs",
        "adjudicate.rs",
        "mind.rs",
        "verifier.rs",
    ];
    let mut offenders = Vec::new();
    for src in member_sources() {
        walk(&src, &mut |path, raw| {
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if forbidden.contains(&file.as_str()) && raw.contains("examined_inputs") {
                offenders.push(file);
            }
        });
    }
    assert!(
        offenders.is_empty(),
        "the verdict path read the examined-inputs layer: {offenders:?}"
    );
}

fn predicate_call_sites(injection: Option<(&str, &str)>) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let mut injected = injection.is_none();
    for src in member_sources() {
        walk(&src, &mut |path, raw| {
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut text = raw.to_string();
            if let Some((target, snippet)) = injection {
                if file == target && !injected {
                    text.push_str(snippet);
                    injected = true;
                }
            }
            let stripped = strip_cfg_test(&text);
            let mut current = String::new();
            for line in stripped.lines() {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed
                    .strip_prefix("pub fn ")
                    .or_else(|| trimmed.strip_prefix("fn "))
                    .or_else(|| trimmed.strip_prefix("pub(crate) fn "))
                {
                    current = rest
                        .split(['(', '<', ' '])
                        .next()
                        .unwrap_or_default()
                        .to_string();
                }
                // The definition itself is not a call of itself.
                if current == thruline_core::examined_inputs::THE_PREDICATE {
                    continue;
                }
                if line.contains(&format!(
                    "{}(",
                    thruline_core::examined_inputs::THE_PREDICATE
                )) && !line.contains("pub fn ")
                    && !line.contains("//")
                {
                    found.insert((file.clone(), current.clone()));
                }
            }
        });
    }
    found
}

/// `#[cfg(test)]` modules are tests; they may call anything.
fn strip_cfg_test(raw: &str) -> String {
    let mut out = String::new();
    let mut depth: i32 = 0;
    let mut skipping = false;
    for line in raw.lines() {
        if !skipping && line.trim_start().starts_with("#[cfg(test)]") {
            skipping = true;
            depth = 0;
            continue;
        }
        if skipping {
            depth += line.matches('{').count() as i32;
            depth -= line.matches('}').count() as i32;
            if depth <= 0 && line.contains('}') {
                skipping = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn member_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let crates = root.join("crates");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&crates) {
        for entry in entries.flatten() {
            let src = entry.path().join("src");
            if src.is_dir() {
                out.push(src);
            }
        }
    }
    out
}

fn walk(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                visit(&path, &raw);
            }
        }
    }
}
