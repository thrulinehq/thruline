//! §6.3 narrowing flags for the six runners this crate reaches, enumerated from
//! each runner's own documentation on this machine.
//!
//! # Why this is not one flat list
//!
//! `verify::tests_pass::NARROWING_FLAGS` is a single flat `&[&str]` of thirteen
//! spellings, matched against every argv regardless of which runner produced it.
//! It was assembled for pytest, cargo and jest, and against these six runners it
//! is wrong in **both** directions at once:
//!
//! - It **fires on flags that do not narrow.** `-t` is in it. For Gradle 9.7.1
//!   `-t` is `--continuous` (`gradle -h`); for `dotnet test` it is
//!   `--list-tests`; for RSpec it is `--tag`. One of those three narrows. So a
//!   perfectly honest `gradle test -t` is reported as a subset run — and under a
//!   `scope: all` claim that finding is `Severity::Fail`. Refusing honest
//!   evidence is the expensive direction, and a shared list guarantees it.
//! - It **misses the real ones.** Not one of `-Dtest=`, `-Dit.test=`, `--tests`,
//!   `--filter`, `-run`, `-test.run`, `-e`, `--example`, `-i`, `--include`,
//!   `-pl` is in it. `go test -run=TestOne ./...` is, today, indistinguishable
//!   from a full run.
//!
//! The full audit — every entry of that list that is wrong for these runners and
//! every spelling it lacks — is in the merge plan. This module does not edit
//! `verify`; it publishes the runner-keyed table `verify` needs.
//!
//! # Every entry was re-derived, not transcribed
//!
//! `scripts/vendor-freshness.py` is this repo's precedent: a hand-copied vendor
//! constant with a dated comment beside it is not a checked fact, and four of the
//! six Codex facts that script was written to guard were already wrong on its
//! first run. So every [`NarrowingFlag::derived_from`] below names a command that
//! was actually run here, or a file on this machine that was actually read:
//!
//! ```text
//! go help test          go help testflag        go version go1.27.1 darwin/arm64
//! mvn -h                                        Apache Maven 3.9.16
//! unzip -p ~/.m2/…/maven-surefire-plugin-3.5.2.jar META-INF/maven/plugin.xml
//! unzip -p …/maven-failsafe-plugin-3.5.2.jar      META-INF/maven/plugin.xml
//! gradle -h             gradle help --task test  Gradle 9.7.1
//! dotnet test --help    dotnet vstest --help     .NET SDK 9.0.317
//! rspec --help                                   rspec-core 3.13.6
//! /opt/homebrew/lib/ruby/gems/4.0.0/gems/minitest-6.0.6/lib/minitest.rb
//! …/minitest-6.0.6/lib/minitest/test_task.rb
//! ```
//!
//! That re-derivation immediately corrected two facts this stage was briefed
//! with. **minitest's narrowing flags are not `-n`/`--name` any more**: in
//! 6.0.6 they are `-i`/`--include` and `-e`/`--exclude`, with `-n`, `--name` and
//! `-x` kept as aliases that `--help` does not print (`lib/minitest.rb`, the
//! `opts.alias` block). And **minitest has no `-t` at all**, in any spelling —
//! the brief's "`-t` narrows for minitest-via-rake" describes no flag that
//! exists in the installed gem; via rake the assignment is `N=`/`I=`/`X=`/`E=`
//! (`lib/minitest/test_task.rb`).
//!
//! # The inclusion rule, stated once
//!
//! Two failure directions, and they are not symmetric. Missing a narrowing flag
//! lets a subset run support a `scope: all` claim. Inventing one refuses an
//! honest full-suite run. So:
//!
//! > **A flag no honest full-suite run would ever carry is included even when
//! > some other §6.3 clause also covers it. A flag an honest full-suite run
//! > plausibly carries is excluded unless the runner's own documentation says it
//! > selects a subset.**
//!
//! Everything excluded under the second half is in [`NOT_NARROWING`], with the
//! reason, and `tests/narrowing_flags.rs` asserts each of those argvs still
//! comes back `None`. An exclusion that is written down and tested is a decision;
//! one that is merely absent is a bug waiting to be found by a user.
//!
//! # The measurement that forced `--dry-run` in and `-count=0` out
//!
//! Both of those were run here rather than reasoned about.
//!
//! `rspec --dry-run` on a two-example suite prints `2 examples, 0 failures` and
//! exits 0 — **the same bytes a real passing run prints**, with nothing marked
//! pending. No output parser can tell the two apart, so §6.3 would call a run
//! that executed nothing `supported`. It is in the table.
//!
//! `gradle test --test-dry-run` on a two-test JUnit 5 project reports both as
//! `SKIPPED` and writes `tests="2" skipped="2" failures="0"`. §6.3's skip clause
//! sees that — but it is `Severity::Warn`, and §6.3's conjunction never requires
//! `passed == tests_total`, so the verdict is still `supported`. It is in the
//! table too.
//!
//! `go test -count=0 ./...` prints `no tests to run` and emits no `--- PASS`
//! lines at all, so the run reports zero tests and §6.3's `tests_total == 0`
//! clause returns `unverifiable` *before* the narrowing detector is reached.
//! Flagging it would only risk `-count=1`, which is the idiomatic honest
//! cache-buster. It is in [`NOT_NARROWING`].

use crate::runners::{PackRunner, Runner};

