use super::*;

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_string()).collect()
}

const PYTEST_OK: &str = "..                                                     [100%]\n\
                         ===================== 2 passed in 0.03s =====================\n";

#[test]
fn a_direct_runner_becomes_a_check_with_parsed_numbers() {
    let out = check_from_exec(&argv(&["pytest", "-q"]), 0, PYTEST_OK);
    let draft = out.draft().expect("a check");
    assert_eq!(draft.runner, Runner::Pytest);
    assert!(!draft.via_shell);
    assert_eq!(draft.exit_code, 0);
    let parsed = draft.outcome.parsed().expect("numbers");
    assert_eq!(
        (parsed.passed, parsed.failed, parsed.tests_total),
        (2, 0, 2)
    );
}

#[test]
fn a_runner_inside_a_shell_payload_is_found_and_its_own_argv_is_recorded() {
    let out = check_from_exec(&argv(&["bash", "-lc", "pytest -q"]), 0, PYTEST_OK);
    let draft = out.draft().expect("a check");
    assert!(draft.via_shell);
    assert_eq!(
        draft.runner_argv,
        argv(&["pytest", "-q"]),
        "argv_hash would have committed to the shell wrapper"
    );
}

#[test]
fn an_exit_code_of_zero_with_unreadable_output_is_a_check_with_no_numbers() {
    // The one mistake this file must make impossible. `pytest` ran, it exited
    // 0, and the output says nothing — that is a check whose RESULT is
    // unverifiable, not a clean run.
    let out = check_from_exec(&argv(&["pytest", "-q"]), 0, "collecting ...\n");
    let draft = out.draft().expect("the command was still a check");
    assert!(
        draft.outcome.is_unverifiable(),
        "an unreadable output produced numbers"
    );
    assert!(draft.outcome.parsed().is_none());
}

#[test]
fn a_nonzero_exit_is_carried_verbatim() {
    let failing = "F                                                     [100%]\n\
                   ===================== 1 failed in 0.02s =====================\n";
    let out = check_from_exec(&argv(&["pytest"]), 1, failing);
    let draft = out.draft().expect("a check");
    assert_eq!(draft.exit_code, 1);
    assert_eq!(draft.outcome.parsed().expect("numbers").failed, 1);
}

#[test]
fn two_runners_sharing_one_output_are_refused_rather_than_attributed() {
    // `pytest -q && cargo test` has one exit code and one output stream. If a
    // passing cargo run could carry a failing pytest run, the product's own
    // central failure mode would be committed by the product.
    let out = check_from_exec(
        &argv(&["sh", "-c", "pytest -q && cargo test"]),
        0,
        PYTEST_OK,
    );
    match out {
        CheckOutcome::NotACheck { reason } => {
            assert!(reason.contains("2 recognised runners"), "{reason}");
            assert!(reason.contains("nested exec events"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unparseable_shell_payload_is_reported_as_unreadable_not_as_no_runner() {
    let out = check_from_exec(&argv(&["sh", "-c", "$RUNNER -q"]), 0, PYTEST_OK);
    match out {
        CheckOutcome::NotACheck { reason } => {
            assert!(reason.contains("could not be parsed"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_echo_that_says_tests_pass_is_not_a_check() {
    // §0 principle 2 in one assertion.
    let out = check_from_exec(
        &argv(&["sh", "-c", "echo 'pytest: all tests pass'"]),
        0,
        "pytest: all tests pass\n",
    );
    assert!(out.draft().is_none(), "an echo became a test check");
}
