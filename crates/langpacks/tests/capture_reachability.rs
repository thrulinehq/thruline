//! Which of this crate's runners can actually be reached from real capture.
//!
//! # Why this is a test and not a comment
//!
//! `Runner::from_argv`'s doc comment in `crates/verify` states the rule:
//!
//! > *"an arm whose program is not in that list is unreachable from real
//! > capture however correct it is … Adding an arm and adding the wrapper are
//! > one change. Half of it is worth nothing, and reads as coverage that does
//! > not exist."*
//!
//! This crate ships parsers for seven runners. Four of the programs behind them
//! are wrapped; **`rspec`, `ruby`, `rake` and `bundle` are not**, so every Ruby
//! parser here is correct and unreachable. That is a fact about a file in
//! another crate, which means a transcribed copy of it goes stale silently —
//! the precise failure `scripts/vendor-freshness.py` exists to stop, and which
//! it found four of six times on its first run.
//!
//! So the list is **re-derived from `crates/capture/src/shim.rs` at test time**
//! and compared against `PackRunner::reachable_from_capture`. If someone adds a
//! `ruby` wrapper, this test fails and tells them to flip the flag; if someone
//! removes `mvn`, it fails and tells them Java went dark. Either way the claim
//! and the reality cannot drift apart in silence.

use std::path::PathBuf;

use thruline_langpacks::runners::PackRunner;

/// The `WRAPPED` list, read out of `crates/capture/src/shim.rs`.
///
/// Parsed rather than imported: `thruline-langpacks` deliberately does not
/// depend on `thruline-capture` (nothing here captures anything), and adding a
/// whole crate dependency to read one constant would be the larger change.
fn shim_source() -> String {
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../capture/src/shim.rs")
        .canonicalize()
        .expect("crates/capture/src/shim.rs must exist — this test is about its contents");
    std::fs::read_to_string(&shim).expect("read shim.rs")
}

fn wrapped_programs() -> Vec<String> {
    parse_wrapped(&shim_source())
}

