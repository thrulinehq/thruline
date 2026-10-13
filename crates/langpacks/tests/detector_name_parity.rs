//! The detector names this crate emits must be **byte-identical** to the ones
//! `crates/verify` emits and dispatches on.
//!
//! # What silently breaks if they are not
//!
//! `verify::tests_pass::escalate` opens with:
//!
//! ```text
//! if finding.detector != "skip_marker_added" {
//!     return finding;
//! }
//! ```
//!
//! That single string comparison is the whole mechanism by which a skip marker
//! becomes a `fail` when the run left tests unrun, and an `info` when it left
//! nothing unrun. A pack that emitted `skip_added`, or `skipMarkerAdded`, or
//! `skip_marker_added ` with a trailing space, would be neither escalated nor
//! de-escalated: a permanent `warn`.
//!
//! A permanent `warn` is the worst of the three outcomes. It never refutes a
//! claim, so a real cheat passes; and it never resolves to `info`, so it
//! accumulates as noise against honest work until a reader learns to skip it.
//! Nothing in the type system prevents it — `Finding::detector` is a `String` —
//! and nothing in a normal test would notice, because the finding is still
//! *produced*, still carries a sensible message, and still appears in the
//! report.
//!
//! So the names are **derived from `verify`'s own source at test time** and
//! compared against every name this crate can emit. A new detector on either
//! side fails this test, which is the point: adding one is a decision someone
//! makes in writing, in both crates at once.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Every detector name `crates/verify` constructs, read out of its source.
///
/// Derived rather than transcribed. A transcribed list is exactly the artefact
/// `scripts/vendor-freshness.py` was written to catch going stale, and it found
/// four of the six facts it guards already wrong on its first run.
fn verify_detector_names() -> BTreeSet<String> {
    let tamper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../verify/src/tamper.rs")
        .canonicalize()
        .expect("crates/verify/src/tamper.rs must exist — this test is about its contents");
    let text = std::fs::read_to_string(&tamper).expect("read verify tamper.rs");

    detector_names_in(&text)
}

/// Every detector-shaped string literal inside a `Finding::new(...)` call.
///
/// **Scanned by paren balance, not by a line window.** A fixed window was the
/// first implementation and it was wrong: verify writes
///
/// ```text
/// Finding::new(
///     // §6.7: an assertion REMOVED is fail; a count merely
///     // decreased is warn. "All of them" is the removal case.
///     if all_gone { Severity::Fail } else { Severity::Warn },
///     if all_gone { "assertion_removed" } else { "assertion_count_decreased" },
/// ```
///
/// — the names sit **twelve** lines below the call, behind a comment, and an
/// eight-line window silently found seven of the ten detectors. Silently: the
/// test still passed its "langpacks emits nothing verify does not know" check,
/// because a SMALLER derived set makes that check *stricter*, not looser. It
/// was the deliberate `EXPECTED_AT_TIME_OF_WRITING` guard that caught it, which
/// is the entire reason that guard exists.
fn detector_names_in(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let hay: Vec<char> = text.chars().collect();
    let mut search = 0usize;
    while let Some(at) = text[search..].find("Finding::new(") {
        let start = search + at + "Finding::new(".len();
        let start_chars = text[..start].chars().count();
        let mut depth = 1i32;
        let mut i = start_chars;
        let mut call = String::new();
        while i < hay.len() && depth > 0 {
            match hay[i] {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                call.push(hay[i]);
            }
            i += 1;
        }
        for line in call.lines() {
            // A `//` comment can contain a quoted phrase; skip it rather than
            // read a detector name out of prose.
            let code = line.split("//").next().unwrap_or("");
            for literal in string_literals(code) {
                if is_detector_shaped(&literal) {
                    names.insert(literal);
                }
            }
        }
        search = start;
    }
    names
}

