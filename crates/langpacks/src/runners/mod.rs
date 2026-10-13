//! §6.2 runner output parsers for the four languages.
//!
//! # Every parser here was written against bytes a real runner produced
//!
//! `tests/fixtures/runners/README.md` names the tool and version behind each
//! file. That convention is `verify`'s, and it exists because of a specific
//! failure recorded there: a `pytest -q` parser was "proven" against a
//! hand-written fixture carrying a banner pytest does not emit, and the first
//! real run came back `unverifiable`.
//!
//! `verify` also recorded a gap it could not close — *"`go test` is still a
//! documented shape — there is no Go toolchain here"*. **That gap is closed
//! here**: Go, Maven, Gradle, dotnet, RSpec and minitest were all installed and
//! run, and every fixture in this crate is a capture.

pub mod dotnet;
pub mod gotest;
pub mod gradle;
pub mod junit_xml;
pub mod maven;
pub mod rspec;

pub use thruline_verify::parse::{Parsed, Runner, RunnerOutcome};

/// The runners this crate parses — including the two `verify::parse::Runner`
/// **cannot name**.
///
/// # The gap this enum exists to make visible rather than paper over
///
/// `verify::parse::Runner` is a closed enum: `Pytest`, `CargoTest`, `GoTest`,
/// `Jest`, `Mocha`, `DotnetTest`, `Maven`, `Gradle`, `JUnitXml`. There is no
/// `RSpec` and no `Minitest`. So a Ruby parser has nowhere to put its answer —
/// not because Ruby is unsupported, but because the type that names runners
/// has no room for it.
///
/// The wrong fix is to return `Runner::JUnitXml` for an RSpec run. That is a
/// lie in a field a report prints, and it is the kind of lie this product
/// exists to catch. So Ruby's runners are named here, `into_verify()` returns
/// `None` for them, and the merge plan asks for the two variants.
///
/// # The second half of the gap: capture
///
/// `thruline_capture::shim::WRAPPED` is
/// `pytest, cargo, go, npm, pnpm, yarn, npx, make, mvn, gradle, dotnet, uv,
/// uvx, python, python3`.
///
/// `go`, `mvn`, `gradle` and `dotnet` are on it. **`ruby`, `rspec`, `rake` and
/// `bundle` are not**, so on the live path no `exec` event for a Ruby test run
/// is ever written, and a perfect RSpec parser would still never be reached.
/// `Runner::from_argv`'s own doc comment states the rule this falls under:
/// *"an arm whose program is not in that list is unreachable from real capture
/// however correct it is … Adding an arm and adding the wrapper are one
/// change. Half of it is worth nothing, and reads as coverage that does not
/// exist."*
///
/// The Ruby parsers here are therefore **correct and unreachable** until the
/// wrapper lands. That is written down rather than left for someone to
/// discover from an empty report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackRunner {
    /// Reachable: `go` is wrapped.
    GoTest,
    /// Reachable: `mvn` is wrapped.
    Maven,
    /// Reachable as `gradle`; **not** as `./gradlew`, which is a path and never
    /// consults PATH at all.
    Gradle,
    /// Reachable: `dotnet` is wrapped.
    DotnetTest,
    /// Reachable: whoever produced the XML, it is read from a file.
    JUnitXml,
    /// **Not reachable from capture today.** No `Runner` variant, no wrapper.
    RSpec,
    /// **Not reachable from capture today.** No `Runner` variant, no wrapper.
    Minitest,
}

impl PackRunner {
    pub fn as_str(self) -> &'static str {
        match self {
            PackRunner::GoTest => "go test",
            PackRunner::Maven => "mvn test",
            PackRunner::Gradle => "gradle test",
            PackRunner::DotnetTest => "dotnet test",
            PackRunner::JUnitXml => "junit-xml",
            PackRunner::RSpec => "rspec",
            PackRunner::Minitest => "minitest",
        }
    }

    /// The `verify::parse::Runner` this maps to, or `None` when verify has no
    /// name for it. **Never guess a near-miss variant here.**
    pub fn into_verify(self) -> Option<Runner> {
        Some(match self {
            PackRunner::GoTest => Runner::GoTest,
            PackRunner::Maven => Runner::Maven,
            PackRunner::Gradle => Runner::Gradle,
            PackRunner::DotnetTest => Runner::DotnetTest,
            PackRunner::JUnitXml => Runner::JUnitXml,
            PackRunner::RSpec | PackRunner::Minitest => return None,
        })
    }

    /// Is the program that runs this on `thruline_capture::shim::WRAPPED`?
    ///
    /// Re-derived by `tests/capture_reachability.rs` from `shim.rs` itself, so
    /// this cannot quietly go stale the way a transcribed constant does — the
    /// convention `scripts/vendor-freshness.py` established.
    pub fn reachable_from_capture(self) -> bool {
        !matches!(self, PackRunner::RSpec | PackRunner::Minitest)
    }
}

/// Build the `Parsed` all parsers here return, so no parser can forget a field
/// and have it default to a number that reads as a clean run.
pub fn parsed(runner: Runner) -> Parsed {
    Parsed {
        runner,
        tests_total: 0,
        passed: 0,
        failed: 0,
        skipped: 0,
        xfail: 0,
        test_ids: Default::default(),
        duration_ms: None,
    }
}

/// The one shape of `Unverifiable` this crate emits: a reason and a hint that
/// says what would make the run parseable. §6.2 requires the hint.
pub fn unverifiable(reason: impl Into<String>, hint: impl Into<String>) -> RunnerOutcome {
    RunnerOutcome::Unverifiable {
        reason: reason.into(),
        hint: hint.into(),
    }
}