/// The parse, over text rather than over a path.
///
/// Split out so `the_derivation_can_reach_a_change` can feed it a MUTATED copy
/// of the real source. A derivation that is only ever run against unchanged
/// input has never been shown to notice a change — which is Track C's §4c rule,
/// and it applies to this file more than most, because the merge plan
/// cites this test's green as evidence that the contract still holds.
fn parse_wrapped(text: &str) -> Vec<String> {
    let start = text
        .find("pub const WRAPPED: &[&str] = &[")
        .expect("shim.rs no longer declares `pub const WRAPPED` — the derivation this test performs is broken, which is a failure to report, not a reason to skip");
    let rest = &text[start..];
    // The opening bracket of the ARRAY, not the one in the type `&[&str]`.
    // Searching for a bare `[` found the type's bracket and swept the whole
    // declaration into the first element — the guard test below is what caught
    // it, which is why that guard is not optional.
    const OPEN: &str = "= &[";
    let open = rest.find(OPEN).expect("WRAPPED's array literal") + OPEN.len();
    let close = rest.find("];").expect("WRAPPED's closing bracket");
    assert!(close > open, "WRAPPED's brackets are inverted");

    rest[open..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The program each runner is invoked as, for the reachability question.
fn program_for(runner: PackRunner) -> &'static str {
    match runner {
        PackRunner::GoTest => "go",
        PackRunner::Maven => "mvn",
        PackRunner::Gradle => "gradle",
        PackRunner::DotnetTest => "dotnet",
        PackRunner::RSpec => "rspec",
        PackRunner::Minitest => "ruby",
        // Read from a report file on disk, not from an exec of its own. It is
        // reachable whenever the runner that WROTE it was.
        PackRunner::JUnitXml => "mvn",
    }
}

const ALL: &[PackRunner] = &[
    PackRunner::GoTest,
    PackRunner::Maven,
    PackRunner::Gradle,
    PackRunner::DotnetTest,
    PackRunner::JUnitXml,
    PackRunner::RSpec,
    PackRunner::Minitest,
];

#[test]
fn the_derivation_finds_a_plausible_wrapped_list() {
    // A guard on the guard: a parse that silently returned nothing would make
    // every assertion below vacuously "unreachable", which is the direction
    // that reports a gap that does not exist.
    let wrapped = wrapped_programs();
    assert!(
        wrapped.len() >= 10,
        "the WRAPPED derivation returned {} entries, which means the parse broke \
         rather than that the list shrank: {wrapped:?}",
        wrapped.len()
    );
    for expected in ["pytest", "cargo", "go", "mvn", "gradle", "dotnet"] {
        assert!(
            wrapped.iter().any(|w| w == expected),
            "`{expected}` is missing from the derived WRAPPED list {wrapped:?} — \
             either the parse is broken or the capture layer changed"
        );
    }
}

#[test]
fn every_runners_reachability_flag_matches_the_capture_layer() {
    let wrapped = wrapped_programs();
    for &runner in ALL {
        let program = program_for(runner);
        let actually_wrapped = wrapped.iter().any(|w| w == program);
        assert_eq!(
            runner.reachable_from_capture(),
            actually_wrapped,
            "`{}` is invoked as `{program}`, which is {} in \
             `thruline_capture::shim::WRAPPED`, but `reachable_from_capture()` says {}. \
             Adding a parser arm and adding the wrapper are ONE change; flip the flag \
             and say so in MERGE-PLAN.md rather than leaving the two disagreeing.",
            runner.as_str(),
            if actually_wrapped {
                "present"
            } else {
                "absent"
            },
            runner.reachable_from_capture(),
        );
    }
}

#[test]
fn ruby_is_named_as_unreachable_rather_than_quietly_missing() {
    // The whole point. A Ruby parser that cannot be reached is a real thing to
    // ship — but only if the fact travels with it.
    for runner in [PackRunner::RSpec, PackRunner::Minitest] {
        assert!(
            !runner.reachable_from_capture(),
            "`{}` claims to be reachable from capture. If a `ruby`/`rspec` wrapper \
             really landed, this test's sibling above will already have failed and \
             told you to flip the flag.",
            runner.as_str()
        );
        assert_eq!(
            runner.into_verify(),
            None,
            "`{}` was mapped onto a `verify::parse::Runner` variant. There is no \
             correct one: naming an RSpec run `junit-xml` is a lie in a field a \
             report prints.",
            runner.as_str()
        );
    }
}

#[test]
fn the_reachable_runners_map_onto_a_verify_runner_without_guessing() {
    for &runner in ALL {
        if runner.reachable_from_capture() {
            let mapped = runner.into_verify();
            assert!(
                mapped.is_some(),
                "`{}` is reachable from capture but has no `verify::parse::Runner` \
                 to be reported as — a result with nowhere to go",
                runner.as_str()
            );
        }
    }
}

/// **Show the harness can reach the change.** Track C's rule:
/// *before citing any green against a changed contract, show the harness can
/// reach the change.*
///
/// The merge plan cites this file's green as evidence that the contract
/// still holds at the core's head. That citation is only worth something if the
/// derivation would go red when the contract moves — and until this test
/// existed, nothing had ever shown it would. A parse that silently returned the
/// same answer for changed input would have looked identical.
///
/// So the real source is mutated three ways and the derivation is required to
/// notice each.
#[test]
fn the_derivation_can_reach_a_change() {
    let real = shim_source();
    let baseline = parse_wrapped(&real);
    assert!(
        baseline.iter().any(|w| w == "go"),
        "the baseline derivation does not contain `go`, so the mutations below \
         prove nothing: {baseline:?}"
    );

    // 1. A program REMOVED from WRAPPED. This is the change that would silently
    //    make a language unreachable from capture.
    let removed = real.replacen("\"go\", ", "", 1);
    assert_ne!(removed, real, "the mutation did not apply");
    let after = parse_wrapped(&removed);
    assert!(
        !after.iter().any(|w| w == "go"),
        "`go` was removed from WRAPPED and the derivation still reports it — \
         this file's green means nothing: {after:?}"
    );

    // 2. A program ADDED. `ruby` landing here is precisely the change R4 asks
    //    for, and the one that would flip PackRunner::reachable_from_capture.
    let added = real.replacen("\"pytest\",", "\"pytest\", \"ruby\",", 1);
    assert_ne!(added, real, "the mutation did not apply");
    let after = parse_wrapped(&added);
    assert!(
        after.iter().any(|w| w == "ruby"),
        "`ruby` was added to WRAPPED and the derivation did not see it, so this \
         crate would go on reporting Ruby unreachable after the fix landed: \
         {after:?}"
    );

    // 3. The declaration RENAMED. A derivation that cannot find its anchor must
    //    fail loudly rather than return an empty list, because an empty deny
    //    list reads as "nothing is wrapped" — the same shape the core's commit
    //    hook refuses for its ownership deny list (docs/OWNERSHIP.md in the
    //    CORE's tree — not backticked, because it does not exist in this
    //    worktree, which is pinned to a commit before it was added).
    let renamed = real.replacen("pub const WRAPPED:", "pub const WRAPPED_PROGRAMS:", 1);
    assert_ne!(renamed, real, "the mutation did not apply");
    let panicked = std::panic::catch_unwind(move || parse_wrapped(&renamed)).is_err();
    assert!(
        panicked,
        "WRAPPED was renamed and the derivation returned quietly instead of \
         failing. A derivation that cannot find its anchor must say so."
    );
}
