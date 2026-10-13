//! Property tests for the §6.2 parsers (§16 requires proptest for parsers).
//!
//! The property that matters is not "parsing round-trips". It is:
//!
//! > **No input can make a parser report a clean run that did not happen.**
//!
//! Everything else a parser gets wrong is recoverable. That one is the product
//! committing the failure it exists to detect, so it is the one generated
//! adversarially.

use proptest::prelude::*;
use thruline_verify::{parse, Runner, RunnerOutcome};

/// Text that looks like runner output without being any particular runner's.
fn noise() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 :;.=_/-]{0,200}",
        // Fragments a naive parser might latch onto.
        Just("test result:".to_string()),
        Just("passed".to_string()),
        Just("0 passed".to_string()),
        Just("=== ===".to_string()),
        Just("running 0 tests".to_string()),
        Just("in 1.0s".to_string()),
        any::<String>(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4_000))]

    /// **The property.** Whatever comes out, it is never a passing run the input
    /// did not report.
    #[test]
    fn no_input_is_ever_read_as_a_clean_run_it_did_not_report(
        text in noise(),
        pytest in any::<bool>(),
    ) {
        let runner = if pytest { Runner::Pytest } else { Runner::CargoTest };
        match parse(Some(runner), &text) {
            RunnerOutcome::Unverifiable { reason, hint } => {
                // Both halves: a reason a human can act on, and a hint saying
                // how to fix it. §6.2 requires the hint.
                prop_assert!(!reason.is_empty());
                prop_assert!(!hint.is_empty());
            }
            RunnerOutcome::Parsed(p) => {
                // A run of zero tests is a legitimate result — an empty test
                // binary really does print `0 passed; 0 failed` — but only when
                // the text carried counts. §6.3 is what refuses to call it a
                // pass; the parser's job is to report what was said.
                prop_assert!(
                    text.contains("passed") || text.contains("failed")
                        || text.contains("ignored") || text.contains("error")
                        || text.contains("skipped") || text.contains("xfail")
                        || text.contains("no tests ran"),
                    "parsed a result from text carrying no outcome word: {:?}", text
                );
                prop_assert_eq!(p.tests_total, p.passed + p.failed + p.skipped + p.xfail);
                if p.failed == 0 && p.passed > 0 {
                    prop_assert!(
                        text.contains("passed") || text.contains("ok"),
                        "reported {} passing tests from text saying neither \
                         `passed` nor `ok`: {:?}", p.passed, text
                    );
                }
            }
        }
    }

    /// Noise before a real failing summary must not dilute it.
    #[test]
    fn a_cargo_failure_summary_survives_any_preceding_noise(prefix in noise()) {
        let text = format!(
            "{prefix}\ntest result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; \
             0 filtered out; finished in 0.01s\n"
        );
        let p = parse(Some(Runner::CargoTest), &text)
            .parsed().cloned().expect("a real summary must parse");
        prop_assert!(p.failed >= 2, "failures were lost behind noise: {:?}", p);
    }

    #[test]
    fn a_pytest_failure_summary_survives_any_preceding_noise(prefix in noise()) {
        let text = format!("{prefix}\n===== 2 failed, 1 passed in 0.10s =====\n");
        let p = parse(Some(Runner::Pytest), &text)
            .parsed().cloned().expect("a real summary must parse");
        prop_assert!(p.failed >= 2, "failures were lost behind noise: {:?}", p);
    }

    /// A failing binary anywhere in a workspace run must survive, wherever it
    /// sits among passing ones. This is the "read only the last summary" bug,
    /// generated rather than exemplified.
    #[test]
    fn a_failing_binary_is_never_hidden_by_passing_ones(
        before in 0usize..6,
        after in 0usize..6,
    ) {
        let ok = "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; \
                  0 filtered out; finished in 0.01s\n";
        let bad = "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; \
                   0 filtered out; finished in 0.01s\n";
        let text = format!("{}{bad}{}", ok.repeat(before), ok.repeat(after));
        let p = parse(Some(Runner::CargoTest), &text)
            .parsed().cloned().expect("parses");
        prop_assert_eq!(p.failed, 1, "the failing binary was lost among {} passing ones",
                        before + after);
        prop_assert_eq!(p.passed, 3 * (before + after) as u64);
    }

    /// §13's obfuscated-command defence: recognition is over argv, so a command
    /// that merely mentions a runner is not one.
    ///
    /// # The `prop_assume!` is the property, not a workaround
    ///
    /// `before` is the PROGRAM. `[a-z]{1,8}` can draw `jest`, `go`, `npm`,
    /// `mvn`, `tox` — real runner names — and `from_argv(["jest","pytest"])`
    /// then returns `Some(Jest)` **correctly**, because `jest` is in the
    /// program position and is a runner. The unguarded assertion therefore
    /// asserted the opposite of the parser's contract on those draws, and was
    /// green only because the generator had not yet drawn one: it survived
    /// 3274 draws and failed on 3275 during an unrelated full-workspace run,
    /// which is a coin flip wearing a test's name. The seed is pinned in
    /// `parser_properties.proptest-regressions`, so this replays.
    ///
    /// Stating "the program is not itself a runner" is what the sentence above
    /// always meant. Filtering here rather than narrowing the regex keeps the
    /// guard honest as the runner table grows — a new runner name added to
    /// `from_argv` is excluded automatically instead of silently re-arming
    /// this.
    #[test]
    fn mentioning_a_runner_does_not_make_a_command_one(
        word in "(pytest|py\\.test|cargo)",
        before in "[a-z]{1,8}",
    ) {
        prop_assume!(Runner::from_argv(std::slice::from_ref(&before)).is_none());
        // …and that `before` is not a LAUNCHER. `uvx pytest` really is a pytest
        // invocation — `uvx` is `uv tool run` — so `Some(Pytest)` there is the
        // parser being right and this property being too strong. Found the same
        // way the last one was: a draw the generator had not made yet
        // (`before = "uvx"`, after `before = "jest"`), which is why the file's
        // own docs already call this shape "a coin flip wearing a test's name".
        //
        // PROBED, not listed. A launcher is one that forwards, so ask the parser
        // with a runner unrelated to the drawn `word` — a launcher added to
        // `from_argv` later is excluded automatically, which is the same reason
        // the assumption above filters rather than narrowing the regex.
        prop_assume!(
            Runner::from_argv(&[before.clone(), "mocha".to_string()]).is_none()
        );
        let argv = vec![before, word];
        prop_assert_eq!(
            Runner::from_argv(&argv), None,
            "{:?} was recognised as a runner from a non-program position", argv
        );
    }
}