/// One runner's one narrowing flag, and where the fact came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NarrowingFlag {
    /// The runner this spelling belongs to. **Never matched across runners** —
    /// see the module header for the three different meanings of `-t`.
    pub runner: PackRunner,
    /// Every spelling that reaches the same behaviour, exactly as the runner
    /// accepts it, case included. A spelling that does not begin with `-` is a
    /// `NAME=value` assignment on a `rake` command line, not a flag.
    pub spellings: &'static [&'static str],
    /// Does the documentation give this an operand — either as the next argv
    /// element or after an `=`? Booleans that are merely *allowed* to be written
    /// `--flag=true` (Maven's `-D` properties) are `false`; the matcher accepts
    /// the `=` form for everything regardless.
    pub takes_value: bool,
    /// One line, quote-free, of what that runner's documentation says it does.
    pub what_the_docs_say: &'static str,
    /// The command that was run, or the file that was read, to establish this.
    pub derived_from: &'static str,
}

/// A spelling deliberately **left out**, and the reason.
///
/// This exists so the refusing-honest-evidence direction is testable. Every
/// entry is an argv `narrowing_flag` must return `None` for, and
/// `tests/narrowing_flags.rs` checks all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotNarrowing {
    pub runner: PackRunner,
    pub spellings: &'static [&'static str],
    /// Why an honest full-suite run can carry this.
    pub why: &'static str,
}

const GO_TESTFLAG: &str = "go help testflag (go1.27.1 darwin/arm64)";
const GO_TEST: &str = "go help test (go1.27.1 darwin/arm64)";
const MVN_H: &str = "mvn -h (Apache Maven 3.9.16)";
const SUREFIRE: &str =
    "META-INF/maven/plugin.xml inside ~/.m2/…/maven-surefire-plugin-3.5.2.jar (unzip -p)";
const FAILSAFE: &str =
    "META-INF/maven/plugin.xml inside maven-failsafe-plugin-3.5.2.jar (unzip -p)";
const GRADLE_H: &str = "gradle -h (Gradle 9.7.1)";
const GRADLE_TASK: &str = "gradle help --task test (Gradle 9.7.1, org.gradle.jvm-test-suite)";
const DOTNET_HELP: &str = "dotnet test --help (.NET SDK 9.0.317)";
const RSPEC_HELP: &str = "rspec --help (rspec-core 3.13.6)";
const MINITEST_SRC: &str =
    "minitest-6.0.6/lib/minitest.rb, the Minitest.process_args OptionParser block";
const MINITEST_RAKE: &str = "minitest-6.0.6/lib/minitest/test_task.rb, make_test_cmd's ENV reads";

