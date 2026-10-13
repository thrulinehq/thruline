//! T-3-4 — the execution sentences, swept.
//!
//! Three things are asserted mechanically rather than believed:
//!
//! 1. **The Plain half never says "pid", "process group" or "SIGTERM".** The
//!    card forbids all three by name. The Technical half says all three, which
//!    is what the register is for, so the sweep is asymmetric on purpose.
//! 2. **No execution sentence ever says a stopped run passed.** "Pass",
//!    "passed" and "Proven" do not appear in this table at all.
//! 3. **Every state and every reason has a line**, so a surface can never meet
//!    a slug the copy file has no sentence for and render the slug at a person.
//!
//! The browser's copy of these sentences lives in
//! `ui/src/workspace/hq-copy.js` under `change.model.execution`, and
//! `ui/src/workspace/change-model.test.js` renders them; this file is the
//! engine-side fence on the wording itself.

use thruline_core::run_execution::{ExecutionState, Reason};
use thruline_core::voice::{execution_line, EXECUTION_LINES};

/// The three words the card forbids in Sam's register, lowercased for the
/// comparison. "SIGTERM" is caught by the same pass.
const FORBIDDEN_IN_PLAIN: &[&str] = &["pid", "process group", "sigterm", "sigkill", "pgid"];

/// A stopped run is never a pass, and the copy may not imply otherwise.
const NEVER: &[&str] = &["passed", "a pass", "proven"];

#[test]
fn the_plain_half_never_says_pid_process_group_or_sigterm() {
    for line in EXECUTION_LINES {
        let plain = line.plain.to_lowercase();
        for word in FORBIDDEN_IN_PLAIN {
            assert!(
                !plain.contains(word),
                "`{}`'s Plain half says {word:?}, which Sam does not read: {:?}",
                line.slug,
                line.plain
            );
        }
    }
}

#[test]
fn no_execution_sentence_says_a_stopped_run_passed() {
    for line in EXECUTION_LINES {
        for half in [line.plain, line.technical] {
            let text = half.to_lowercase();
            for word in NEVER {
                assert!(
                    !text.contains(word),
                    "`{}` says {word:?}: an execution sentence describes what became \
                     of the RUN, and must never make a claim about what it showed",
                    line.slug
                );
            }
        }
    }
}

#[test]
fn every_state_and_every_reason_has_a_sentence_in_both_registers() {
    for state in [
        ExecutionState::NotRun,
        ExecutionState::Running,
        ExecutionState::Completed,
        ExecutionState::Cancelled,
        ExecutionState::Interrupted,
        ExecutionState::Blocked,
    ] {
        let line = execution_line(state.slug())
            .unwrap_or_else(|| panic!("no sentence for state {}", state.as_str()));
        assert!(!line.plain.trim().is_empty());
        assert!(!line.technical.trim().is_empty());
    }
    for reason in [
        Reason::HqRestarted,
        Reason::HqShutDown,
        Reason::CancelledByPerson,
        Reason::PidReused,
        Reason::InputsChanged,
        Reason::PrerequisiteAbsent,
    ] {
        let line = execution_line(reason.slug())
            .unwrap_or_else(|| panic!("no sentence for reason {}", reason.as_str()));
        assert!(!line.plain.trim().is_empty());
        assert!(!line.technical.trim().is_empty());
    }
}

/// The one sentence the card quotes verbatim. It is the Plain half a person
/// meets on an interrupted run's card after HQ has restarted under it, and it
/// is pinned here so a later wording change is a decision rather than a drift.
#[test]
fn the_restart_sentence_is_the_one_the_card_asks_for() {
    let line = execution_line("execution_reason_hq_restarted").expect("the sentence exists");
    assert_eq!(
        line.plain,
        "This check was interrupted when ThruLine restarted. Run it again."
    );
}

/// The browser's copy of these sentences is the same text, word for word.
#[test]
fn the_plain_sentences_match_the_browsers_copy_file_word_for_word() {
    let copy = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/workspace/hq-copy.js"),
    )
    .expect("the browser copy file must be readable");
    for line in EXECUTION_LINES {
        // The button label is a control's text, not a sentence the engine
        // serves; it is carried in the copy file under its own key.
        if line.slug == "execution_run_it_again" {
            continue;
        }
        let needle = line.plain.replace('\'', "\\'");
        assert!(
            copy.contains(line.plain) || copy.contains(&needle),
            "`{}`'s Plain sentence is not in ui/src/workspace/hq-copy.js: {:?}",
            line.slug,
            line.plain
        );
    }
}
