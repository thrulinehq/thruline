//! Both directions of the narrowing detector, for every flag it carries.
//!
//! # The direction that costs more
//!
//! A missed narrowing flag lets a subset run support a `scope: all` claim. An
//! invented one refuses an honest full-suite run — and §6.3 raises that finding
//! to `Severity::Fail` under a `scope: all` claim, so it does not merely warn,
//! it overturns the verdict. `verify`'s own comment on the bare-path rule names
//! the failure: *"reading it as a test path made every full `cargo test` run
//! look narrowed — the direction that refuses honest evidence."*
//!
//! So every test here is a **pair**: an argv that must narrow and the nearest
//! argv that must not. A one-sided test would pass for a function that returns
//! `Some` unconditionally.
//!
//! # The table checks itself
//!
//! `every_flag_has_a_pair` walks [`NARROWING_FLAGS`] and fails if a spelling is
//! added without a case here, so the table cannot grow past its own coverage —
//! and `no_flag_fires_for_another_runner` walks the cross product, which is the
//! test that would have caught `-t` being shared between Gradle's continuous
//! build, dotnet's `--list-tests` and RSpec's `--tag`.

use thruline_langpacks::narrowing::{
    narrowing_flag, narrowing_flag_from_verify, NARROWING_FLAGS, NOT_NARROWING, SELECTOR_FORMS,
};
use thruline_langpacks::runners::{PackRunner, Runner};

fn argv(cmd: &str) -> Vec<String> {
    cmd.split_whitespace().map(str::to_string).collect()
}

#[track_caller]
fn narrows(runner: PackRunner, cmd: &str) {
    assert!(
        narrowing_flag(runner, &argv(cmd)).is_some(),
        "`{cmd}` narrows the {} run and was not detected",
        runner.as_str()
    );
}

#[track_caller]
fn full(runner: PackRunner, cmd: &str) {
    assert_eq!(
        narrowing_flag(runner, &argv(cmd)),
        None,
        "`{cmd}` is a full {} run and was reported as narrowed — the direction \
         that refuses honest evidence",
        runner.as_str()
    );
}

// ---------------------------------------------------------------------------
// The regression the whole detector exists for: a full suite, per runner.
// ---------------------------------------------------------------------------

#[test]
fn a_full_run_of_each_runner_is_not_narrowed() {
    full(PackRunner::GoTest, "go test ./...");
    full(PackRunner::Maven, "mvn test");
    full(PackRunner::Gradle, "gradle test");
    full(PackRunner::DotnetTest, "dotnet test");
    full(PackRunner::RSpec, "rspec");
    full(PackRunner::Minitest, "rake test");
}

/// The same six with the noise a real CI line carries. None of it selects
/// anything, and all of it is on commands people actually run.
#[test]
fn a_full_run_with_routine_ci_noise_is_not_narrowed() {
    full(PackRunner::GoTest, "go test -count=1 -race -v ./...");
    full(PackRunner::GoTest, "go test -count 1 -timeout 20m ./...");
    full(PackRunner::Maven, "mvn -B -q -o clean test");
    full(PackRunner::Gradle, "gradle test --console=plain --offline");
    full(
        PackRunner::DotnetTest,
        "dotnet test -c Release --nologo -v minimal",
    );
    full(PackRunner::RSpec, "rspec -f documentation --order random");
    full(PackRunner::Minitest, "rake test A=-v");
}

// ---------------------------------------------------------------------------
// go test
// ---------------------------------------------------------------------------