fn string_literals(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let mut j = i + 1;
            let mut lit = String::new();
            while j < bytes.len() && bytes[j] != '"' {
                if bytes[j] == '\\' {
                    j += 2;
                    continue;
                }
                lit.push(bytes[j]);
                j += 1;
            }
            out.push(lit);
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// A detector name is `snake_case`, one word or several, no spaces, no
/// punctuation. This excludes the message strings that share the same lines.
fn is_detector_shaped(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 40
        && s.contains('_')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
        && !s.starts_with('_')
        && !s.ends_with('_')
}

/// Every detector name `crates/langpacks` constructs, read out of its own
/// source the same way. Both halves are derived so neither can be updated
/// alone.
fn langpack_detector_names() -> BTreeSet<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut names = BTreeSet::new();
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    assert!(
        files.len() >= 5,
        "the source walk found only {} files under {} — the walk is broken, \
         which would make every assertion below vacuous",
        files.len(),
        root.display()
    );
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        names.extend(detector_names_in(&text));
    }
    names
}

fn collect_rs(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The §6.7 detectors, as of the version of `verify` this crate was written
/// against. Present so the derivation itself has something to be checked
/// against — a derivation that returned an empty set would otherwise make
/// every assertion below pass.
const EXPECTED_AT_TIME_OF_WRITING: &[&str] = &[
    "assertion_count_decreased",
    "assertion_removed",
    "assertion_swallowed",
    "assertion_weakened",
    "mocked_unit_under_test",
    "skip_marker_added",
    "sleep_or_retry_added",
    "test_added",
    "test_deleted",
    "test_renamed",
];

#[test]
fn the_derivation_finds_the_detectors_it_was_written_against() {
    let derived = verify_detector_names();
    for expected in EXPECTED_AT_TIME_OF_WRITING {
        assert!(
            derived.contains(*expected),
            "the derivation over verify's tamper.rs did not find `{expected}`. \
             Either verify removed a detector — which this crate must follow — or \
             the derivation is broken, which would make this whole gate vacuous. \
             Found: {derived:?}"
        );
    }
}

#[test]
fn langpacks_emits_no_detector_verify_does_not_know() {
    let verify = verify_detector_names();
    let ours = langpack_detector_names();
    let unknown: Vec<&String> = ours.difference(&verify).collect();
    assert!(
        unknown.is_empty(),
        "these detector names are emitted by thruline-langpacks and are NOT emitted \
         anywhere in crates/verify: {unknown:?}\n\n\
         A name verify does not know is a name `tests_pass::escalate` cannot dispatch \
         on and a name no report of verify's has a row for. If the detector is new and \
         deliberate, it belongs in MERGE-PLAN.md as an addition to verify, not as a \
         string only this crate says.\n\
         verify knows: {verify:?}"
    );
}

#[test]
fn the_escalated_detector_is_spelled_exactly_as_escalate_dispatches_on_it() {
    // The single most consequential string in the crate, checked against the
    // literal in `escalate` itself rather than against a copy of it.
    let tests_pass = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../verify/src/tests_pass.rs")
        .canonicalize()
        .expect("crates/verify/src/tests_pass.rs must exist");
    let text = std::fs::read_to_string(&tests_pass).expect("read tests_pass.rs");

    let escalate = text
        .find("fn escalate(")
        .expect("verify no longer has `escalate` — the escalation this crate relies on is gone");
    let body = &text[escalate..escalate + 400.min(text.len() - escalate)];
    let dispatched: Vec<String> = string_literals(body)
        .into_iter()
        .filter(|s| is_detector_shaped(s))
        .collect();
    assert!(
        dispatched.contains(&"skip_marker_added".to_string()),
        "`escalate` dispatches on {dispatched:?}, not on `skip_marker_added`. \
         Whatever it now compares against is the name this crate must emit, or \
         every skip finding from every language pack is a permanent `warn`: \
         never escalated to `fail` when the run left tests unrun, never lowered \
         to `info` when it did not."
    );

    // **Unconditional.** This read `if !ours.is_empty() { … }`, which made the
    // one assertion that matters skippable: an empty derivation — every source
    // file unreadable, say — took the `else` branch and the test passed having
    // checked nothing about the most consequential string in the crate.
    //
    // `langpack_detector_names` already asserts the walk found at least five
    // files, so an empty set was never likely; "never likely" is not the
    // standard a guard is held to, and the conditional was doing no work that
    // the file-count assert was not already doing better.
    //
    // Found in DECISIONS #177's "a check that passes by not running" census.
    let ours = langpack_detector_names();
    assert!(
        ours.contains("skip_marker_added"),
        "no file under src/ emits `skip_marker_added`, but the engine in \
         src/tamper.rs is supposed to. Found: {ours:?}"
    );
}

/// **Show the harness can reach the change.** Track C's rule.
///
/// This file's green is cited in the merge plan as evidence that the
/// detector-name contract still holds at the core's head. A derivation that has
/// only ever been run against unchanged input has not been shown to notice a
/// change; the two guard tests above prove it finds *something*, which is a
/// weaker claim than that it tracks what it reads.
///
/// The real source is mutated three ways, each corresponding to a change that
/// would actually matter, and the derivation is required to notice each.
#[test]
fn the_derivation_can_reach_a_change() {
    let tamper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../verify/src/tamper.rs")
        .canonicalize()
        .expect("verify's tamper.rs");
    let real = std::fs::read_to_string(&tamper).expect("read");
    let baseline = detector_names_in(&real);
    assert!(
        baseline.contains("skip_marker_added"),
        "the baseline derivation lacks `skip_marker_added`, so the mutations \
         below prove nothing: {baseline:?}"
    );

    // 1. THE detector RENAMED. This is the change that silently turns every
    //    skip finding from every pack into a permanent `warn` — `escalate`
    //    dispatches on the literal string and would stop matching.
    let renamed = real.replace("\"skip_marker_added\"", "\"skip_added\"");
    assert_ne!(renamed, real, "the mutation did not apply");
    let after = detector_names_in(&renamed);
    assert!(
        after.contains("skip_added") && !after.contains("skip_marker_added"),
        "verify's most consequential detector name was changed and the \
         derivation reported the old one. This file's green would then be \
         asserting parity against a name verify no longer emits: {after:?}"
    );

    // 2. A detector REMOVED. The parity check compares this crate's names
    //    against verify's, so a shrunken verify set must show up — otherwise
    //    this crate could go on emitting a name verify has dropped.
    let removed = real.replace("\"sleep_or_retry_added\"", "\"__gone__\"");
    assert_ne!(removed, real, "the mutation did not apply");
    let after = detector_names_in(&removed);
    assert!(
        !after.contains("sleep_or_retry_added"),
        "a detector was removed from verify and the derivation still reports \
         it: {after:?}"
    );

    // 3. A detector ADDED. A new §6.7 finding in verify is a new row this crate
    //    may need to emit; a derivation blind to additions would let the two
    //    drift apart in the direction of missing coverage.
    let added = real.replace(
        "Finding::new(\n                    Severity::Warn,\n                    \"skip_marker_added\"",
        "Finding::new(\n                    Severity::Warn,\n                    \"assertion_reordered\"",
    );
    if added != real {
        let after = detector_names_in(&added);
        assert!(
            after.contains("assertion_reordered"),
            "a detector was added to verify and the derivation did not see it: \
             {after:?}"
        );
    } else {
        // The exact call shape moved. Fall back to a synthetic call, which
        // still exercises the scanner rather than skipping silently.
        let synthetic = format!(
            "{real}\nfn x() {{ Finding::new(Severity::Warn, \"assertion_reordered\", \"d\"); }}\n"
        );
        let after = detector_names_in(&synthetic);
        assert!(
            after.contains("assertion_reordered"),
            "the scanner missed a `Finding::new` call appended verbatim, so it \
             is not finding calls generally: {after:?}"
        );
    }
}