/// Every narrowing flag this crate knows, keyed by the runner that accepts it.
pub const NARROWING_FLAGS: &[NarrowingFlag] = &[
    // ---- go test ---------------------------------------------------------
    //
    // Go's flag package accepts one or two leading dashes for the same flag;
    // `go test --run=TestOne -v ./...` was run here and reported one test where
    // the unflagged run reported two. The matcher normalises `--x` to `-x` for
    // this runner only, so both spellings need not be listed twice.
    NarrowingFlag {
        runner: PackRunner::GoTest,
        spellings: &["-run"],
        takes_value: true,
        what_the_docs_say:
            "runs only the tests, examples and fuzz tests whose identifier matches the regular \
             expression, splitting it on unbracketed slashes to match subtests",
        derived_from: GO_TESTFLAG,
    },
    NarrowingFlag {
        runner: PackRunner::GoTest,
        spellings: &["-skip"],
        takes_value: true,
        what_the_docs_say:
            "runs only the tests, examples, fuzz tests and benchmarks that do NOT match the \
             regular expression",
        derived_from: GO_TESTFLAG,
    },
    // The `-test.` forms are the spellings the compiled test binary itself
    // takes, and `go test` passes them straight through. This is not theory:
    // `go test -test.run=TestOne -v ./...` was run here and executed exactly one
    // of the two tests. It is the single most complete blind spot in verify's
    // flat list, because nothing in that list resembles it.
    NarrowingFlag {
        runner: PackRunner::GoTest,
        spellings: &["-test.run"],
        takes_value: true,
        what_the_docs_say:
            "the test binary's own spelling of -run; go test forwards it unchanged, and it \
             narrows exactly as -run does",
        derived_from: GO_TESTFLAG,
    },
    NarrowingFlag {
        runner: PackRunner::GoTest,
        spellings: &["-test.skip"],
        takes_value: true,
        what_the_docs_say: "the test binary's own spelling of -skip, forwarded unchanged",
        derived_from: GO_TESTFLAG,
    },
    // Included under the inclusion rule: no honest full-suite run passes -list,
    // because -list runs nothing at all.
    NarrowingFlag {
        runner: PackRunner::GoTest,
        spellings: &["-list", "-test.list"],
        takes_value: true,
        what_the_docs_say:
            "lists the matching tests, benchmarks, fuzz tests or examples and runs none of them",
        derived_from: GO_TESTFLAG,
    },
    // ---- mvn: the reactor ------------------------------------------------
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-pl", "--projects"],
        takes_value: true,
        what_the_docs_say:
            "restricts the reactor to a comma-delimited list of the named modules, so modules \
             outside the list are not built and their tests never run",
        derived_from: MVN_H,
    },
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-N", "--non-recursive"],
        takes_value: false,
        what_the_docs_say:
            "does not recurse into sub-projects, so in a multi-module build only the aggregator \
             POM's own tests are considered",
        derived_from: MVN_H,
    },
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-rf", "--resume-from"],
        takes_value: true,
        what_the_docs_say:
            "resumes the reactor from the named module, so every module ordered before it is \
             skipped",
        derived_from: MVN_H,
    },
    // ---- mvn: Surefire and Failsafe --------------------------------------
    //
    // These are plugin parameters, not `mvn` options, so `mvn -h` does not list
    // them. The `${…}` user property behind each was read out of the plugin
    // descriptor in the jar that is actually in this machine's ~/.m2 — the
    // artefact, not a web page.
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-Dtest"],
        takes_value: true,
        what_the_docs_say:
            "runs individual tests by file-name pattern, overriding the plugin's includes and \
             excludes; a pattern prefixed with ! excludes instead",
        derived_from: SUREFIRE,
    },
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-Dit.test"],
        takes_value: true,
        what_the_docs_say:
            "the Failsafe equivalent of -Dtest, selecting which integration tests mvn verify runs",
        derived_from: FAILSAFE,
    },
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-Dgroups"],
        takes_value: true,
        what_the_docs_say:
            "runs only classes or methods carrying one of the named JUnit tags, TestNG groups or \
             JUnit 4 categories",
        derived_from: SUREFIRE,
    },
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-DexcludedGroups"],
        takes_value: true,
        what_the_docs_say:
            "runs everything except classes or methods carrying one of the named tags, groups or \
             categories",
        derived_from: SUREFIRE,
    },
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &[
            "-Dsurefire.includes",
            "-Dsurefire.excludes",
            "-Dsurefire.includesFile",
            "-Dsurefire.excludesFile",
        ],
        takes_value: true,
        what_the_docs_say:
            "replaces the plugin's default include or exclude patterns for which test classes are \
             part of the run",
        derived_from: SUREFIRE,
    },
    // Under the inclusion rule: no honest run that claims its tests passed
    // carries a flag whose documented job is to not run them.
    NarrowingFlag {
        runner: PackRunner::Maven,
        spellings: &["-DskipTests", "-Dmaven.test.skip", "-Dmaven.test.skip.exec"],
        takes_value: false,
        what_the_docs_say:
            "skips running the tests entirely; maven.test.skip also skips compiling them",
        derived_from: SUREFIRE,
    },
    // ---- gradle ----------------------------------------------------------
    NarrowingFlag {
        runner: PackRunner::Gradle,
        spellings: &["--tests"],
        takes_value: true,
        what_the_docs_say:
            "sets the test class or method name to include, in addition to the test task's own \
             filters; * is supported",
        derived_from: GRADLE_TASK,
    },
    NarrowingFlag {
        runner: PackRunner::Gradle,
        spellings: &["-x", "--exclude-task"],
        takes_value: true,
        what_the_docs_say: "excludes the named task from execution",
        derived_from: GRADLE_H,
    },
    // See the module header: this one reports every test as SKIPPED with a
    // green build, and §6.3's skip clause is only a warn.
    NarrowingFlag {
        runner: PackRunner::Gradle,
        spellings: &["--test-dry-run"],
        takes_value: false,
        what_the_docs_say: "simulates test execution rather than performing it",
        derived_from: GRADLE_TASK,
    },
    // `-m`/`--dry-run` disables every task action, so the test task produces no
    // results at all. Kept for the same reason as `--test-dry-run`.
    NarrowingFlag {
        runner: PackRunner::Gradle,
        spellings: &["-m", "--dry-run"],
        takes_value: false,
        what_the_docs_say: "runs the build with all task actions disabled",
        derived_from: GRADLE_H,
    },
    // ---- dotnet test -----------------------------------------------------
    NarrowingFlag {
        runner: PackRunner::DotnetTest,
        spellings: &["--filter"],
        takes_value: true,
        what_the_docs_say:
            "runs only the tests matching the given expression, over properties such as \
             FullyQualifiedName, Priority or Category",
        derived_from: DOTNET_HELP,
    },
    NarrowingFlag {
        runner: PackRunner::DotnetTest,
        spellings: &["-t", "--list-tests"],
        takes_value: false,
        what_the_docs_say: "lists the discovered tests instead of running the tests",
        derived_from: DOTNET_HELP,
    },
    // ---- rspec -----------------------------------------------------------
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["-e", "--example"],
        takes_value: true,
        what_the_docs_say:
            "runs only the examples whose full nested name includes the given string; may be \
             given more than once",
        derived_from: RSPEC_HELP,
    },
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["-E", "--example-matches"],
        takes_value: true,
        what_the_docs_say:
            "runs only the examples whose full nested name matches the given regular expression",
        derived_from: RSPEC_HELP,
    },
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["-t", "--tag"],
        takes_value: true,
        what_the_docs_say:
            "runs only the examples carrying the given tag, or excludes them when the tag is \
             prefixed with a tilde",
        derived_from: RSPEC_HELP,
    },
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["--only-failures"],
        takes_value: false,
        what_the_docs_say: "filters to just the examples that failed the last time they ran",
        derived_from: RSPEC_HELP,
    },
    // RSpec's `-n` is a switch. Minitest's `-n` takes a value. Same two
    // characters, different arity, different runner — which is why arity is a
    // per-entry field and not a global rule.
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["-n", "--next-failure"],
        takes_value: false,
        what_the_docs_say:
            "applies --only-failures and aborts after one failure, so at most one previously \
             failing example runs",
        derived_from: RSPEC_HELP,
    },
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["-P", "--pattern"],
        takes_value: true,
        what_the_docs_say:
            "loads only the spec files matching the pattern, replacing the default of \
             spec/**/*_spec.rb",
        derived_from: RSPEC_HELP,
    },
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["--exclude-pattern"],
        takes_value: true,
        what_the_docs_say: "loads every spec file except those matching the pattern",
        derived_from: RSPEC_HELP,
    },
    // The one measured here: it prints a passing tally for a suite it never ran.
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["--dry-run"],
        takes_value: false,
        what_the_docs_say:
            "prints the formatter output for the suite without running any examples or hooks",
        derived_from: RSPEC_HELP,
    },
    NarrowingFlag {
        runner: PackRunner::RSpec,
        spellings: &["--bisect"],
        takes_value: false,
        what_the_docs_say:
            "repeatedly re-runs subsets of the suite to isolate a failure to the smallest \
             reproducible case",
        derived_from: RSPEC_HELP,
    },
    // ---- minitest --------------------------------------------------------
    //
    // Direct invocation: `ruby -Itest test/foo_test.rb -i /pat/`. In 6.0.6 the
    // documented pair is -i/--include and -e/--exclude; -n, --name and -x are
    // aliases installed by `opts.alias` and deliberately absent from --help.
    // Both spellings are carried here because both still work and both appear
    // in the wild.
    NarrowingFlag {
        runner: PackRunner::Minitest,
        spellings: &["-i", "--include", "-n", "--name"],
        takes_value: true,
        what_the_docs_say:
            "includes only the tests whose name matches the given regexp or string; -n and --name \
             are undocumented aliases of the same switch",
        derived_from: MINITEST_SRC,
    },
    NarrowingFlag {
        runner: PackRunner::Minitest,
        spellings: &["-e", "--exclude", "-x"],
        takes_value: true,
        what_the_docs_say:
            "excludes the tests whose name matches the given regexp or string; -x is an \
             undocumented alias of the same switch",
        derived_from: MINITEST_SRC,
    },
    // Minitest via rake. `Minitest::TestTask` turns these environment-style
    // assignments on the rake command line into the flags above, so they reach
    // the runner without any of the flags above ever appearing in argv.
    NarrowingFlag {
        runner: PackRunner::Minitest,
        spellings: &["N", "I"],
        takes_value: true,
        what_the_docs_say: "rake-level assignment that Minitest::TestTask forwards as -i (include)",
        derived_from: MINITEST_RAKE,
    },
    NarrowingFlag {
        runner: PackRunner::Minitest,
        spellings: &["X", "E"],
        takes_value: true,
        what_the_docs_say: "rake-level assignment that Minitest::TestTask forwards as -x (exclude)",
        derived_from: MINITEST_RAKE,
    },
    // `A=`, `TESTOPTS=` and `FILTER=` carry a whole argument string through to
    // the runner, so whether they narrow depends on what is inside them. The
    // matcher splits the value and looks; `A=-v` does not fire, `A=-n /slow/`
    // does. Flagging the assignment itself would have refused `rake test A=-v`,
    // which is the example the task's own documentation gives.
    NarrowingFlag {
        runner: PackRunner::Minitest,
        spellings: &["A", "TESTOPTS", "FILTER"],
        takes_value: true,
        what_the_docs_say:
            "rake-level assignment whose value is shell-split and appended to the runner's \
             argv, so it narrows exactly when the argv it carries does",
        derived_from: MINITEST_RAKE,
    },
];