#[test]
fn go_run_and_skip_narrow_in_every_spelling_go_accepts() {
    // go's flag package takes one or two leading dashes for the same flag;
    // `go test --run=TestOne -v ./...` was run on go1.27.1 and executed one of
    // two tests where the unflagged run executed both.
    narrows(PackRunner::GoTest, "go test -run TestOne ./...");
    narrows(PackRunner::GoTest, "go test -run=TestOne ./...");
    narrows(PackRunner::GoTest, "go test --run=TestOne ./...");
    narrows(PackRunner::GoTest, "go test -skip=TestSlow ./...");
    // The compiled binary's own spelling, forwarded unchanged. Nothing in
    // verify's flat list resembles it; this one was measured, not assumed.
    narrows(PackRunner::GoTest, "go test -test.run=TestOne ./...");
    narrows(PackRunner::GoTest, "go test -test.skip=TestSlow ./...");
    narrows(PackRunner::GoTest, "go test -list . ./...");
    narrows(PackRunner::GoTest, "go test -test.list . ./...");

    full(PackRunner::GoTest, "go test -v ./...");
    // -short only sets testing.Short(); any skip it causes lands in the tally.
    full(PackRunner::GoTest, "go test -short ./...");
    // -bench selects benchmarks and leaves the ordinary tests running in full.
    full(PackRunner::GoTest, "go test -bench=. ./...");
    // -count=0 does run nothing, but it emits no PASS lines, so §6.3's
    // tests_total == 0 clause answers first. -count=1 is the honest idiom and
    // is the reason -count is not in the table at all.
    full(PackRunner::GoTest, "go test -count=1 ./...");
}

#[test]
fn go_package_selectors_narrow_but_the_module_root_does_not() {
    narrows(PackRunner::GoTest, "go test ./internal/...");
    narrows(PackRunner::GoTest, "go test ./pkg/parser");
    narrows(PackRunner::GoTest, "go test .");

    full(PackRunner::GoTest, "go test ./...");
    full(PackRunner::GoTest, "go test all");
    // A bare `go test` is package-local mode. It IS narrower than the module
    // whenever the module holds more than one package — and argv cannot say
    // whether it does. Guessing here would refuse the most common honest
    // invocation in the language on every one-package repository, so this is a
    // deliberate, named blind spot rather than a bug.
    full(PackRunner::GoTest, "go test");
    // An operand must not be read as a package.
    full(PackRunner::GoTest, "go test -outputdir ./cov ./...");
}

// ---------------------------------------------------------------------------
// mvn / Surefire / Failsafe
// ---------------------------------------------------------------------------

#[test]
fn maven_surefire_and_failsafe_selectors_narrow() {
    narrows(PackRunner::Maven, "mvn test -Dtest=UserServiceTest");
    narrows(
        PackRunner::Maven,
        "mvn test -Dtest=UserServiceTest#happyPath",
    );
    narrows(PackRunner::Maven, "mvn verify -Dit.test=UserServiceIT");
    narrows(PackRunner::Maven, "mvn test -Dgroups=fast");
    narrows(PackRunner::Maven, "mvn test -DexcludedGroups=slow");
    narrows(
        PackRunner::Maven,
        "mvn test -Dsurefire.includes=**/Fast*.java",
    );
    narrows(
        PackRunner::Maven,
        "mvn test -Dsurefire.excludes=**/Slow*.java",
    );
    narrows(
        PackRunner::Maven,
        "mvn test -Dsurefire.includesFile=only.txt",
    );
    narrows(
        PackRunner::Maven,
        "mvn test -Dsurefire.excludesFile=skip.txt",
    );
    narrows(PackRunner::Maven, "mvn verify -DskipTests");
    narrows(PackRunner::Maven, "mvn verify -Dmaven.test.skip=true");
    narrows(PackRunner::Maven, "mvn verify -Dmaven.test.skip.exec=true");

    full(PackRunner::Maven, "mvn clean test");
    full(PackRunner::Maven, "mvn -Dmaven.compiler.release=17 test");
    // A property whose name merely starts with `test`.
    full(PackRunner::Maven, "mvn test -DtestcontainersReuse=true");
    // The suppressor, not the selector. A multi-module reactor legitimately
    // sets it so a module with no matching test does not break the build; it
    // narrows nothing on its own, and paired with -Dtest= that flag fires.
    full(
        PackRunner::Maven,
        "mvn test -Dsurefire.failIfNoSpecifiedTests=false",
    );
}

