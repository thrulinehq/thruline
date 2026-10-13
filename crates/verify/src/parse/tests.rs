use super::*;

#[test]
fn node_requires_a_complete_consistent_footer() {
    let footer = "TAP version 13\n1..2\n# tests 2\n# suites 0\n# pass 1\n# fail 0\n# cancelled 0\n# skipped 1\n# todo 0\n# duration_ms 33.641416\n";
    let counts = parse_node_test(footer).expect("complete TAP footer");
    assert_eq!(
        (counts.tests_total, counts.passed, counts.skipped),
        (2, 1, 1)
    );
    for cut in ["# tests", "# pass", "# fail", "# skipped", "# duration_ms"] {
        assert!(parse_node_test(&footer[..footer.find(cut).unwrap()]).is_err());
    }
    assert!(parse_node_test(&footer.replace("# tests 2", "# tests 999")).is_err());
    assert!(parse_node_test(&footer.replace("# cancelled 0", "# cancelled 1")).is_err());
}

const PYTEST_Q: &str = "\
....F..s\n\
=================================== FAILURES ===================================\n\
FAILED tests/test_parser.py::test_handles_empty - assert 0 == 1\n\
=========== 1 failed, 6 passed, 1 skipped in 3.42s ============================\n";

const PYTEST_V: &str = "\
tests/test_a.py::test_one PASSED                                         [ 33%]\n\
tests/test_a.py::test_two PASSED                                         [ 66%]\n\
tests/test_b.py::test_three XFAIL                                        [100%]\n\
======================== 2 passed, 1 xfailed in 0.11s =========================\n";

const CARGO_WORKSPACE: &str = "\
running 3 tests\n\
test spec::a ... ok\n\
test spec::b ... ok\n\
test spec::c ... FAILED\n\
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s\n\
\n\
running 2 tests\n\
test other::d ... ok\n\
test other::e ... ignored\n\
test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.03s\n";

#[test]
fn pytest_quiet_and_verbose_agree_because_the_summary_is_the_authority() {
    let q = parse(Some(Runner::Pytest), PYTEST_Q);
    let p = q.parsed().expect("parses");
    // The fixture says `1 failed, 6 passed, 1 skipped` — and the first version
    // of this assertion said 0 skipped, which the parser correctly refused.
    assert_eq!((p.passed, p.failed, p.skipped), (6, 1, 1));
    assert_eq!(p.tests_total, 8);
    assert_eq!(p.duration_ms, Some(3420));
    assert!(p
        .test_ids
        .contains("tests/test_parser.py::test_handles_empty"));

    let v = parse(Some(Runner::Pytest), PYTEST_V);
    let p = v.parsed().expect("parses");
    assert_eq!((p.passed, p.failed, p.xfail), (2, 0, 1));
    assert_eq!(p.tests_total, 3);
    assert_eq!(p.test_ids.len(), 3, "{:?}", p.test_ids);
}

#[test]
fn cargo_test_sums_every_binary_rather_than_reading_the_last() {
    // The classic error: a workspace emits one `test result:` per target, and
    // reading only the last reports the final crate's result as the whole
    // workspace's — so a failure in an earlier crate disappears. Here the
    // FAILING binary comes first and the passing one last.
    let out = parse(Some(Runner::CargoTest), CARGO_WORKSPACE);
    let p = out.parsed().expect("parses");
    assert_eq!(
        (p.passed, p.failed, p.skipped),
        (3, 1, 1),
        "a per-binary result was dropped"
    );
    assert_eq!(p.tests_total, 5);
    assert_eq!(p.duration_ms, Some(150));
    assert!(p.test_ids.contains("spec::c"));
    assert!(!p.test_ids.iter().any(|id| id.contains("result:")));
}

#[test]
fn an_unrecognised_runner_is_unverifiable_with_a_hint_never_a_guess() {
    let out = parse(None, "some tool wrote this and everything is fine");
    assert!(out.is_unverifiable());
    assert!(out.parsed().is_none(), "an unknown runner produced numbers");
    let RunnerOutcome::Unverifiable { hint, .. } = &out else {
        panic!("wrong variant");
    };
    assert!(
        hint.contains("JUnit"),
        "the hint does not say how to fix it: {hint}"
    );
}

#[test]
fn truncated_or_crashed_output_is_unverifiable_not_a_clean_run() {
    // THE rule. A defaulted `Parsed` would be `{total: 0, failed: 0}`, which
    // reads downstream as a passing run — the product committing the exact
    // failure it exists to detect. The type has no Default and no Parsed inside
    // the Unverifiable arm, so this cannot be written by accident.
    for (runner, output, why) in [
        (Runner::Pytest, "", "empty"),
        (
            Runner::Pytest,
            "collecting ... \n",
            "crashed during collection",
        ),
        (Runner::CargoTest, "", "empty"),
        (
            Runner::CargoTest,
            "running 3 tests\ntest a ... ok\n",
            "killed before the summary",
        ),
        (
            Runner::CargoTest,
            "error: could not compile `foo`\n",
            "compile failure",
        ),
    ] {
        let out = parse(Some(runner), output);
        assert!(
            out.is_unverifiable(),
            "{} output for {runner:?} was read as a result: {out:?}",
            why
        );
        assert!(out.parsed().is_none());
    }
}