/// Narrowing that carries no flag at all.
///
/// Four of the six runners take a selector as a bare argument, and it is
/// invisible to any table of flag spellings. `verify::tests_pass::narrowing_flag`
/// has one rule for this — a bare argument that `is_test_path` recognises — and
/// that rule was written for pytest paths. It matches none of these: `./pkg/...`
/// has no test-ish segment, `One.csproj` has none, `:core:test` has none, and
/// `spec/models/user_spec.rb:42` matches only because of the `spec` segment,
/// losing the `:42` that is the actual narrowing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectorForm {
    pub runner: PackRunner,
    /// An argv, as a command line, that this form narrows.
    pub narrows: &'static str,
    /// The nearest argv that does **not** narrow, so the pair is the boundary.
    pub does_not_narrow: &'static str,
    pub what_the_docs_say: &'static str,
    pub derived_from: &'static str,
}

/// Every no-flag narrowing form, with the honest run it must be told apart from.
pub const SELECTOR_FORMS: &[SelectorForm] = &[
    SelectorForm {
        runner: PackRunner::GoTest,
        narrows: "go test ./internal/...",
        does_not_narrow: "go test ./...",
        what_the_docs_say:
            "in package list mode each named package gets its own test binary, so naming a \
             subtree tests that subtree and nothing else; ./... is the whole module",
        derived_from: GO_TEST,
    },
    SelectorForm {
        runner: PackRunner::RSpec,
        narrows: "rspec spec/models/user_spec.rb:42",
        does_not_narrow: "rspec",
        what_the_docs_say:
            "appending line numbers to a filename selects individual examples, and a bare \
             filename or directory selects the examples in it; with no argument RSpec loads its \
             whole default path",
        derived_from: RSPEC_HELP,
    },
    SelectorForm {
        runner: PackRunner::DotnetTest,
        narrows: "dotnet test src/One/One.csproj",
        does_not_narrow: "dotnet test All.sln",
        what_the_docs_say:
            "the positional argument is the project or solution under test; naming one project \
             of a solution runs that project's tests only",
        derived_from: DOTNET_HELP,
    },
    SelectorForm {
        runner: PackRunner::Gradle,
        narrows: "gradle :core:test",
        does_not_narrow: "gradle test",
        what_the_docs_say:
            "a task is addressed by a project-qualified path, so :core:test is the test task of \
             one subproject where the unqualified name reaches every subproject that has one",
        derived_from: GRADLE_TASK,
    },
    SelectorForm {
        runner: PackRunner::Maven,
        narrows: "mvn -pl core test",
        does_not_narrow: "mvn test",
        what_the_docs_say:
            "Maven's positional arguments are lifecycle phases and goals, never test selectors; \
             the reactor is narrowed by the -pl, -N and -rf options instead",
        derived_from: MVN_H,
    },
    SelectorForm {
        runner: PackRunner::Minitest,
        narrows: "ruby -Itest test/models/user_test.rb",
        does_not_narrow: "rake test",
        what_the_docs_say:
            "minitest is invoked either on one test file directly or through a rake task that \
             loads the whole test directory",
        derived_from: MINITEST_RAKE,
    },
];