#[test]
fn maven_reactor_narrowing_is_the_flag_not_the_goal() {
    narrows(PackRunner::Maven, "mvn -pl core test");
    narrows(PackRunner::Maven, "mvn --projects core,api test");
    narrows(PackRunner::Maven, "mvn -N test");
    narrows(PackRunner::Maven, "mvn --non-recursive test");
    narrows(PackRunner::Maven, "mvn -rf :api test");
    narrows(PackRunner::Maven, "mvn --resume-from :api test");

    // Goals and phases are positional, and none of them is a selector — the
    // rule verify's bare-path arm gets wrong for this runner, since a goal like
    // `org.apache.maven.plugins:maven-surefire-plugin:test` contains both a
    // dot and a colon.
    full(PackRunner::Maven, "mvn clean verify");
    full(
        PackRunner::Maven,
        "mvn org.apache.maven.plugins:maven-surefire-plugin:test",
    );
    // An alternate POM is as often the only POM as it is one module of many.
    full(PackRunner::Maven, "mvn -f backend/pom.xml test");
}

// ---------------------------------------------------------------------------
// gradle
// ---------------------------------------------------------------------------

#[test]
fn gradle_test_filters_narrow() {
    narrows(
        PackRunner::Gradle,
        "gradle test --tests com.example.UserTest",
    );
    narrows(PackRunner::Gradle, "gradle check -x integrationTest");
    narrows(PackRunner::Gradle, "gradle test --exclude-task slowTest");
    // Measured: on Gradle 9.7.1 this reports every test SKIPPED with a green
    // build and writes tests="2" skipped="2" failures="0". §6.3's skip clause
    // is only a warn and its conjunction never requires passed == total, so
    // without this entry a run that executed nothing is `supported`.
    narrows(PackRunner::Gradle, "gradle test --test-dry-run");
    narrows(PackRunner::Gradle, "gradle -m test");
    narrows(PackRunner::Gradle, "gradle --dry-run test");

    full(PackRunner::Gradle, "gradle test");
    full(PackRunner::Gradle, "gradle check");
    full(PackRunner::Gradle, "gradle test --rerun-tasks --continue");
    // `-t` is Gradle's continuous build. It runs no less than the same command
    // without it. verify's flat list holds `-t`, and this is the entry it gets
    // wrong: an honest watch-mode run reported as a subset, at Fail severity
    // under a `scope: all` claim.
    full(PackRunner::Gradle, "gradle test -t");
    full(PackRunner::Gradle, "gradle test --continuous");
    full(PackRunner::Gradle, "gradle test --fail-fast");
}

#[test]
fn gradle_task_paths_narrow_but_bare_task_names_do_not() {
    narrows(PackRunner::Gradle, "gradle :core:test");
    narrows(PackRunner::Gradle, "gradle :okhttp:test :okcurl:test");

    full(PackRunner::Gradle, "gradle test");
    full(PackRunner::Gradle, "gradle build test");
}

// ---------------------------------------------------------------------------
// dotnet test
// ---------------------------------------------------------------------------

#[test]
fn dotnet_filter_narrows_and_environment_does_not() {
    narrows(PackRunner::DotnetTest, "dotnet test --filter Priority=1");
    narrows(
        PackRunner::DotnetTest,
        "dotnet test --filter=FullyQualifiedName~Serilog.Core",
    );
    narrows(PackRunner::DotnetTest, "dotnet test -t");
    narrows(PackRunner::DotnetTest, "dotnet test --list-tests");

    full(PackRunner::DotnetTest, "dotnet test");
    full(PackRunner::DotnetTest, "dotnet test --no-build --nologo");
    // .NET 9.0.317: `-e` is --environment. It is RSpec and minitest where -e
    // selects examples. Sharing one table across runners would refuse every
    // dotnet run that sets a variable.
    full(PackRunner::DotnetTest, "dotnet test -e CI=true");
    full(
        PackRunner::DotnetTest,
        "dotnet test --environment ASPNETCORE_ENVIRONMENT=Test",
    );
    // One target framework of a multi-targeted project is fewer executions,
    // not fewer distinct tests — and argv cannot tell it from a project that
    // only ever had one.
    full(PackRunner::DotnetTest, "dotnet test -f net8.0");
}

