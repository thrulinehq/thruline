//! What `verify::parse::parse_go_test` actually does when given bytes a real
//! `go test` produced.
//!
//! # Why this file exists
//!
//! `crates/verify/tests/remaining_parsers.rs` says, in its own words:
//!
//! > *"**`go test` is still a documented shape** — there is no Go toolchain
//! > here. It is named as such rather than presented as a capture."*
//!
//! That was an honest gap, honestly named. It is also the exact risk the
//! `pytest -q` story in `tests/fixtures/runners/README.md` is about: a parser
//! proven against a hand-written fixture carrying a banner the tool does not
//! emit, whose first real run came back `unverifiable`.
//!
//! Go 1.27.1 is installed on this machine now, and this crate captured real
//! output. So this file closes the loop the only way it can be closed: it runs
//! **verify's own parser** over **real captured bytes** and records what
//! happens.
//!
//! # This is a characterisation test, not a wish
//!
//! Each assertion states what verify's parser *does today*. Where the answer is
//! wrong, the assertion still encodes the real answer and the comment says so
//! and points at the merge plan — `crates/verify` is read-only to this crate
//! (Rule 3), and a defect recorded in a passing test that names it is louder
//! than a failing test someone learns to ignore.

use std::path::PathBuf;

use thruline_verify::parse::{parse, Runner};

fn captured(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runners")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn verify_counts(name: &str) -> Option<(u64, u64, u64, u64)> {
    parse(Some(Runner::GoTest), &captured(name))
        .parsed()
        .map(|p| (p.tests_total, p.passed, p.failed, p.skipped))
}

#[test]
fn the_fixtures_this_test_reads_are_real_captures() {
    for name in [
        "gotest-all-pass.txt",
        "gotest-mixed.txt",
        "gotest-json.txt",
        "gotest-build-failed.txt",
        "gotest-no-test-files.txt",
        "gotest-skipped.txt",
    ] {
        let text = captured(name);
        assert!(
            text.len() > 20,
            "{name} is {} bytes — that is not a capture",
            text.len()
        );
    }
    let json = captured("gotest-json.txt");
    assert!(
        json.contains("\"Time\":\"20") && json.contains("\"Action\":"),
        "gotest-json.txt does not look like real `go test -json` output"
    );
}

#[test]
fn verify_reads_a_real_passing_run() {
    let counts = verify_counts("gotest-all-pass.txt");
    assert!(
        counts.is_some(),
        "verify's parser returned Unverifiable for a real, ordinary, all-passing \
         `go test` run — the `pytest -q` failure repeating in another language."
    );
    let (total, passed, failed, _) = counts.expect("counts");
    assert_eq!(failed, 0, "a passing run reported failures");
    assert!(
        total > 0 && passed > 0,
        "a real passing run parsed to total={total} passed={passed} — zero counts \
         from a run that passed tests read downstream as a clean run over nothing"
    );
}

#[test]
fn verify_reads_a_real_mixed_run() {
    let Some((total, passed, failed, skipped)) = verify_counts("gotest-mixed.txt") else {
        panic!("verify's parser returned Unverifiable for a real mixed `go test` run");
    };
    assert!(
        failed > 0,
        "a real run with failing tests parsed to failed=0. This is the one \
         direction that must never happen: it turns a red suite into evidence \
         that supports a claim."
    );
    // `total = passed + failed + skipped`, not `passed + failed`. The real
    // capture is 2 pass, 2 fail, 1 skip; asserting the two-term form here was
    // this test's own bug and it read as a defect in verify's parser until the
    // fixture was looked at. Written down because "the test was wrong" is the
    // commonest way a parser gets blamed for output it read correctly.
    assert_eq!(
        (total, passed, failed, skipped),
        (5, 2, 2, 1),
        "the real capture is 2 pass, 2 fail, 1 skip"
    );
    assert_eq!(
        total,
        passed + failed + skipped,
        "counts do not reconcile for a real mixed run"
    );
}

#[test]
fn a_real_build_failure_never_reads_as_a_clean_run() {
    for name in [
        "gotest-build-failed.txt",
        "gotest-vet-failed.txt",
        "gotest-setup-failed.txt",
    ] {
        if let Some(p) = parse(Some(Runner::GoTest), &captured(name)).parsed() {
            assert_eq!(
                p.passed, 0,
                "{name}: a run that produced no test results reported {} passing tests",
                p.passed
            );
            assert!(
                p.failed > 0 || (p.tests_total == 0 && p.passed == 0),
                "{name}: parsed to total={} passed={} failed={} — a caller reading \
                 `failed == 0` as success would treat a build failure as a green suite.",
                p.tests_total,
                p.passed,
                p.failed
            );
        }
    }
}

#[test]
fn a_real_zero_test_run_is_distinguishable_from_a_real_passing_run() {
    let none = parse(Some(Runner::GoTest), &captured("gotest-no-test-files.txt"));
    let pass = parse(Some(Runner::GoTest), &captured("gotest-all-pass.txt"));
    assert_ne!(
        none.parsed().map(|p| (p.tests_total, p.passed)),
        pass.parsed().map(|p| (p.tests_total, p.passed)),
        "`go test` on a package with no test files and a real passing run parse to \
         the SAME numbers. A claim that 'the tests pass' would be supported by a \
         run that executed nothing."
    );
}

#[test]
fn real_skips_are_counted_as_skips_and_not_as_passes() {
    let Some((total, passed, _failed, skipped)) = verify_counts("gotest-skipped.txt") else {
        panic!("verify's parser returned Unverifiable for a real run containing skips");
    };
    assert!(
        skipped > 0,
        "a real run that skipped tests parsed to skipped=0 (total={total} \
         passed={passed}). `tests_pass::escalate` decides whether a \
         `skip_marker_added` finding becomes a `fail` by reading exactly this \
         number — a skip counted as a pass silently disarms the skip detector \
         for every Go claim."
    );
}

#[test]
fn package_level_json_events_are_not_counted_as_tests() {
    let Some((total, passed, failed, skipped)) = verify_counts("gotest-json.txt") else {
        panic!("verify's parser returned Unverifiable for real `go test -json` output");
    };
    let text = captured("gotest-json.txt");
    let mut names = std::collections::BTreeSet::new();
    for line in text.lines() {
        if let Some(at) = line.find("\"Test\":\"") {
            let rest = &line[at + 8..];
            if let Some(end) = rest.find('"') {
                names.insert(rest[..end].to_string());
            }
        }
    }
    assert!(
        !names.is_empty(),
        "the fixture names no tests — it is not real `-json` output"
    );
    assert_eq!(
        total,
        names.len() as u64,
        "verify counted {total} tests where the real output names {} distinct \
         tests ({names:?}). A package-level `pass` counted as a test inflates \
         the denominator of every Go claim.",
        names.len()
    );
    assert_eq!(
        total,
        passed + failed + skipped,
        "counts do not reconcile for real `-json` output"
    );
}