/// Spellings this module refuses to treat as narrowing, and why.
///
/// Everything here is a flag an honest full-suite run plausibly carries. Firing
/// on one of them produces a `narrowed_run` finding, which is `Severity::Fail`
/// under a `scope: all` claim — that is a true statement made false, and it is
/// the direction that costs a user their evidence.
pub const NOT_NARROWING: &[NotNarrowing] = &[
    NotNarrowing {
        runner: PackRunner::GoTest,
        spellings: &["-short"],
        why: "it only sets testing.Short(); whether any test then skips is that test's own \
              decision, and every skip it causes is reported in the tally as a skip",
    },
    NotNarrowing {
        runner: PackRunner::GoTest,
        spellings: &["-count"],
        why: "-count=1 is the idiomatic way to defeat go's test cache and is on a great many \
              honest full-suite runs; -count=0 does run nothing, but it emits `no tests to run` \
              and no PASS lines, so §6.3's tests_total == 0 clause returns unverifiable before \
              the narrowing detector is reached (captured here on go1.27.1)",
    },
    NotNarrowing {
        runner: PackRunner::GoTest,
        spellings: &["-bench", "-fuzz"],
        why: "both select benchmarks or fuzz targets; the package's ordinary tests still run in \
              full, and the idiom that suppresses them is -run=^$, which this table does catch",
    },
    NotNarrowing {
        runner: PackRunner::Maven,
        spellings: &[
            "-Dsurefire.failIfNoSpecifiedTests",
            "-Dit.failIfNoSpecifiedTests",
        ],
        why: "on its own it narrows nothing: it only decides whether a -Dtest= that matched \
              nothing fails the build, and multi-module reactors legitimately set it false so \
              modules without a matching test do not break. It is a suppressor, and it is \
              dangerous only paired with -Dtest=, which fires on its own",
    },
    NotNarrowing {
        runner: PackRunner::Maven,
        spellings: &["-f", "--file"],
        why: "an alternate POM is as often the only POM as it is a sub-module of a larger \
              reactor, and argv cannot tell which; -pl, -N and -rf cover the cases where the \
              narrowing is stated",
    },
    NotNarrowing {
        runner: PackRunner::Maven,
        spellings: &["-o", "--offline", "-q", "--quiet", "-B", "--batch-mode"],
        why: "none of them touches test selection, and all three are routine on CI command lines",
    },
    NotNarrowing {
        runner: PackRunner::Gradle,
        spellings: &["-t", "--continuous"],
        why: "for Gradle -t is continuous build — it re-executes tasks when inputs change, and \
              runs no less than the same command without it. verify's flat list contains -t, \
              and this is the entry that list gets wrong",
    },
    NotNarrowing {
        runner: PackRunner::Gradle,
        spellings: &["--fail-fast"],
        why: "it stops after the first failing test, and a run with no failures is unaffected; a \
              run with one has a non-zero exit code, which §6.3 refuses for its own reasons",
    },
    NotNarrowing {
        runner: PackRunner::Gradle,
        spellings: &["--offline", "--rerun", "--rerun-tasks", "--continue"],
        why: "cache and network behaviour, not test selection",
    },
    NotNarrowing {
        runner: PackRunner::DotnetTest,
        spellings: &["-e", "--environment"],
        why: "for dotnet test -e sets an environment variable. It is RSpec and minitest where -e \
              selects examples, and matching it across runners would refuse every dotnet run \
              that sets one",
    },
    NotNarrowing {
        runner: PackRunner::DotnetTest,
        spellings: &["-f", "--framework"],
        why: "it picks one target framework of a multi-targeted project. That is fewer test \
              *executions* but not fewer distinct tests, and argv cannot tell a multi-targeted \
              project from a single-targeted one",
    },
    NotNarrowing {
        runner: PackRunner::DotnetTest,
        spellings: &[
            "--no-build",
            "--nologo",
            "-v",
            "--verbosity",
            "-c",
            "--configuration",
        ],
        why: "build and output options; --no-build weakens the binding to the final tree, which \
              is §6.1's problem and not this detector's",
    },
    NotNarrowing {
        runner: PackRunner::RSpec,
        spellings: &["-I", "-r", "--require"],
        why: "load path and require, not selection. -I is one ASCII case away from minitest's -i \
              (include), which is why this matcher is case-sensitive and verify's — which \
              lowercases every argument before comparing — could not be reused",
    },
    NotNarrowing {
        runner: PackRunner::RSpec,
        spellings: &["--fail-fast"],
        why: "same as Gradle's: a clean run is unaffected, and a dirty one fails on its exit code",
    },
    NotNarrowing {
        runner: PackRunner::RSpec,
        spellings: &["--default-path"],
        why: "it relocates the root RSpec looks in rather than filtering within it, and every \
              project that does not keep its specs in spec/ passes it on every single run",
    },
    NotNarrowing {
        runner: PackRunner::RSpec,
        spellings: &[
            "-f",
            "--format",
            "-o",
            "--out",
            "--order",
            "--seed",
            "-p",
            "--profile",
        ],
        why: "formatting and ordering. --order and --seed change which order the whole suite runs \
              in, not which part of it runs",
    },
    NotNarrowing {
        runner: PackRunner::Minitest,
        spellings: &[
            "-s",
            "--seed",
            "-v",
            "--verbose",
            "-q",
            "--quiet",
            "--show-skips",
        ],
        why: "ordering and reporting; none of them changes which tests are collected",
    },
    NotNarrowing {
        runner: PackRunner::Minitest,
        spellings: &["-S", "--skip"],
        why: "despite the name it skips *reporting* of certain result types (eg E for error), \
              not the tests themselves — read out of the option's own description in \
              lib/minitest.rb rather than guessed from the word skip",
    },
];