#[test]
fn dotnet_one_project_narrows_but_the_solution_does_not() {
    narrows(
        PackRunner::DotnetTest,
        "dotnet test src/Core.Tests/Core.Tests.csproj",
    );
    narrows(
        PackRunner::DotnetTest,
        "dotnet test bin/Debug/Core.Tests.dll",
    );

    full(PackRunner::DotnetTest, "dotnet test AutoMapper.sln");
    full(PackRunner::DotnetTest, "dotnet test");
    // An operand of a value-taking option is not a project path.
    full(
        PackRunner::DotnetTest,
        "dotnet test --logger trx --results-directory out",
    );
}

// ---------------------------------------------------------------------------
// rspec
// ---------------------------------------------------------------------------

#[test]
fn rspec_example_and_tag_selectors_narrow() {
    narrows(PackRunner::RSpec, "rspec -e signs_in_a_user");
    narrows(PackRunner::RSpec, "rspec --example signs_in");
    narrows(PackRunner::RSpec, "rspec -E ^User#save");
    narrows(PackRunner::RSpec, "rspec --example-matches ^User#save");
    narrows(PackRunner::RSpec, "rspec -t slow");
    narrows(PackRunner::RSpec, "rspec --tag ~slow");
    narrows(PackRunner::RSpec, "rspec --only-failures");
    narrows(PackRunner::RSpec, "rspec -n");
    narrows(PackRunner::RSpec, "rspec --next-failure");
    narrows(PackRunner::RSpec, "rspec -P spec/unit/**/*_spec.rb");
    narrows(PackRunner::RSpec, "rspec --pattern spec/unit/**/*_spec.rb");
    narrows(PackRunner::RSpec, "rspec --exclude-pattern spec/slow/**");
    narrows(PackRunner::RSpec, "rspec --bisect");
    // OptionParser accepts a short flag with its value attached.
    narrows(PackRunner::RSpec, "rspec -esigns_in");

    full(PackRunner::RSpec, "rspec");
    full(PackRunner::RSpec, "rspec --no-color -f progress");
    // -I is a load path. It is one ASCII case away from minitest's -i
    // (include), and verify's matcher lowercases every argument before
    // comparing, which collapses the two.
    full(PackRunner::RSpec, "rspec -I lib -r spec_helper");
    full(PackRunner::RSpec, "rspec --fail-fast");
    full(PackRunner::RSpec, "rspec --seed 1234 --order random");
}

#[test]
fn rspec_dry_run_narrows_because_its_output_is_indistinguishable_from_a_real_pass() {
    // Measured on rspec-core 3.13.6: a two-example suite prints
    // `2 examples, 0 failures` and exits 0 under --dry-run, byte-for-byte the
    // shape of a real passing run, with nothing marked pending. No output
    // parser can separate them, so argv is the only place this can be caught.
    narrows(PackRunner::RSpec, "rspec --dry-run");
    full(PackRunner::RSpec, "rspec --format progress");
}

#[test]
fn rspec_file_and_line_selectors_narrow() {
    narrows(PackRunner::RSpec, "rspec spec/models/user_spec.rb:42");
    narrows(PackRunner::RSpec, "rspec spec/models/user_spec.rb");
    narrows(PackRunner::RSpec, "rspec spec/models");
    narrows(PackRunner::RSpec, "rspec spec/a_spec.rb[1:5,1:6]");

    full(PackRunner::RSpec, "rspec");
    // The operand of a formatter option is not a spec path.
    full(PackRunner::RSpec, "rspec -f documentation");
    full(PackRunner::RSpec, "rspec --out tmp/rspec.txt");
}

// ---------------------------------------------------------------------------
// minitest
// ---------------------------------------------------------------------------