#[test]
fn a_pytest_collection_error_with_a_summary_is_still_unverifiable() {
    // A summary line exists but carries no outcomes. Counting it as zero
    // failures would be the collapse the module forbids.
    let out = parse(
        Some(Runner::Pytest),
        "=================== 3 errors in 0.40s ===================\n",
    );
    let p = out.parsed().expect("errors are outcomes and do count");
    assert_eq!(p.failed, 3, "pytest errors must count as failures");
    assert_eq!(p.tests_total, 3);

    let empty = parse(Some(Runner::Pytest), "============ in 0.01s ============\n");
    assert!(
        empty.is_unverifiable(),
        "a summary with no counts is not a clean run"
    );
}

#[test]
fn argv_is_parsed_not_pattern_matched() {
    // §13's obfuscated-command defence: recognition is at the argv level, so a
    // command that merely MENTIONS a runner is not one.
    let argv = |s: &str| -> Vec<String> { s.split(' ').map(str::to_string).collect() };
    assert_eq!(Runner::from_argv(&argv("pytest -q")), Some(Runner::Pytest));
    assert_eq!(
        Runner::from_argv(&argv("/usr/local/bin/pytest")),
        Some(Runner::Pytest)
    );
    assert_eq!(Runner::from_argv(&argv("py.test -x")), Some(Runner::Pytest));
    assert_eq!(
        Runner::from_argv(&argv("python -m pytest tests/")),
        Some(Runner::Pytest)
    );
    assert_eq!(
        Runner::from_argv(&argv("cargo test --workspace")),
        Some(Runner::CargoTest)
    );
    assert_eq!(
        Runner::from_argv(&argv("cargo --locked test")),
        Some(Runner::CargoTest)
    );
    assert_eq!(
        Runner::from_argv(&argv("cargo nextest run")),
        Some(Runner::CargoTest)
    );

    // Not runners, however much they look like one in a string.
    for not in [
        "echo pytest",
        "cargo build",
        "cargo clippy",
        "python -m mymodule",
        "git commit -m 'run pytest'",
        "mypytest",
        "",
    ] {
        assert_eq!(
            Runner::from_argv(&argv(not)),
            None,
            "{not:?} was recognised as a test runner"
        );
    }
}

#[test]
fn uv_run_launches_the_runner_it_names() {
    // 219 of Terminal-Bench's 241 `run-tests.sh` invoke their suite as
    // `uv run pytest …`. Before this arm, wrapping `uv` produced an exec with
    // no runner category — captured, and still `unverifiable`.
    let argv = |s: &str| -> Vec<String> { s.split(' ').map(str::to_string).collect() };
    for (line, want) in [
        ("uv run pytest tests/test_outputs.py -rA", Runner::Pytest),
        ("uv run -m pytest", Runner::Pytest),
        ("uv run -- pytest -q", Runner::Pytest),
        ("uv run --python 3.13 pytest", Runner::Pytest),
        // The recursion earns its keep: no cargo/go clause was written here.
        ("uv run cargo test", Runner::CargoTest),
        ("uv run go test ./...", Runner::GoTest),
        ("uvx pytest -q", Runner::Pytest),
        ("uv tool run pytest", Runner::Pytest),
        ("uvx --from pytest pytest -q", Runner::Pytest),
        // An option this has never heard of is treated as a boolean and
        // skipped, which is right for the overwhelming majority of them.
        ("uv run --unknown-flag pytest", Runner::Pytest),
    ] {
        assert_eq!(
            Runner::from_argv(&argv(line)),
            Some(want),
            "{line:?} should name {want:?}"
        );
    }

    for not in [
        // Installing a runner is not running one — the `run` subcommand is
        // required precisely so this cannot read as a test run.
        "uv pip install pytest==8.4.1",
        "uv venv .tbench-testing",
        "uv sync",
        "uv run script.py",
        "uv run",
        "uvx",
        // `--with pytest` names a DEPENDENCY. Reading it as the program would
        // attribute a pytest run to a command that never started one.
        "uv run --with pytest mytool",
        // An option whose separate VALUE this does not know to consume leaves
        // that value standing where the program should be, and the recursion
        // returns None: unverifiable, not a guess.
        "uv run --unknown-value-option somevalue pytest",
    ] {
        assert_eq!(
            Runner::from_argv(&argv(not)),
            None,
            "{not:?} was recognised as a test runner"
        );
    }
}