/// Does this argv narrow the run, and by what?
///
/// Matched on the **argv**, never on a command string — §13, and the same reason
/// `verify::parse::Runner::from_argv` exists. `argv[0]` is skipped: it is the
/// program, and for `mvn`/`gradle`/`dotnet`/`go` the subcommand that follows is
/// a bare word this must not read as a selector.
///
/// The return is prose rather than a bare flag because the caller interpolates
/// it into a sentence (`"… narrows the run with {flag} …"`) and some of these
/// need a clause to keep that sentence true — `--dry-run` does not run a subset,
/// it runs nothing. `verify`'s own bare-path arm sets that precedent by
/// returning `"a named path (…)"`.
///
/// Case-sensitive throughout. `verify::tests_pass::narrowing_flag` lowercases
/// every argument before comparing, which would collapse RSpec's `-I` (load
/// path, harmless) into minitest's `-i` (include, narrowing).
pub fn narrowing_flag(runner: PackRunner, argv: &[String]) -> Option<String> {
    // A JUnit XML report has no command line at all; it is a file found on
    // disk. Answering anything but "I cannot tell" from an argv that does not
    // belong to it would be an answer about some other process.
    if runner == PackRunner::JUnitXml {
        return None;
    }

    for (i, raw) in argv.iter().enumerate().skip(1) {
        // Go accepts one or two leading dashes for the same flag; `go test
        // --run=TestOne` was run here and narrowed. Normalising once beats
        // listing every Go spelling twice and forgetting one.
        let arg: &str = if runner == PackRunner::GoTest && raw.starts_with("--") && raw.len() > 2 {
            &raw[1..]
        } else {
            raw.as_str()
        };

        for entry in NARROWING_FLAGS.iter().filter(|f| f.runner == runner) {
            for spelling in entry.spellings {
                if !spelling_matches(runner, entry, spelling, arg) {
                    continue;
                }
                // The three pass-through assignments carry an argv inside a
                // string; look at what is actually in there.
                if is_passthrough_assignment(runner, spelling) {
                    match passthrough_narrows(runner, arg) {
                        Some(inner) => {
                            return Some(format!("{spelling}= carrying `{inner}`"));
                        }
                        None => continue,
                    }
                }
                return Some(describe(runner, entry, spelling, arg, argv.get(i + 1)));
            }
        }
    }

    positional_narrowing(runner, argv)
}

/// The same question, asked with `verify::parse::Runner`.
///
/// # Rule 7: this signature is the stage brief's, and it cannot carry Ruby
///
/// The brief specified `narrowing_flag(runner: Runner, …)` with `verify`'s
/// `Runner`. That enum is closed and has no `RSpec` and no `Minitest` variant —
/// `crate::runners::PackRunner` exists precisely because of that gap and says so
/// at length. Typing the table with `Runner` would have silently dropped every
/// Ruby entry above: six flags, four rake assignments, and the `file:LINE`
/// selector. So [`NARROWING_FLAGS`] is keyed by `PackRunner`, and this is the
/// adapter for callers that hold a `Runner`.
///
/// It returns `None` for pytest, cargo, jest and mocha — not because those never
/// narrow, but because `verify::tests_pass::NARROWING_FLAGS` is where their
/// spellings live and this crate does not own them. A caller must not read that
/// `None` as "not narrowed".
pub fn narrowing_flag_from_verify(runner: Runner, argv: &[String]) -> Option<String> {
    narrowing_flag(from_verify(runner)?, argv)
}

/// The total half of `PackRunner::into_verify`.
fn from_verify(runner: Runner) -> Option<PackRunner> {
    Some(match runner {
        Runner::GoTest => PackRunner::GoTest,
        Runner::Maven => PackRunner::Maven,
        Runner::Gradle => PackRunner::Gradle,
        Runner::DotnetTest => PackRunner::DotnetTest,
        Runner::JUnitXml => PackRunner::JUnitXml,
        Runner::Pytest | Runner::CargoTest | Runner::Jest | Runner::Mocha => return None,
    })
}

/// Does one argv element name this spelling?
///
/// Three accepted forms, and no fourth: exact equality, `spelling=value`, and —
/// for Ruby's two OptionParser-driven runners only — a one-letter short flag
/// with its value stuck to it (`-e/slow/`). OptionParser accepts that form;
/// Go's flag package, MSBuild and Gradle's option parser do not, and allowing it
/// everywhere would make `-name` match `-n`.
fn spelling_matches(runner: PackRunner, entry: &NarrowingFlag, spelling: &str, arg: &str) -> bool {
    if !spelling.starts_with('-') {
        // A rake-style assignment: only ever `NAME=value`, never a bare word.
        // Requiring the `=` is what stops the bare goal name `test` in
        // `rake test` from matching the spelling `TESTOPTS` by prefix.
        return arg.len() > spelling.len()
            && arg.starts_with(spelling)
            && arg.as_bytes().get(spelling.len()) == Some(&b'=');
    }
    if arg == spelling {
        return true;
    }
    if arg.len() > spelling.len()
        && arg.starts_with(spelling)
        && arg.as_bytes().get(spelling.len()) == Some(&b'=')
    {
        return true;
    }
    let ruby = matches!(runner, PackRunner::RSpec | PackRunner::Minitest);
    if ruby
        && entry.takes_value
        && spelling.len() == 2
        && arg.len() > 2
        && arg.starts_with(spelling)
    {
        return true;
    }
    false
}