#[test]
fn minitest_include_and_exclude_narrow_in_both_generations_of_spelling() {
    // 6.0.6 documents -i/--include and -e/--exclude; -n, --name and -x survive
    // as aliases installed by `opts.alias` that --help does not print. Both
    // generations appear in the wild, so both are carried. This is the fact the
    // stage brief had wrong — it named -n/--name as the current spelling — and
    // reading the installed gem rather than transcribing the brief is what
    // caught it.
    narrows(
        PackRunner::Minitest,
        "ruby -Itest test/user_test.rb -i /save/",
    );
    narrows(
        PackRunner::Minitest,
        "ruby -Itest test/user_test.rb --include save",
    );
    narrows(
        PackRunner::Minitest,
        "ruby -Itest test/user_test.rb -n /save/",
    );
    narrows(
        PackRunner::Minitest,
        "ruby -Itest test/user_test.rb --name save",
    );
    narrows(
        PackRunner::Minitest,
        "ruby -Itest test/user_test.rb -e /slow/",
    );
    narrows(
        PackRunner::Minitest,
        "ruby -Itest test/user_test.rb --exclude slow",
    );
    narrows(
        PackRunner::Minitest,
        "ruby -Itest test/user_test.rb -x /slow/",
    );

    full(PackRunner::Minitest, "rake test");
    // -I is ruby's load path, not minitest's -i (include). Case is the only
    // thing between them, and verify's matcher lowercases every argument before
    // comparing, which collapses the two.
    full(PackRunner::Minitest, "rake test -Itest -Ilib");
    full(PackRunner::Minitest, "rake test A=-v");
    full(PackRunner::Minitest, "rake test A=--seed 4242");
    // Despite the name, -S/--skip suppresses *reporting* of result types, not
    // tests. Read out of the option's own description in lib/minitest.rb.
    full(PackRunner::Minitest, "rake test A=-S E");
}

#[test]
fn minitest_rake_assignments_narrow_and_a_bare_goal_does_not() {
    narrows(PackRunner::Minitest, "rake test N=/save/");
    narrows(PackRunner::Minitest, "rake test I=save");
    narrows(PackRunner::Minitest, "rake test X=/slow/");
    narrows(PackRunner::Minitest, "rake test E=slow");
    // The pass-through assignments are inspected, not assumed: this one is
    // carrying a selector.
    narrows(PackRunner::Minitest, "rake test A=-n /save/");
    narrows(PackRunner::Minitest, "rake test TESTOPTS=--include save");
    narrows(PackRunner::Minitest, "rake test FILTER=-x /slow/");

    full(PackRunner::Minitest, "rake test");
    full(PackRunner::Minitest, "rake test TESTOPTS=-v");
    full(PackRunner::Minitest, "rake test:units");
}

#[test]
fn minitest_one_file_narrows_where_the_rake_task_does_not() {
    narrows(PackRunner::Minitest, "ruby -Itest test/models/user_test.rb");
    full(PackRunner::Minitest, "rake test");
}

// ---------------------------------------------------------------------------
// Runner-awareness: the property, not the anecdote.
// ---------------------------------------------------------------------------

/// The argv prefix a realistic invocation of each runner starts with.
///
/// It matters: `narrowing_flag` skips `argv[0]` and treats a runner's
/// subcommand as structural, so a synthetic argv missing them is not the argv
/// the detector will ever see. An earlier version of the harness pushed the
/// word `test` in front of every runner, and for RSpec — which has no
/// subcommand — that word became a named spec path and failed the exclusion
/// test for a reason that had nothing to do with the flag under test.
fn prefix(runner: PackRunner) -> Vec<String> {
    match runner {
        PackRunner::GoTest => argv("go test"),
        PackRunner::Maven => argv("mvn test"),
        PackRunner::Gradle => argv("gradle test"),
        PackRunner::DotnetTest => argv("dotnet test"),
        PackRunner::RSpec => argv("rspec"),
        PackRunner::Minitest => argv("rake test"),
        PackRunner::JUnitXml => argv("junit-xml"),
    }
}