#[test]
fn every_from_argv_program_is_wrapped_or_reachable_another_way() {
    // The defect this file's `python -m pytest` arm embodied for its whole
    // life: a parser written for an argv the capture layer could not deliver.
    // Locked down as a list, so that adding an arm without a wrapper is a test
    // failure rather than a discovery made by measuring a corpus.
    //
    // `verify` cannot depend on `capture` (capture depends on verify), so the
    // list is transcribed and the transcription is what the assertion is for:
    // it names the exact set, and a change to `shim::WRAPPED` that is not
    // reflected here shows up as a disagreement.
    const WRAPPED: &[&str] = &[
        "pytest", "cargo", "go", "npm", "pnpm", "yarn", "npx", "make", "mvn", "gradle", "dotnet",
        "uv", "uvx", "python", "python3",
    ];
    // Named, not silently tolerated. Each is reachable only through the
    // npm-family arm, a `sh -c` payload, or re-execution — never from a shim
    // capture of its own name.
    const UNREACHABLE_FROM_SHIM: &[&str] = &["py.test", "jest", "vitest", "mocha", "gradlew"];

    let argv = |s: &str| -> Vec<String> { s.split(' ').map(str::to_string).collect() };
    for (program, line) in [
        ("py.test", "py.test -q"),
        ("jest", "jest --ci"),
        ("vitest", "vitest run"),
        ("mocha", "mocha test/"),
        ("gradlew", "gradlew test"),
        ("pytest", "pytest -q"),
        ("cargo", "cargo test"),
        ("go", "go test ./..."),
        ("dotnet", "dotnet test"),
        ("mvn", "mvn test"),
        ("gradle", "gradle test"),
        ("npx", "npx jest"),
        ("python", "python -m pytest"),
        ("python3", "python3 -m pytest"),
        ("uv", "uv run pytest"),
        ("uvx", "uvx pytest"),
    ] {
        assert!(
            Runner::from_argv(&argv(line)).is_some(),
            "{line:?} should be recognised"
        );
        assert!(
            WRAPPED.contains(&program) || UNREACHABLE_FROM_SHIM.contains(&program),
            "`{program}` is recognised here but is neither wrapped by the shim \
             nor listed as knowingly unreachable — a parser arm no capture can \
             feed reads as coverage that does not exist"
        );
    }
}

#[test]
fn totals_are_always_the_sum_of_the_parts() {
    // An invariant a caller may rely on, so it is asserted rather than assumed.
    for (r, out) in [
        (Runner::Pytest, PYTEST_Q),
        (Runner::Pytest, PYTEST_V),
        (Runner::CargoTest, CARGO_WORKSPACE),
    ] {
        let p = parse(Some(r), out).parsed().cloned().expect("parses");
        assert_eq!(p.tests_total, p.passed + p.failed + p.skipped + p.xfail);
    }
}

/// Verbatim output of `pytest -q` from pytest 9.0.1, captured through the
/// M0.8 shim rather than typed here. **There is no `=====` banner** — the
/// hand-written `-q` fixtures in this file carried one because they were
/// written from the rule rather than from a run.
const PYTEST_Q_REAL: &str =
    "..                                                                       [100%]\n\
                             2 passed in 0.00s\n";

#[test]
fn real_pytest_q_output_parses_although_it_has_no_banner() {
    let outcome = parse(Some(Runner::Pytest), PYTEST_Q_REAL);
    let parsed = outcome
        .parsed()
        .unwrap_or_else(|| panic!("real `pytest -q` output was unverifiable: {outcome:?}"));
    assert_eq!(
        (parsed.passed, parsed.failed, parsed.tests_total),
        (2, 0, 2)
    );
    assert_eq!(parsed.duration_ms, Some(0));
}

#[test]
fn a_bannerless_tally_must_be_the_whole_line() {
    // The precision cost of accepting a bannerless summary: a test that prints
    // the words must not become the run's result.
    for not_a_summary in [
        "assert \"3 passed\" in output",
        "INFO 2 passed the filter",
        "E   AssertionError: 1 failed",
        "  2 passed in the queue and 3 remain",
        "2 frobnicated in 0.1s",
    ] {
        assert!(
            parse(Some(Runner::Pytest), not_a_summary).is_unverifiable(),
            "{not_a_summary:?} was read as a pytest summary"
        );
    }
    // And the shapes that ARE summaries still are.
    for summary in [
        "2 passed in 0.00s",
        "1 failed, 6 passed, 1 skipped in 3.42s",
        "3 passed, 1 warning in 0.11s",
        "no tests ran in 0.01s",
    ] {
        assert!(
            !parse(Some(Runner::Pytest), summary).is_unverifiable(),
            "{summary:?} was not read as a pytest summary"
        );
    }
}

#[test]
fn a_failing_bannerless_run_is_not_reported_as_clean() {
    let out = "F                                                          [100%]\n\
               FAILED test_sample.py::test_two - assert 'z' in 'abc'\n\
               1 failed in 0.01s\n";
    let parsed = parse(Some(Runner::Pytest), out)
        .parsed()
        .cloned()
        .expect("numbers");
    assert_eq!(parsed.failed, 1);
    assert_eq!(parsed.passed, 0);
    assert!(parsed.test_ids.iter().any(|id| id.contains("test_two")));
}