fn is_passthrough_assignment(runner: PackRunner, spelling: &str) -> bool {
    runner == PackRunner::Minitest && matches!(spelling, "A" | "TESTOPTS" | "FILTER")
}

/// Does the argument string carried by `A=`/`TESTOPTS=`/`FILTER=` narrow?
///
/// Shell-split is approximated by whitespace, which is what `Shellwords.split`
/// does for every realistic value here. If it holds a minitest selection flag,
/// that flag is the answer; if it holds `-v`, nothing fires.
fn passthrough_narrows(runner: PackRunner, arg: &str) -> Option<String> {
    let (_, value) = arg.split_once('=')?;
    let inner: Vec<String> = std::iter::once(String::from("rake"))
        .chain(value.split_whitespace().map(str::to_string))
        .collect();
    narrowing_flag(runner, &inner)
}

/// The sentence fragment the caller interpolates.
///
/// Most spellings are their own best description. The exceptions are the ones
/// that run nothing at all, where "narrows the run" understates what happened
/// badly enough that the sentence would mislead a reader who trusted it.
fn describe(
    runner: PackRunner,
    entry: &NarrowingFlag,
    spelling: &str,
    arg: &str,
    next: Option<&String>,
) -> String {
    let nothing_ran = match (runner, spelling) {
        (PackRunner::RSpec, "--dry-run")
        | (PackRunner::Gradle, "--test-dry-run")
        | (PackRunner::Gradle, "-m")
        | (PackRunner::Gradle, "--dry-run")
        | (PackRunner::DotnetTest, "-t")
        | (PackRunner::DotnetTest, "--list-tests")
        | (PackRunner::GoTest, "-list")
        | (PackRunner::GoTest, "-test.list") => true,
        (PackRunner::Maven, s) => {
            matches!(
                s,
                "-DskipTests" | "-Dmaven.test.skip" | "-Dmaven.test.skip.exec"
            )
        }
        _ => false,
    };
    if nothing_ran {
        return format!("{spelling} (which executes no tests at all, not merely a subset)");
    }
    // The argument already carries its own operand (`-Dtest=Foo`, `-eslow`);
    // echo it whole. Otherwise show the separated operand, but only for a
    // spelling the documentation says takes one — RSpec's `-n` is a switch, and
    // appending the path that follows it would invent an operand it never had.
    if arg.len() > spelling.len() {
        return arg.to_string();
    }
    match next {
        Some(v) if entry.takes_value && !v.starts_with('-') => format!("{spelling} {v}"),
        _ => spelling.to_string(),
    }
}

/// Narrowing that carries no flag: a package selector, a `file:LINE`, a single
/// project in a solution, a project-qualified Gradle task.
fn positional_narrowing(runner: PackRunner, argv: &[String]) -> Option<String> {
    let positionals = positionals(runner, argv);
    match runner {
        // `go test ./internal/...` versus `go test ./...`. `./...` and `all`
        // are the whole module; anything else is a subset of it.
        //
        // A run with **no** package list at all is package-local mode — `go
        // help test`: it compiles the sources and tests found in the current
        // directory. That is narrower than the module whenever the module has
        // more than one package, and argv cannot say whether it does. Bare
        // `go test` is therefore NOT flagged: it is the single most common
        // honest invocation in the language, and guessing against it would
        // refuse evidence on every one-package repository.
        PackRunner::GoTest => {
            let pkgs: Vec<&String> = positionals
                .iter()
                .copied()
                .filter(|a| a.starts_with("./") || *a == "." || *a == "all" || *a == "...")
                .collect();
            if pkgs.is_empty() {
                return None;
            }
            if pkgs
                .iter()
                .any(|p| *p == "./..." || *p == "all" || *p == "...")
            {
                return None;
            }
            Some(format!("a package selector ({})", pkgs[0]))
        }
        // A Gradle task written with a project path — `:core:test`, or
        // `core:test` — runs in that subproject only, where the unqualified
        // `test` runs it in every subproject that has one.
        PackRunner::Gradle => positionals
            .iter()
            .find(|a| a.contains(':') && !a.starts_with('-'))
            .map(|t| format!("a project-qualified task ({t})")),
        // `dotnet test One.csproj` inside a solution runs that project only.
        // A `.sln`/`.slnx` argument is the whole solution and is not narrowing.
        PackRunner::DotnetTest => positionals
            .iter()
            .find(|a| {
                let l = a.to_ascii_lowercase();
                l.ends_with(".csproj")
                    || l.ends_with(".vbproj")
                    || l.ends_with(".fsproj")
                    || l.ends_with(".dll")
            })
            .map(|p| format!("a single project or assembly ({p})")),
        // `rspec spec/models/user_spec.rb:42`, `rspec spec/models`, and the
        // id form `rspec spec/a_spec.rb[1:5,1:6]` — all documented under
        // Filtering/tags in `rspec --help`. Bare `rspec` loads the whole
        // default path and is not narrowing.
        PackRunner::RSpec => positionals
            .first()
            .map(|p| format!("a named spec path ({p})")),
        // `ruby -Itest test/models/user_test.rb` runs one file where `rake
        // test` runs the suite. Only `.rb` paths count: `ruby` itself takes
        // positional arguments that are not test files.
        PackRunner::Minitest => positionals
            .iter()
            .find(|a| a.to_ascii_lowercase().ends_with(".rb"))
            .map(|p| format!("a named test file ({p})")),
        // Maven positionals are lifecycle phases and goals — `clean`, `test`,
        // `verify`, `org.foo:bar:baz`. None of them selects a subset of tests;
        // reactor narrowing is `-pl`/`-N`/`-rf`, which are flags.
        PackRunner::Maven => None,
        PackRunner::JUnitXml => None,
    }
}