/// No spelling in the table fires for a runner it does not belong to.
///
/// This is the whole reason the table is keyed by runner. `-t` alone means
/// `--tag` (RSpec, narrows), `--list-tests` (dotnet, narrows) and `--continuous`
/// (Gradle, does not) — and `-e` means `--example` (RSpec), `--exclude`
/// (minitest) and `--environment` (dotnet, does not).
#[test]
fn no_flag_fires_for_another_runner() {
    let runners = [
        PackRunner::GoTest,
        PackRunner::Maven,
        PackRunner::Gradle,
        PackRunner::DotnetTest,
        PackRunner::RSpec,
        PackRunner::Minitest,
    ];
    for entry in NARROWING_FLAGS {
        for spelling in entry.spellings {
            // A rake-style assignment needs its `=` to be an argument at all.
            let arg = if spelling.starts_with('-') {
                (*spelling).to_string()
            } else {
                format!("{spelling}=x")
            };
            for other in runners {
                if other == entry.runner {
                    continue;
                }
                // Only assert the negative where the *other* runner's own table
                // does not legitimately claim the same spelling: `-e` narrows
                // for both RSpec and minitest, and `--dry-run` for both RSpec
                // and Gradle. Those are agreements, not leaks.
                let claimed_by_other = NARROWING_FLAGS
                    .iter()
                    .filter(|f| f.runner == other)
                    .any(|f| f.spellings.contains(spelling));
                if claimed_by_other {
                    continue;
                }
                // The one place this property genuinely does not hold, stated
                // rather than worked around. RSpec and minitest are parsed by
                // Ruby's OptionParser, whose short options are all one letter
                // and take their value attached, so for those two runners a
                // single-dash multi-character token IS a short flag plus a
                // value. Measured on rspec-core 3.13.6: `rspec -test.run`
                // against a passing two-example suite reports `0 examples` —
                // OptionParser read it as `-t est.run`, a tag filter, exactly
                // as it reads `-tslow`. Firing there is correct, so asserting
                // silence would be asserting a bug.
                let ruby = matches!(other, PackRunner::RSpec | PackRunner::Minitest);
                if ruby && arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2 {
                    continue;
                }
                // The other side of the same coin: RSpec's positional grammar
                // is "any bare word is a spec file or directory", so minitest's
                // rake assignment `N=x` really is a path as far as RSpec is
                // concerned. Detecting it is right, not a leak.
                if other == PackRunner::RSpec && !arg.starts_with('-') {
                    continue;
                }
                let mut cmd = prefix(other);
                cmd.push(arg.clone());
                assert_eq!(
                    narrowing_flag(other, &cmd),
                    None,
                    "`{arg}` is {}'s flag and it fired for {}",
                    entry.runner.as_str(),
                    other.as_str()
                );
            }
        }
    }
}

/// The three collisions, spelled out, because a property test that stops
/// holding is easier to delete than a named case.
#[test]
fn the_three_way_collisions_resolve_per_runner() {
    // -t
    narrows(PackRunner::RSpec, "rspec -t slow");
    narrows(PackRunner::DotnetTest, "dotnet test -t");
    full(PackRunner::Gradle, "gradle test -t");
    full(PackRunner::Maven, "mvn -t toolchains.xml test");

    // -e
    narrows(PackRunner::RSpec, "rspec -e login");
    narrows(PackRunner::Minitest, "ruby test/x_test.rb -e /slow/");
    full(PackRunner::DotnetTest, "dotnet test -e CI=true");

    // --dry-run: Gradle disables every task action, RSpec prints a passing
    // tally for a suite it never ran. Both narrow; neither is go's.
    narrows(PackRunner::Gradle, "gradle --dry-run test");
    narrows(PackRunner::RSpec, "rspec --dry-run");
}

// ---------------------------------------------------------------------------
// The table cannot outgrow its own coverage.
// ---------------------------------------------------------------------------