/// The arguments that are not flags and not the operand of one.
///
/// The per-runner operand lists below are the value-taking options of each
/// runner's own help output. They exist for one reason: without them
/// `rspec -f documentation` reads `documentation` as a named spec path and every
/// honest formatted run is reported as narrowed. That is the failure mode
/// `verify`'s own comment records for `cargo test` — *"reading it as a test path
/// made every full `cargo test` run look narrowed — the direction that refuses
/// honest evidence."*
fn positionals(runner: PackRunner, argv: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for (i, arg) in argv.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with('-') {
            skip_next = takes_operand(runner, arg);
            continue;
        }
        // Words that are never selectors for their runner: the subcommand
        // (`go test`, `dotnet test`), a Maven lifecycle phase or goal, an
        // unqualified Gradle task name, the `test` goal of `rake test`.
        let structural = match runner {
            PackRunner::GoTest | PackRunner::DotnetTest | PackRunner::Minitest => {
                i == 1 && (arg == "test" || arg == "vstest")
            }
            PackRunner::Maven => true,
            PackRunner::Gradle => !arg.contains(':'),
            PackRunner::RSpec | PackRunner::JUnitXml => false,
        };
        if structural {
            continue;
        }
        out.push(arg);
    }
    out
}

/// Does this flag consume the next argv element?
///
/// Only the value-taking options each runner's help actually lists. A flag not
/// named here consumes nothing, so a genuine selector immediately after an
/// unknown switch is still seen — the safe direction for this particular table,
/// because over-listing hides selectors and under-listing at worst mistakes one
/// operand for a path.
fn takes_operand(runner: PackRunner, arg: &str) -> bool {
    // `--flag=value` carries its own operand.
    if arg.contains('=') {
        return false;
    }
    match runner {
        PackRunner::GoTest => matches!(
            arg,
            "-run"
                | "-skip"
                | "-bench"
                | "-benchtime"
                | "-count"
                | "-cpu"
                | "-fuzz"
                | "-fuzztime"
                | "-fuzzminimizetime"
                | "-list"
                | "-outputdir"
                | "-parallel"
                | "-timeout"
                | "-shuffle"
                | "-coverprofile"
                | "-covermode"
                | "-coverpkg"
                | "-cpuprofile"
                | "-memprofile"
                | "-blockprofile"
                | "-mutexprofile"
                | "-trace"
                | "-exec"
                | "-tags"
                | "-gcflags"
                | "-ldflags"
                | "-o"
        ),
        PackRunner::Maven => matches!(
            arg,
            "-pl"
                | "--projects"
                | "-rf"
                | "--resume-from"
                | "-f"
                | "--file"
                | "-s"
                | "--settings"
                | "-P"
                | "--activate-profiles"
                | "-t"
                | "--toolchains"
                | "-b"
                | "--builder"
                | "-T"
                | "--threads"
                | "-l"
                | "--log-file"
                | "-emp"
                | "-ep"
        ),
        PackRunner::Gradle => matches!(
            arg,
            "--tests"
                | "-x"
                | "--exclude-task"
                | "-p"
                | "--project-dir"
                | "-b"
                | "--build-file"
                | "-c"
                | "--settings-file"
                | "-g"
                | "--gradle-user-home"
                | "-I"
                | "--init-script"
                | "-D"
                | "--system-prop"
                | "-P"
                | "--project-prop"
                | "--console"
                | "--max-workers"
                | "--priority"
                | "--warning-mode"
                | "--include-build"
        ),
        PackRunner::DotnetTest => matches!(
            arg,
            "-s" | "--settings"
                | "-e"
                | "--environment"
                | "--filter"
                | "--test-adapter-path"
                | "-l"
                | "--logger"
                | "-o"
                | "--output"
                | "--artifacts-path"
                | "-d"
                | "--diag"
                | "--results-directory"
                | "--collect"
                | "--blame-crash-dump-type"
                | "--blame-hang-dump-type"
                | "--blame-hang-timeout"
                | "-c"
                | "--configuration"
                | "-f"
                | "--framework"
                | "-r"
                | "--runtime"
                | "-v"
                | "--verbosity"
                | "-a"
                | "--arch"
                | "--os"
        ),
        PackRunner::RSpec => matches!(
            arg,
            "-I" | "-r"
                | "--require"
                | "-O"
                | "--options"
                | "--order"
                | "--seed"
                | "--failure-exit-code"
                | "--error-exit-code"
                | "--drb-port"
                | "-f"
                | "--format"
                | "-o"
                | "--out"
                | "--deprecation-out"
                | "-P"
                | "--pattern"
                | "--exclude-pattern"
                | "-e"
                | "--example"
                | "-E"
                | "--example-matches"
                | "-t"
                | "--tag"
                | "--default-path"
        ),
        // `ruby -Itest -rfoo test/x_test.rb -i /pat/`: ruby's own -I and -r
        // attach their operand, but the separated spelling is legal too.
        PackRunner::Minitest => matches!(
            arg,
            "-I" | "-r"
                | "-i"
                | "--include"
                | "-n"
                | "--name"
                | "-e"
                | "--exclude"
                | "-x"
                | "-s"
                | "--seed"
                | "-S"
                | "--skip"
        ),
        PackRunner::JUnitXml => false,
    }
}