/// Every spelling in the table is exercised by a `narrows` case above.
///
/// Without this, adding a flag and forgetting the test is silent, and a table
/// entry nothing exercises is indistinguishable from one that does not work.
#[test]
fn every_flag_in_the_table_is_covered_by_a_case_here() {
    let source = include_str!("narrowing_flags.rs");
    let mut uncovered: Vec<String> = Vec::new();
    for entry in NARROWING_FLAGS {
        for spelling in entry.spellings {
            // Whole-token match. A substring check would let `-Dtest` be
            // "covered" by the `-DtestcontainersReuse` case that exists to
            // prove it is NOT matched, and `-list` by `--list-tests`.
            let covered = source.match_indices(spelling).any(|(at, _)| {
                let before = source[..at].chars().next_back();
                let after = source[at + spelling.len()..].chars().next();
                let boundary = |c: Option<char>| {
                    !matches!(c, Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
                };
                // `-Dtest` inside `-Dtest=X` is a whole token; inside
                // `-DtestcontainersReuse` it is not.
                let after_ok = boundary(after) || after == Some('=');
                boundary(before) && after_ok && before != Some('-')
            });
            if !covered {
                uncovered.push(format!("{} {}", entry.runner.as_str(), spelling));
            }
        }
    }
    assert!(
        uncovered.is_empty(),
        "spellings in NARROWING_FLAGS with no case in this file: {uncovered:?}"
    );
}

/// Every deliberate exclusion is still excluded.
///
/// [`NOT_NARROWING`] is the written-down half of the inclusion rule. If one of
/// these starts firing, an honest full-suite run has started being refused, and
/// this is the test that says so rather than a user discovering it.
#[test]
fn every_deliberate_exclusion_still_returns_none() {
    for entry in NOT_NARROWING {
        for spelling in entry.spellings {
            let mut cmd = prefix(entry.runner);
            cmd.push((*spelling).to_string());
            // A trailing bare word stands in for the flag's operand, so the
            // "operand misread as a selector" failure is exercised too — except
            // for RSpec, whose positional grammar is *any* bare word is a spec
            // file. There, a trailing word is a genuine named path and
            // detecting it would be correct; the operand cases for RSpec are
            // spelled out by hand instead (`rspec -f documentation`).
            if entry.runner != PackRunner::RSpec {
                cmd.push("value".to_string());
            }
            assert_eq!(
                narrowing_flag(entry.runner, &cmd),
                None,
                "{} `{spelling}` is documented as not narrowing — {} — but it fired",
                entry.runner.as_str(),
                entry.why
            );
        }
    }
}

/// Every no-flag selector form narrows, and its paired full run does not.
#[test]
fn every_selector_form_narrows_and_its_pair_does_not() {
    for form in SELECTOR_FORMS {
        // Maven's row is the one whose narrowing side is a flag (`-pl`) rather
        // than a positional; it is in the table because the pair is what
        // documents that Maven positionals are goals and never selectors.
        assert!(
            narrowing_flag(form.runner, &argv(form.narrows)).is_some(),
            "`{}` narrows and was not detected",
            form.narrows
        );
        assert_eq!(
            narrowing_flag(form.runner, &argv(form.does_not_narrow)),
            None,
            "`{}` is a full run and was reported as narrowed",
            form.does_not_narrow
        );
    }
}

// ---------------------------------------------------------------------------
// The verify-typed adapter.
// ---------------------------------------------------------------------------

/// The adapter answers for the runners `verify::parse::Runner` can name, and
/// says nothing about the ones it cannot reach.
#[test]
fn the_verify_typed_adapter_maps_the_runners_verify_can_name() {
    assert!(narrowing_flag_from_verify(Runner::GoTest, &argv("go test -run X ./...")).is_some());
    assert!(narrowing_flag_from_verify(Runner::Maven, &argv("mvn test -Dtest=X")).is_some());
    assert!(narrowing_flag_from_verify(Runner::Gradle, &argv("gradle test --tests X")).is_some());
    assert!(
        narrowing_flag_from_verify(Runner::DotnetTest, &argv("dotnet test --filter X")).is_some()
    );
    assert_eq!(
        narrowing_flag_from_verify(Runner::GoTest, &argv("go test ./...")),
        None
    );

    // Pytest, cargo, jest and mocha belong to verify's own list. `None` here
    // means "not this crate's question", never "not narrowed", and a caller
    // that reads it the second way loses the detector for four runners.
    assert_eq!(
        narrowing_flag_from_verify(Runner::Pytest, &argv("pytest -k test_login")),
        None
    );
    assert_eq!(
        narrowing_flag_from_verify(Runner::CargoTest, &argv("cargo test --exact foo")),
        None
    );
    // A JUnit XML report is a file on disk, not a process; there is no argv of
    // its own to read, and answering from someone else's would be an answer
    // about some other run.
    assert_eq!(
        narrowing_flag_from_verify(Runner::JUnitXml, &argv("anything --filter x")),
        None
    );
}

// ---------------------------------------------------------------------------
// The guard against passing by not running
// ---------------------------------------------------------------------------

/// **Every table-walking test in this file asserts nothing if its table is
/// empty.** This is the guard that makes them mean something.
///
/// Track Z's DECISIONS #177 census listed this file as a candidate for "a check
/// that passes by not running". Read, the mechanism it named — an early return
/// on a missing `env::var`, fixture or binary — is not present: this file
/// touches no environment, spawns no process and reads no fixture. Every
/// `continue` in it is a **deliberate, measured exclusion** inside a cross
/// product, each with its reason written beside it.
///
/// The hole is real and it is a different one: **nothing asserted the tables
/// were populated.** `NARROWING_FLAGS` empty would make every `for` over it
/// iterate zero times, and a file of green tests would assert nothing at all.
/// That is the same empty-search-space failure this repository refuses for its
/// own deny list, and the reason the census's instinct was right even though
/// its stated mechanism was not.
///
/// The correction is a guard rather than an `#[ignore]`: these tests *do* run
/// and *do* assert real behaviour today, so marking them unrun would be filing
/// a true check as an untrue one.
#[test]
fn the_tables_these_tests_walk_are_populated() {
    assert!(
        NARROWING_FLAGS.len() >= 30,
        "NARROWING_FLAGS holds {} entries. Every table-walking test in this file \
         iterates it, so a short table makes them pass while checking almost \
         nothing. If flags were deliberately removed, lower this bound in the \
         same commit and say why.",
        NARROWING_FLAGS.len()
    );
    assert!(
        !SELECTOR_FORMS.is_empty(),
        "SELECTOR_FORMS is empty — the no-flag narrowing cases assert nothing"
    );
    assert!(
        !NOT_NARROWING.is_empty(),
        "NOT_NARROWING is empty — `every_deliberate_exclusion_still_returns_none` \
         walks it and would pass having checked no exclusion at all"
    );

    // Every runner this crate reaches must actually appear in the table.
    // A table of 30 entries that all named one runner would clear the bound
    // above and still leave five runners unchecked.
    for runner in [
        PackRunner::GoTest,
        PackRunner::Maven,
        PackRunner::Gradle,
        PackRunner::DotnetTest,
        PackRunner::RSpec,
        PackRunner::Minitest,
    ] {
        assert!(
            NARROWING_FLAGS.iter().any(|f| f.runner == runner),
            "no NARROWING_FLAGS entry names {runner:?}, so every assertion about \
             that runner's flags is vacuous"
        );
    }
}

/// And the cross-product walk must actually reach a comparison.
///
/// `no_flag_fires_for_another_runner` is the widest test here and the one whose
/// `continue`s could, between them, skip every case. It exercises
/// flags × other-runners; if the exclusions ever grew to cover the whole
/// product, it would pass having compared nothing.
#[test]
fn the_cross_product_walk_reaches_a_real_comparison() {
    let runners = [
        PackRunner::GoTest,
        PackRunner::Maven,
        PackRunner::Gradle,
        PackRunner::DotnetTest,
        PackRunner::RSpec,
        PackRunner::Minitest,
    ];
    let mut compared = 0usize;
    for entry in NARROWING_FLAGS {
        for spelling in entry.spellings {
            for other in runners {
                if other == entry.runner {
                    continue;
                }
                let claimed = NARROWING_FLAGS
                    .iter()
                    .filter(|f| f.runner == other)
                    .any(|f| f.spellings.contains(spelling));
                if !claimed {
                    compared += 1;
                }
            }
        }
    }
    assert!(
        compared >= 100,
        "the cross product reaches only {compared} comparisons. The exclusions in \
         `no_flag_fires_for_another_runner` are documented and measured, but if \
         they ever cover the product the test passes having compared nothing."
    );
}
