# Runner output fixtures

**Captured from real runs on this machine**, not written from documentation.
Provenance, so a later reader can tell a capture from a shape.

The convention is `crates/verify/tests/fixtures/runners/README.md`'s, and it
exists there because of a specific failure: a `pytest -q` parser was "proven"
against a hand-written fixture carrying a `=====` banner pytest does not emit,
and the first real captured run came back `unverifiable`. A parser proven
against output its runner may not produce is not proven.

`verify` also had to name a gap it could not close — *"`go test` has no capture
here"*. Every toolchain named in this directory was installed before capture,
so nothing here is a documented shape.

Each row names the exact command, the tool version, and **the exit code the real
run produced**. The exit code is part of the evidence: several of these outputs
are distinguishable from a clean green run by nothing else — and a few are not
distinguishable even then, which is recorded per language below.


## Ruby

Toolchain, as reported by the binaries that produced these bytes:

    ruby      4.0.6 (2026-07-14 revision 03b6d3f889) +PRISM [arm64-darwin25]
    rspec     3.13 — rspec-core 3.13.6, rspec-expectations 3.13.5,
                     rspec-mocks 3.13.8, rspec-support 3.13.7
    minitest  6.0.6   (run as `ruby -Ilib -Itest <file>`, no rake, no bundler)

This section is two passes. The tables immediately below are the first: every
command in them ran in a throwaway project built for the capture and kept out of
the repo. The second pass starts at *Second pass — the wrappers*, adds the
wrapper and configuration-driven states, and is the only place where a command
was run inside a real corpus checkout. The `exit` column is not a memory: 30 of these 31 fixtures were
regenerated in a single scripted pass that re-ran each command and printed the
code it returned, and the 31st (`rspec-pending-fixed.txt`, whose project was
split out last) was captured on its own and read the same way. That script lived
in the throwaway capture tree and is **not** preserved here, so reproducing
these bytes means rebuilding the projects from the descriptions in the tables.
The projects are all one `Calc` module with `add`/`sub`/`div`, so the same
arithmetic appears under both runners and a difference between the two tables is
a runner difference and not a fixture difference.

`--seed 20260905` is passed to every minitest run because minitest randomises
order and prints the seed it used; pinning it makes the captures reproducible
byte-for-byte apart from timings. RSpec's `--no-color` is passed wherever colour
was not the point — see the note on `rspec-syntax-error.txt`, where it did not
work.

### RSpec

| file | produced by | exit |
|---|---|---|
| `rspec-all-pass.txt` | `rspec --no-color` on a 6-example green suite — rspec-core 3.13.6. The default formatter **is** `progress`, so this and every other `.txt` row without a `--format` is a `--format progress` capture. | 0 |
| `rspec-documentation.txt` | `rspec --no-color --format documentation`, same green suite | 0 |
| `rspec-mixed.txt` | `rspec --no-color` — 9 examples, 2 failed expectations and 1 escaped `NoMethodError` | 1 |
| `rspec-documentation-mixed.txt` | `rspec --no-color --format documentation`, same mixed suite | 1 |
| `rspec-report.json` | `rspec --no-color --format json`, same mixed suite | 1 |
| `rspec-ansi-color.txt` | `rspec --force-color`, same mixed suite — real SGR codes around every `.`/`F`, around `Failure/Error:`, and around each line of the `Failed examples:` block | 1 |
| `rspec-fail-fast.txt` | `rspec --no-color --fail-fast`, same mixed suite — stops at the first failure and reports `6 examples, 1 failure` for a suite that has 9 examples and 3 failures | 1 |
| `rspec-pending-skipped.txt` | `rspec --no-color` on a suite using `xit`, `skip "msg"` and `pending "msg"` | 0 |
| `rspec-report-pending.json` | `rspec --no-color --format json`, same pending suite | 0 |
| `rspec-pending-fixed.txt` | `rspec --no-color` on the same suite with the `pending` example's expectation made to pass | 1 |
| `rspec-zero-tests.txt` | `rspec --no-color` where `spec/nothing_spec.rb` loads and declares no examples | 0 |
| `rspec-no-spec-files.txt` | `rspec --no-color` on an empty `spec/` directory | 0 |
| `rspec-load-error.txt` | `rspec --no-color` where one spec file `require`s a file that does not exist | 1 |
| `rspec-report-load-error.json` | `rspec --no-color --format json`, same load error | 1 |
| `rspec-syntax-error.txt` | `rspec --no-color` where one spec file is missing an `end` | 1 |
| `rspec-suite-failed-tests-green.txt` | `rspec --no-color` where every example passes and an `after(:suite)` hook raises `NameError` | 1 |
| `rspec-dry-run.txt` | `rspec --no-color --dry-run` over the **mixed** (3-failing) suite | 0 |
| `rspec-dry-run.json` | `rspec --no-color --format json --dry-run` over the same 3-failing suite | 0 |
| `rspec-narrowed-example.txt` | `rspec --no-color -e "adds two positive integers"` over the mixed suite | 0 |
| `rspec-narrowed-line.txt` | `rspec --no-color spec/calc_add_spec.rb:7` over the mixed suite | 0 |
| `rspec-narrowed-tag.txt` | `rspec --no-color --tag '~slow'` over the pending suite | 0 |
| `rspec-narrowed-file.txt` | `rspec --no-color spec/calc_add_spec.rb` over the mixed suite | 0 |

### minitest

| file | produced by | exit |
|---|---|---|
| `minitest-all-pass.txt` | `ruby -Ilib -Itest test/test_calc.rb --seed 20260905` — minitest 6.0.6, ruby 4.0.6 | 0 |
| `minitest-verbose.txt` | `ruby -Ilib -Itest test/test_calc.rb --seed 20260905 --verbose` | 0 |
| `minitest-mixed.txt` | `ruby -Ilib -Itest2 test2/test_calc_mixed.rb --seed 20260905` — 8 runs: 2 failures, 1 error, 2 skips | 1 |
| `minitest-skipped.txt` | `ruby -Ilib -Itest3 test3/test_calc_skipped.rb --seed 20260905` — `skip "msg"` and bare `skip` | 0 |
| `minitest-zero-tests.txt` | `ruby -Ilib -Itest4 test4/test_calc_empty.rb --seed 20260905` — a `Minitest::Test` subclass with no `test_` methods | 0 |
| `minitest-load-error.txt` | `ruby -Ilib -Itest5 test5/test_calc_stats.rb --seed 20260905` — `require` of a file that does not exist | 1 |
| `minitest-syntax-error.txt` | `ruby -Ilib -Itest6 test6/test_calc_syntax.rb --seed 20260905` — missing `end` | 1 |
| `minitest-suite-never-ran.txt` | `ruby -Ilib -Itest7 test7/test_calc_never_ran.rb --seed 20260905` — file requires `minitest/test` but not `minitest/autorun` | 0 |
| `minitest-narrowed.txt` | `ruby -Ilib -Itest test/test_calc.rb --seed 20260905 -n /divides/` | 0 |

### What these captures establish, that a parser written from the docs would miss

**RSpec's summary line is not a verdict, and this is the load-bearing fact for
Ruby.** Four separate captures print a benign-looking count for a run that
proved nothing:

- `rspec-dry-run.txt` is `9 examples, 0 failures` with **exit 0** over a suite
  in which three examples fail when actually executed. `rspec-dry-run.json`
  says the same: `failure_count: 0`, every example `"status": "passed"`, the
  same JSON key set as a real run. Neither the human format nor the
  machine-readable one carries any marker that `--dry-run` was in effect — the
  only difference from a genuine green run anywhere in the bytes is that
  `run_time` sits around `1e-06` instead of `1e-03`, which is a hint and not a
  discriminator. Anything that verifies RSpec from output alone can be defeated
  by six characters on the command line. **The first pass wrote here that "only
  the argv distinguishes these two files"; the second pass disproved it** —
  `rspec-dotrspec-dry-run.txt` is the same `--dry-run` with the flag moved into
  a committed `.rspec` file, so it is invisible in the argv *and* in the output.
  See the second-pass notes below.
- `rspec-load-error.txt`, `rspec-syntax-error.txt` and
  `rspec-suite-failed-tests-green.txt` all end in `0 failures` and are all
  failures. The tell is the clause after it — `, 1 error occurred outside of
  examples` — and in the JSON, `errors_outside_of_examples_count`. A parser that
  matches `(\d+) examples?, (\d+) failures?` and stops reads all three as clean.
  These are the captures that must parse to `Unverifiable`, never to `0 failed`.
- `rspec-fail-fast.txt` reports `6 examples, 1 failure` for a 9-example suite
  with 3 failures. The count is true and the suite is not what it describes.

**`--no-color` does not guarantee ANSI-free output.** `rspec-syntax-error.txt`
was captured with `--no-color` and still contains raw `ESC[1;3m` sequences,
because the `SyntaxError` is rendered by Ruby's own Prism error formatter, which
never sees RSpec's colour setting. `strip_ansi` is therefore not only needed on
the `--force-color` path; it is needed on the path a CI job would call
colour-free.

**RSpec cannot tell you that you have no specs.** `rspec-zero-tests.txt` (a spec
file that loads and declares nothing) and `rspec-no-spec-files.txt` (an empty
`spec/` directory) are the same bytes apart from timings: `No examples found.`
then `0 examples, 0 failures`, exit 0 for both. Both files are kept precisely
because they are indistinguishable — the sameness is the finding.

**Pending is not skipped, and a pending example prints a failure.**
`rspec-pending-skipped.txt` contains three `Failure/Error:` blocks under a
`Pending:` heading and still summarises as `0 failures, 3 pending`, exit 0.
`rspec-pending-fixed.txt` is the inverse trap: a `pending` example whose
expectation *passes* is reported as a **failure** (`FIXED`, `Expected pending
'...' to fail`). Counting `Failure/Error:` occurrences gets both files wrong, in
opposite directions.

**Three of RSpec's four narrowing forms announce themselves; the fourth does
not.** `-e`, `file:line` and `--tag` each print a `Run options:` banner naming
the filter (`include {full_description: ...}`, `include {locations: ...}`,
`exclude {slow: true}`). Passing a spec **file path** prints nothing at all:
`rspec-narrowed-file.txt` is four dots and `4 examples, 0 failures`, structurally
identical to a full green run of a four-example project. Narrowing detection for
RSpec cannot be output-only.

**minitest splits failures from errors, and a parser that reads one column
under-counts.** `minitest-mixed.txt` ends `8 runs, 5 assertions, 2 failures, 1
errors, 2 skips`. The escaped `NoMethodError` is in the `errors` column, not the
`failures` one; reading `failures` alone reports 2 of 3 real problems, and a
run whose only problem is an exception reports as green.

**minitest's silent zero.** `minitest-suite-never-ran.txt` **is 0 bytes, and
that is the capture, not a mistake.** The file requires `minitest/test` but not
`minitest/autorun`, so the `at_exit` hook that runs the suite is never
installed: the process defines a test class containing a deliberately false
assertion, runs nothing, prints nothing, and exits 0. Empty output plus exit 0
is the strongest possible "everything is fine" signal a shell can give, and it
is produced here by a suite that did not execute. `minitest-zero-tests.txt` is
the quieter cousin — a real run over a class with no `test_` methods, ending
`0 runs, 0 assertions, 0 failures, 0 errors, 0 skips`, exit 0.

**minitest self-declares its narrowing.** `Run options: --seed 20260905 -n
/divides/` is echoed on the first line of every run, so unlike RSpec's file-path
form, minitest's `-n` filter is visible in the output as well as the argv.

### Second pass — the wrappers, and the states the first pass named as gaps

The rows above are the runners invoked directly. Almost no real Ruby repository
invokes them that way: it goes through `bundle exec`, or through a `Rakefile`,
or through a committed `.rspec`. Those three wrappers were listed as gaps at the
end of the first pass, and this second pass closes them. Bundler resolved
offline (`bundle install --local`) against the gems already on the machine, so
no network was used here either.

    bundler   4.0.16
    rake      13.3.1

The `exit` column is again the code the real run returned, printed by a second
scripted pass in the throwaway tree (also not preserved here). All fifteen files
below run against the same `Calc` module and the same nine-example mixed suite
as the tables above, except the two `-corpus-` rows, which ran inside real
open-source checkouts.

| file | produced by | exit |
|---|---|---|
| `rspec-bundle-exec-mixed.txt` | `bundle exec rspec --no-color` — bundler 4.0.16, `Gemfile` pinning `rspec 3.13.2` + `rake 13.3.1`, resolved by `bundle install --local` | 1 |
| `rspec-bundle-missing-gem.txt` | `bundle exec rspec --no-color` with `gem "thruline_nonexistent_helper", "1.0.0"` added to the `Gemfile` | 1 |
| `rake-spec-pass.txt` | `rake spec_green` — `RSpec::Core::RakeTask` over a 4-example green pattern, rake 13.3.1 | 0 |
| `rake-spec-failed.txt` | `rake spec` — `RSpec::Core::RakeTask` over the mixed suite | 1 |
| `rake-test-mixed.txt` | `rake test` — `Rake::TestTask` over the minitest mixed suite | 1 |
| `rspec-dotrspec-dry-run.txt` | `rspec`, bare, in a project whose committed `.rspec` holds `--no-color` and `--dry-run` | 0 |
| `rspec-dotrspec-bypassed.txt` | `rspec --options /dev/null --no-color`, same project — the control that shows what that suite really does | 1 |
| `rspec-failure-exit-code-zero.txt` | `rspec --no-color` where `spec_helper.rb` sets `config.failure_exit_code = 0` | 0 |
| `rspec-focus-filter.txt` | `rspec --no-color` where `spec_helper.rb` sets `config.filter_run_when_matching :focus` and one example is tagged `:focus` | 0 |
| `rspec-only-failures.txt` | `rspec --no-color --only-failures`, after a seeding run wrote `.rspec_status` (`config.example_status_persistence_file_path`) | 1 |
| `rspec-flaky-first-run.txt` | `rspec --no-color` on a suite with one example that fails unless a marker file already exists | 1 |
| `rspec-flaky-second-run.txt` | the identical command, run again immediately | 0 |
| `rspec-corpus-missing-dev-deps.txt` | `rspec --no-color spec/rspec/core/formatters/progress_formatter_spec.rb` inside the **rspec-core** checkout in the corpus, with its dev dependencies not installed | 1 |
| `minitest-corpus-missing-dev-deps.txt` | `ruby -Ilib -Itest test/faker/default/test_faker_name.rb --seed 20260905` inside the **faker** checkout in the corpus, same condition | 1 |

### What the second pass establishes

**A committed `.rspec` file defeats argv inspection, and this is the strongest
Ruby finding in this directory.** `rspec-dotrspec-dry-run.txt` is 100 bytes —
nine dots, `9 examples, 0 failures`, exit 0 — produced by the single word
`rspec` in a directory whose `.rspec` contains `--dry-run`. Nothing ran.
`rspec-dotrspec-bypassed.txt` is the same project, the same suite, with
`--options /dev/null` added: three failures, exit 1, and it is byte-identical to
`rspec-mixed.txt` apart from timings. So the two files differ only by a
dotfile in the working tree. A verifier that reads output, exit code and argv
and nothing else is defeated by a two-line file, and the fix is not a better
parser — it is reading `.rspec` (and `SPEC_OPTS`, and `~/.rspec`, both of which
feed the same path and are **not** captured here).

**Exit code is not a verdict either.** `rspec-failure-exit-code-zero.txt` is
byte-identical to `rspec-mixed.txt` apart from timings — three visible
`Failure/Error:` blocks, `9 examples, 3 failures` — and the process exited **0**,
because `spec_helper.rb` sets `config.failure_exit_code = 0`. Paired with the
`.rspec` capture, the two together cover both halves: output can lie while the
exit code is honest, and the exit code can lie while the output is honest.

**`bundle exec` is invisible when it works and total when it doesn't.**
`rspec-bundle-exec-mixed.txt` is byte-identical to `rspec-mixed.txt` apart from
timings — bundler contributes nothing to a successful run, so no parser needs to
learn a "bundler mode". `rspec-bundle-missing-gem.txt` is the other extreme:
4 KB of `Bundler::GemNotFound` backtrace, not one line of RSpec output, exit 1.
It has no summary line, no example count, and no `error occurred outside of
examples` clause — nothing a Ruby test parser recognises at all. **This must
reach `Unverifiable`, and the reason it must is that it is what a tool sees on a
fresh checkout every time,** which makes it the single most likely real-world
input in this whole section.

**Rake's two task types fail differently, and the first pass guessed wrong about
one of them.** The first pass predicted `rake aborted!` framing for both.
`rake-test-mixed.txt` (`Rake::TestTask`) does end that way, with a rake
backtrace after minitest's own summary. `rake-spec-failed.txt`
(`RSpec::Core::RakeTask`) does **not**: it echoes the assembled `ruby ... exe/rspec
--pattern ...` command as its first line and, on failure, repeats that entire
command line with the word ` failed` appended as its last line — no `rake
aborted!` anywhere. Both task types put a long absolute command line **before**
the runner's first byte, so anything anchoring on the first line of output is
wrong under rake.

**`rake test` is not reproducible, and that is recorded rather than hidden.**
`Rake::TestTask` passes no `--seed`, so minitest randomises: three consecutive
runs printed seeds 18474, 37347 and 63433, and the progress markers came out
`SEF...FS` and differently again after. The captured file is one real run, not a
canonical one; the counts on the summary line are stable, the marker order is
not.

**Narrowing has a second invisible form, in the opposite direction from the
first.** The first pass found that a spec **file path** narrows without saying
so in the output. `rspec-focus-filter.txt` is the mirror image: the argv is bare
`rspec --no-color`, the narrowing lives in `spec_helper.rb`
(`filter_run_when_matching :focus`), and the *output* announces it —
`Run options: include {focus: true}`, one example, exit 0. `rspec-only-failures.txt`
behaves the same way, printing `Run options: include {last_run_status: "failed"}`
for a filter that is driven by a `.rspec_status` file on disk. So for RSpec,
neither the argv nor the output is individually sufficient to detect narrowing,
and the two miss in opposite directions.

**A real flake, captured as a pair.** `rspec-flaky-first-run.txt` (1 failure,
exit 1) and `rspec-flaky-second-run.txt` (green, 92 bytes, exit 0) are the same
command run twice in a row. Nothing in the second file records that the first
one happened — no "retried", no "flaky", no count of attempts. A green capture
is not evidence that the previous run was green, and RSpec gives a verifier no
way to tell the two apart from output alone.

**Neither corpus checkout could run its own suite, and the failure is the
capture.** `rspec-corpus-missing-dev-deps.txt` is rspec-core's own spec suite
loading against the system rspec: `LoadError: cannot load such file --
aruba/api`, then `0 examples, 0 failures, 1 error occurred outside of examples`.
`minitest-corpus-missing-dev-deps.txt` is faker's, dying on `require 'simplecov'`
before minitest's `autorun` hook is ever installed — a bare Ruby `LoadError`
backtrace with no minitest output whatsoever, not even `0 runs`. These are real
runs on real repositories in the corpus, and they are the shape a verifier meets
on any checkout it did not itself provision.

### States not captured, and why

- **No green run of any corpus repository.** This is the honest gap in the Ruby
  stage. The machine has only Ruby's default gems plus `rspec`; every corpus
  checkout (rails, rspec-core, sinatra, jekyll, puma, faker) needs dev
  dependencies — `simplecov`, `aruba`, `rack`, `timecop` and more — that are not
  installed, and installing them means a network fetch this stage did not make.
  So **every passing Ruby fixture here comes from a purpose-built toy project**,
  and the only corpus captures are the two failures above. Nothing in this
  section proves a parser against a large real suite's output, and no claim
  about scale (thousands of examples, multi-file summaries, `--format
  documentation` over deep nesting) is supported by anything in this directory.
- **No JUnit/XML report for either Ruby runner.** RSpec emits JUnit XML only
  through the third-party `rspec_junit_formatter` gem, and minitest only through
  `minitest-reporters`; neither is installed on this machine, and installing
  them means a network fetch, which this capture stage did not do. The XML rows
  in this directory come from the Java and .NET stages. Ruby's machine-readable
  coverage here is `rspec --format json` only.
- **No `SPEC_OPTS` or `~/.rspec` capture.** These feed the same option path as
  the `.rspec` file, so the invisible-`--dry-run` finding above almost certainly
  reproduces through both, but *almost certainly* is not a capture and neither
  was run.
- **No parallel-runner capture** (`parallel_tests`, `flatware`), which
  interleaves and then re-aggregates summaries. Not installed, not captured.
- **No `rspec-retry` capture.** The flake pair above is two separate processes;
  a retry *inside* one run, with `rspec-retry`'s `RSpec::Retry` reprinting an
  example, is a different shape and is not represented.
- **`bundle exec` covers only the two clean outcomes** — full resolution, and a
  gem missing entirely. A stale `Gemfile.lock`, a git-sourced gem, or a platform
  mismatch each produce their own bundler diagnostics, and none of those is
  here.
- **Truncation: none.** The largest Ruby fixture is `rspec-bundle-missing-gem.txt`
  at about 4 KB. Nothing in this section was truncated or edited after capture;
  every byte is what the process wrote, absolute scratch paths included.

## Go

`go version go1.27.1 darwin/arm64` (`/opt/homebrew/Cellar/go/1.27.1/libexec`).
Every row below is a capture. The purpose-built ones were run against throwaway
modules under the session scratchpad (`example.com/capture`, `example.com/broken`,
`example.com/vet`); the `corpus` ones were run against a real clone of
`github.com/spf13/cobra` and `github.com/stretchr/testify`.

This section is the one `crates/verify` said it could not write: *"there is no
Go toolchain on this machine, so its fixtures remain written from the documented
formats"*. There is one now, and four things below are not what the documented
format says.

The `exit` column is not a memory. Every command in the table was re-run in a
later session against the same throwaway tree, and the code recorded here is the
one that second run returned. That pass also re-read the fixtures: **all 26
purpose-built captures that existed at the time came back byte-identical** once
elapsed times were blanked, as did `gotest-corpus-summary-only.txt` and both
mutated-cobra rows. The four `-race`/`-shuffle` rows were captured after that
pass and so were never part of it; what was checked for them is stated where
they are described.
The single exception is `gotest-corpus-verbose.txt`, and it is not a drift — see
the reproducibility note below.

| file | produced by | exit |
|---|---|---|
| `gotest-all-pass.txt` | `go test -v -count=1 ./allpass/` — go1.27.1 | 0 |
| `gotest-mixed.txt` | `go test -v -count=1 ./mixed/` — 2 pass, 2 fail, 1 `t.Skip` | 1 |
| `gotest-skipped.txt` | `go test -v -count=1 -short ./skipped/` — `t.Skip`, `t.SkipNow` and `testing.Short()` all in one run | 0 |
| `gotest-no-test-files.txt` | `go test -count=1 ./notests/` — a package with no `_test.go` at all | 0 |
| `gotest-summary-only.txt` | `go test -count=1 ./allpass/` — the whole output is `ok \tpkg\t0.254s` | 0 |
| `gotest-cached.txt` | `go test ./allpass/ ./mixed/` on a warm cache — `ok \tpkg\t(cached)`, no elapsed time | 1 |
| `gotest-subtests.txt` | `go test -v -count=1 ./subtests/` — nested `--- FAIL`/`--- PASS` plus `=== PAUSE`/`=== CONT` from `t.Parallel()` | 1 |
| `gotest-benign-summary.txt` | `go test -v -count=1 ./benign/` — `TestMain` calls `os.Exit(0)` after `m.Run()`; a test FAILS, the package says `ok`, exit code 0 | 0 |
| `gotest-panic.txt` | `go test -v -count=1 ./panicking/` — nil deref: `--- FAIL` then a goroutine dump, exit 1 | 1 |
| `gotest-timeout.txt` | `go test -v -timeout 5s ./hang/` — `panic: test timed out after 5s`, no `--- FAIL` for the hung test | 1 |
| `gotest-build-failed.txt` | `go test -count=1 ./...` in `example.com/broken` — `FAIL\tpkg [build failed]` | 1 |
| `gotest-build-failed-verbose.txt` | `go test -v -count=1 ./...` in `example.com/broken` — one package build-failed, a second package's `--- PASS` in the same stream | 1 |
| `gotest-vet-failed.txt` | `go test -v -count=1 ./...` in `example.com/vet` — a `fmt.Printf` arg-count mismatch; `go test` runs vet before compiling and reports `[build failed]` | 1 |
| `gotest-setup-failed.txt` | `go test -count=1 ./nosuchpackage/` — `[setup failed]`, a third distinct "nothing ran" word | 1 |
| `gotest-narrowed.txt` | `go test -v -count=1 -run 'TestAdd$' ./allpass/` — 1 of 4 tests | 0 |
| `gotest-narrowed-no-match.txt` | `go test -v -count=1 -run TestNoSuchThing ./allpass/` — `testing: warning: no tests to run`, `PASS`, `ok`, exit 0 | 0 |
| `gotest-multipackage.txt` | `go test -count=1` over 6 packages — `ok`, `FAIL`, `?` and a trailing bare `FAIL` in one stream | 1 |
| `gotest-multipackage-verbose.txt` | the same run with `-v` | 1 |
| `gotest-cover.txt` | `go test -count=1 -cover` over the same 6 packages | 1 |
| `gotest-json.txt` | `go test -json -count=1 ./allpass/` | 0 |
| `gotest-json-mixed.txt` | `go test -json -count=1 ./mixed/` — `run`/`output`/`pass`/`fail`/`skip` events | 1 |
| `gotest-json-narrowed.txt` | `go test -json -count=1 -run 'TestAdd$' ./allpass/` | 0 |
| `gotest-json-multipackage.txt` | `go test -json -count=1` over 4 packages, including the `TestMain`-swallowed one | 1 |
| `gotest-json-build-failed.txt` | `go test -json -count=1 ./...` in `example.com/broken` — `build-output` and `build-fail` events | 1 |
| `gotest-ansi-from-test.txt` | `go test -v -count=1 ./ansiout/` — ANSI written by the test via `t.Logf` | 1 |
| `gotest-ansi-mixed-escapes.txt` | `go test -v -count=1 ./ansiraw/` — the same run carrying both ANSI encodings go1.27.1 produces (see below) | 1 |
| `gotest-race.txt` | `go test -race -v -count=1 ./racy/` — an unsynchronised `c.n++` across 8 goroutines | 1 |
| `gotest-race-bystander.txt` | `go test -race -v -count=1 ./leakrace/` — the race is in a goroutine that outlives the test that started it | 1 |
| `gotest-shuffle.txt` | `go test -shuffle=on -v -count=1 ./allpass/` — `-test.shuffle <seed>` ahead of the first `=== RUN` | 0 |
| `gotest-shuffle-failing.txt` | `go test -shuffle=on -count=1 ./mixed/` — **no `-v`**, and the seed line is printed anyway because the package failed | 1 |
| `gotest-corpus-verbose.txt` | `GOFLAGS=-mod=mod go test -v -count=1 ./...` in a clone of `github.com/spf13/cobra` — real third-party suite, all pass, 56 KB | 0 |
| `gotest-corpus-summary-only.txt` | `GOFLAGS=-mod=mod go test -count=1 ./...` in a clone of `github.com/stretchr/testify` — includes the `go: downloading …` lines the first run emitted | 0 |
| `gotest-corpus-mixed.txt` | the same cobra clone with a one-line regression applied (`args.go:109`, `len(args) != n` → `len(args) < n`), `go test -count=1 ./...` | 1 |
| `gotest-corpus-narrowed.txt` | the mutated cobra clone, `go test -count=1 -run TestExactArgs ./...` | 1 |

### Exit code separates almost nothing

Four fixtures in this table exit **0**: `gotest-all-pass.txt`,
`gotest-no-test-files.txt`, `gotest-narrowed-no-match.txt` and
`gotest-benign-summary.txt`. Exactly one of the four is a suite that ran and
passed. The second ran no tests because the package has none, the third ran none
because `-run` matched nothing, and the fourth contains a `--- FAIL` line. A
verifier that reads `go test`'s exit status and stops there calls all four green,
and §6.3's `tests_total > 0` clause is the only thing standing between the
middle two and a `supported` verdict.

### Four things measured here that the documented format does not say

1. **`go test` emits no ANSI, ever.** Confirmed on a real pty (`pty.spawn`) with
   `TERM=xterm-256color CLICOLOR_FORCE=1`, for both a passing run and a build
   failure: zero `0x1b` bytes in either. `go help testflag` has no colour flag.
   So there is no "force colour on" capture for this runner and there cannot be
   one — the colour in `gotest-ansi-*.txt` comes from the test binary, which is
   the only way ANSI reaches `go test` output in the field.
2. **The `testing` package doubles the ESC byte; `fmt` does not.** `t.Logf`
   output arrives as `1b 1b 5b 33 32 6d` — ESC ESC `[32m` — while a
   `fmt.Println` from inside the same test arrives as plain `1b 5b 33 32 6d`.
   Verified against the compiled test binary run directly, so this is `testing`,
   not the `go` command. `gotest-ansi-mixed-escapes.txt` carries both forms in
   one stream — 4 doubled `ESC ESC [` from `t.Logf`, 6 single `ESC [` from
   `fmt`, 14 `0x1b` bytes in all. This entry used to end *"an ANSI stripper
   matched on `ESC [` will leave a stray `0x1b` behind on every `t.Log` line"*.
   Run against this repo's own `strip_ansi` (`crates/verify/src/parse.rs`), it
   does not, and the truth is worse: the first ESC peeks, sees ESC rather than
   `[`, takes the two-character-escape arm and swallows **both** ESC bytes. Zero
   `0x1b` survive — so nothing keyed on that byte will ever fire — while the SGR
   body survives as ordinary text, `ansiraw_test.go:16: [36mvia t.Logf[0m`. The
   stripper reports a clean line and hands `[36m` downstream as content.
3. **A build failure and a passing package share one stream.**
   `gotest-build-failed-verbose.txt` contains `FAIL\t…/broken [build failed]`
   *and* `--- PASS: TestTwo`. A parser that tallies `--- ` lines reports a clean
   run over a package that never compiled. The `-json` form has the same trap:
   `gotest-json-build-failed.txt` carries `"Action":"build-fail"` on an object
   with an `ImportPath` and no `Test` key, alongside a test-level
   `"Action":"pass"`.
4. **`ok` does not mean the tests passed.** `gotest-benign-summary.txt` is a run
   whose per-test line says `--- FAIL`, whose package line says
   `ok \texample.com/capture/benign\t0.478s`, and whose exit code is 0 — because
   `TestMain` called `os.Exit(0)` after `m.Run()`. The `-json` form of the same
   package (in `gotest-json-multipackage.txt`) has a test-level
   `"Action":"fail"` under a package-level `"Action":"pass"`.

Two smaller ones worth naming. `gotest-cover.txt` contains a genuinely
interleaved line — `\texample.com/capture/notests\t\t--- FAIL: TestUpper …` —
where `go test` merged a `[no test files]` line and another package's failure
into one physical line. It is not a truncation; it is what the process wrote.
This was first described here as the `[no test files]` line *losing a race*, and
the re-run pass says that word was wrong: the same command reproduced the same
interleave at the same offset, so on this machine it is **deterministic**, and a
parser cannot wait it out. Whether it is deterministic anywhere else has not
been measured.

The reproducibility runs the other way in `gotest-corpus-verbose.txt`. Re-running
`go test -v -count=1 ./...` over the unmodified cobra clone produced a file of
exactly the same size carrying exactly the same multiset of lines, in a
**different order** — cobra's `TestCalledAs` drives its subtests from a Go map,
and map iteration order is randomised per run. So this one fixture is
reproducible only up to a permutation, and any check that diffs a fresh
`go test -v` capture against a stored one line-by-line will report a change that
is not one. And `go test`'s narrowing flag is **`-run`** (plus `-short`,
`-count`, `-shuffle`), none of which appear in `tests_pass::NARROWING_FLAGS`;
`gotest-narrowed*.txt` are the evidence that stage will need.

### What `-race` and `-shuffle` turned out to do

**`-race` blames a bystander.** `gotest-race.txt` is the easy case: the racy
test is the one that fails, via `testing.go:1865: race detected during execution
of test`. `gotest-race-bystander.txt` is the case a verifier has to survive. The
race lives in a goroutine that outlives the test that spawned it, so by the time
the detector fires, that test has already printed `--- PASS: TestLeaks`, and the
failure is charged to whichever test happened to be running — here
`TestSleepsPastTheRace`, whose entire body is a `time.Sleep`. **The guilty test
passes and an innocent one fails**, and nothing in the output says so. Stable
across 3 consecutive runs on this machine; not measured anywhere else. Anything
that maps a failing test name to a blamed source file will point at the wrong
file for this shape.

**`-shuffle=on` hides its own seed on success.** With `-v` the run opens with
`-test.shuffle 1788587878811169000`, a line that is neither a test line nor a
summary and precedes the first `=== RUN`. Without `-v` the seed appears **only
if the package fails** — `gotest-shuffle-failing.txt` carries it, and a passing
non-verbose shuffled run prints nothing but `ok \tpkg\t1.738s`, measured 3 times
with `grep -c shuffle` returning 0. So the runs whose order you would most want
to reproduce — the green ones — are the runs that do not tell you their order,
and `-shuffle` is invisible in the output of a passing non-verbose run.

### States not captured, and why

- **A machine-readable report file.** `go test` has no XML/TRX equivalent; its
  only structured form is the `-json` event stream on stdout, which is captured.
  There is no `gotest-report.xml` because the toolchain writes none. Converters
  exist (`gotestsum`, `go-junit-report`) and neither is installed here, so no
  fixture claims their output.
- **A failing run from an unmodified corpus repository.** cobra, testify and gin
  were all run to completion against their real test suites on this machine and
  all three passed. `gotest-corpus-mixed.txt` and `gotest-corpus-narrowed.txt`
  are therefore real runs over a real suite carrying a regression I introduced
  (named in the table), not a naturally failing upstream run.
- **A flaky/nondeterministic pair.** Nothing here is two runs of the same
  command with different *outcomes*; `gotest-cached.txt` is a same-outcome cache
  hit, not a flake. The closest thing measured is the subtest reordering in
  `gotest-corpus-verbose.txt` above — same verdict, different bytes — and only
  one of those two orderings is stored, so the pair itself is not a fixture.
- **`-race` and `-shuffle` output** — *this gap is closed*, and the four
  fixtures are in the table. It is left named here because closing it changed
  two things this section previously assumed; see "What `-race` and `-shuffle`
  turned out to do" above.
- **Truncation: none.** The largest Go fixture is `gotest-corpus-verbose.txt` at
  56 KB, under the ~60 KB bar, so nothing was cut. No fixture was edited after
  capture, and the re-run pass is the evidence — a hand-edited file would not
  have come back byte-identical from a fresh run; the goroutine dumps in `gotest-panic.txt` and `gotest-timeout.txt`
  and the `[setup failed]` line still carry the absolute scratchpad paths the
  process printed.


## C#

Toolchain, as reported by the binaries that produced these bytes:

    dotnet SDK   9.0.317   (commit 26570c2743, base path ~/.dotnet/sdk/9.0.317/)
    MSBuild      17.14.51+25f168cee
    host         9.0.19    (RID osx-arm64, macOS 26.1)
    VSTest       17.14.1 (arm64)      — printed by every run below
    Microsoft.NET.Test.Sdk  17.12.0   — in all three test projects
    xunit        2.9.2  + xunit.runner.visualstudio 2.8.2
    NUnit        4.2.2  + NUnit3TestAdapter 4.6.0
    MSTest       3.6.4

Every command ran in a throwaway project under a scratch directory kept out of
the repo; the projects are one `Shipping` class with `Rate(int)` and
`IsOversize(int)`, restated in each framework's dialect, so a difference between
the xunit, NUnit and MSTest rows is a framework difference and not a fixture
difference. The `exit` column is a measurement, not a memory: after the fixtures
were installed, all 21 commands were re-run in a scripted pass that printed the
code each returned, and the three TRX reports were regenerated and their
`<Counters>` elements compared against the shipped files byte-for-byte. They
matched.

The three `terminallogger` captures were taken through `script(1)` with the pty
sized by `stty rows 40 cols 160`, because that is the only way to make the SDK
believe it is talking to a terminal — see the first finding below, which is the
reason this section exists in the shape it does. Their first three bytes
(`\x04\b\b`) are `script`'s own artefact and not runner output.

| file | produced by | exit |
|---|---|---|
| `dotnet-pass.txt` | `dotnet test` on a 6-test green xunit project — dotnet SDK 9.0.317, VSTest 17.14.1, xunit 2.9.2 | 0 |
| `dotnet-mixed.txt` | `dotnet test` — 8 tests: 5 passed, 2 failed, 1 `[Fact(Skip=…)]` | 1 |
| `dotnet-all-skipped.txt` | `dotnet test` where all 3 tests carry `[Fact(Skip=…)]` | 0 |
| `dotnet-zero-tests.txt` | `dotnet test` on a test project that compiles and declares no `[Fact]` at all | 0 |
| `dotnet-compile-error.txt` | `dotnet test` where the test file is missing a `;` (CS1002) | 1 |
| `dotnet-narrowed.txt` | `dotnet test --filter "FullyQualifiedName~Oversize"` over the mixed suite — 3 of its 8 tests | 1 |
| `dotnet-narrowed-green.txt` | `dotnet test --filter "FullyQualifiedName~SmallParcel"` over the same 2-failure suite — 1 test, green | 0 |
| `dotnet-filter-no-match.txt` | `dotnet test --filter "FullyQualifiedName~ThisNameMatchesNothing"` over the mixed suite | 0 |
| `dotnet-testhost-crash.txt` | `dotnet test` where the third of five tests calls `Environment.Exit(0)`, with `xunit.runner.json` pinning `parallelizeTestCollections: false` | 1 |
| `dotnet-console-detailed.txt` | `dotnet test --logger "console;verbosity=detailed"` over the mixed suite | 1 |
| `dotnet-terminallogger-pass.txt` | `script -q … sh -c 'stty rows 40 cols 160; dotnet test'` on the green project — ANSI, real SGR and cursor-motion codes | 0 |
| `dotnet-terminallogger-mixed.txt` | same, on the mixed project | 1 |
| `dotnet-terminallogger-zero-tests.txt` | same, on the no-tests project | 0 |
| `dotnet-multiproject-mixed.txt` | `dotnet test` on a 2-project solution — `Alpha.Tests` 1 passed/2 failed, `Beta.Tests` 2 passed | 1 |
| `dotnet-multiproject-last-line-green.txt` | the identical command, re-run — the interleaving landed the other way up | 1 |
| `dotnet-nunit-mixed.txt` | `dotnet test` — NUnit 4.2.2, 7 tests: 4 passed, 2 failed, 1 `[Ignore("…")]` | 1 |
| `dotnet-mstest-mixed.txt` | `dotnet test` — MSTest 3.6.4, 7 tests: 4 passed, 2 failed, 1 `[Ignore("…")]` | 1 |
| `dotnet-no-build-missing-binary.txt` | `dotnet test --no-build` after `rm -rf bin` | 1 |
| `dotnet-mixed.trx` | `dotnet test --logger "trx;LogFileName=mixed.trx" --results-directory ./TestResults` over the mixed suite | 1 |
| `dotnet-pass.trx` | same logger over the green suite | 0 |
| `dotnet-zero-tests.trx` | same logger over the no-tests project | 0 |

### What these captures establish, that a parser written from the docs would miss

**The SDK prints a different summary to a terminal than to a pipe, and the
piped grammar is entirely absent from the terminal one.** This is the
load-bearing fact for C#. Piped, `dotnet test` ends:

    Failed!  - Failed:     2, Passed:     5, Skipped:     1, Total:     8, …

Through a pty, MSBuild's TerminalLogger takes over and that line **never
appears** — measured, `grep -c 'Passed!\|Failed!\|Skipped!'` is **0** in all
three `terminallogger-*` fixtures — and the only summary in the output is:

    Test summary: total: 8, failed: 2, succeeded: 5, skipped: 1, duration: 1.5s
    Build failed with 2 error(s) in 3.0s

Note `succeeded:`, not `passed:`. A parser proven against the piped fixtures
alone returns `unverifiable` for every run captured through a pty, which is how
a shim that allocates one captures — and `unverifiable` for a green run reads
to a user as ThruLine being broken. Both grammars are shipped here because both
are what this runner produces, on the same machine, for the same command.

**`--logger console;verbosity=detailed` is a third grammar, and it replaces the
first rather than adding to it.** `dotnet-console-detailed.txt` also has **0**
occurrences of `Passed!`/`Failed!`/`Skipped!`; its summary is a block:

    Test Run Failed.
    Total tests: 8
         Passed: 5
         Failed: 2
        Skipped: 1

One runner, one command, three mutually exclusive summary grammars selected by
the logger flag and by whether stdout is a tty.

**Four captures exit 0 on a run that proved nothing.** These are the files that
must parse to `Unverifiable`, never to `0 failed`:

- `dotnet-zero-tests.txt` — `No test is available in …Shipping.Tests.dll.` and
  **no summary line whatsoever**, exit 0. Absent output plus exit 0 is the
  strongest "everything is fine" signal a shell can give.
- `dotnet-filter-no-match.txt` — `No test matches the given testcase filter
  \`FullyQualifiedName~ThisNameMatchesNothing\``, no summary, exit 0. One typo in
  a `--filter` turns a suite into a green no-op.
- `dotnet-all-skipped.txt` — `Skipped! - Failed:     0, Passed:     0, Skipped:
  3, Total:     3`, exit 0. Here there *is* a summary and it *is* well-formed;
  `failed == 0` is true and the suite ran nothing.
- `dotnet-terminallogger-zero-tests.txt` — the same emptiness in the third
  grammar: `Test summary: total: 0, failed: 0, succeeded: 0, skipped: 0` under
  `Build succeeded with 1 warning(s)`, exit 0.

`dotnet-zero-tests.trx` is the machine-readable form of the same trap:
`<ResultSummary outcome="Completed">` over `<Counters total="0" … failed="0" …/>`.
`outcome="Completed"` and zero failures is what a well-formed report of nothing
looks like.

**Which per-project summary comes last in a solution run is a coin flip.**
`dotnet test` on a two-project solution runs the projects in parallel and
interleaves their output — the fixtures contain two `VSTest version` banners and
two `Starting test execution` lines, out of order. Running the same failing
solution 8 consecutive times, the last summary line was `Passed!` in **3** runs
and `Failed!` in **5**; all 8 exited 1. Both shapes are shipped. The rule is the
`cargo test` rule in another runner — per-target summaries must be **summed**,
never read from the last one — but the failure mode here is worse than cargo's,
because it is intermittent: a reader that takes the last line is wrong three
times in eight and green in exactly those three.

**TRX has no `skipped` counter, and the one it looks like it has is a lie.**
Measured on `dotnet-mixed.trx`, a report of 8 tests with 1 skipped:

    <Counters total="8" executed="7" passed="5" failed="2" … notExecuted="0" …/>

`notExecuted="0"` while the skipped test is right there as
`<UnitTestResult … outcome="NotExecuted">`. The skipped count is
`total - executed`, and nothing else in the header carries it.

**And `outcome=` in a TRX is not a per-test attribute.** The same 8-test report
carries 12 `outcome="…"` attributes: 8 on `<UnitTestResult>`, 1 on
`<ResultSummary>` (`"Failed"`), and 3 on `<RunInfo>` elements that merely echo
the adapter's console lines (two `"Error"`, one `"Warning"` — the `"Warning"`
being the *skipped* test). Counting `outcome="Failed"` over the document gives 3
for a run with 2 failures. `<UnitTestResult` is the scope; the element name is
part of the query.

**A compile error and a crashed host both produce no counts at all.**
`dotnet-compile-error.txt` is three lines, one of them `error CS1002: ; expected`,
with no `Test run for …` line — the runner never started.
`dotnet-testhost-crash.txt` reaches `Starting test execution` and then ends:

    The active test run was aborted. Reason: Test host process crashed

    Test Run Aborted.

The file contains **zero** `  Passed`/`  Failed`/`  Skipped` lines and no
summary, so nothing here can be mistaken for a tally. Whether any of the five
tests completed before the host died is not recoverable from this output and is
therefore not claimed. Both files exit 1, which is the only reason a naive
reader survives them — and `dotnet-zero-tests.txt` shows that exit code is not a
reliable second opinion.

**NUnit writes `1)` at the start of stack-trace lines.** `dotnet-nunit-mixed.txt`
contains two lines beginning `1)    at Shipping.NUnit…`. If mocha's
`numbered_failures` heuristic (`verify::parse::numbered_failures`) were reused
for C#, it would read 1 distinct numbered failure against a summary counting 2,
and veto an honest, completely-captured failing run as truncated. The heuristic
is mocha's, and it does not travel.

**The per-test lines are not one grammar across frameworks, though the summary
is.** All three frameworks produce a byte-identical *summary* shape from the
VSTest console logger, but name their tests differently in the detail lines:

    xunit    Failed Shipping.Tests.ShippingRateTests.LargeParcelUsesPerKilo [2 ms]
    NUnit    Failed LargeParcelUsesPerKilo [17 ms]
    MSTest   Failed OversizeRejectsHeavyParcel [10 ms]
    MSTest   Skipped InternationalSurcharge          <- no [N ms] at all

So `test_ids` harvested from the console logger are fully qualified for xunit
and bare method names for NUnit and MSTest, and a duration bracket is not
guaranteed. The TRX `testName` attribute is fully qualified for all three, and is
the only place a stable identifier lives.

**The summary marker is padded to eight columns.** `Passed!  - `, `Failed!  - `,
`Skipped! - ` — two spaces after the six-letter words, one after `Skipped!`.
Matching the literal `"Passed!  - "` works and matching `"Skipped!  - "` does
not; split on whitespace.

**`--filter` is echoed only when it matches nothing.** `dotnet-narrowed.txt` and
`dotnet-narrowed-green.txt` contain the word "filter" **zero** times; the filter
expression appears in the output of `dotnet-filter-no-match.txt` and nowhere
else. `dotnet-narrowed-green.txt` is `Passed!  - Failed:     0, Passed:     1,
Skipped:     0, Total:     1`, exit 0 — a run of one hand-picked test out of a
suite with two failures, structurally identical to an honest green run of a
one-test project. **Narrowing detection for `dotnet test` is argv-only**, the
same conclusion the Ruby section reaches for RSpec's file-path form, and for the
same reason: the output does not carry the fact.

### States not captured, and why

- **No Microsoft.Testing.Platform (MTP) capture.** The .NET 9 templates on this
  machine pin `Microsoft.NET.Test.Sdk` 17.12.0, so every fixture here goes
  through **VSTest**. xunit v3, MSTest with `<EnableMSTestRunner>true</…>`, and
  `dotnet test --` under MTP produce a *fourth* summary grammar that is not
  represented in this directory at all. Any C# parser must be assumed unproven
  against MTP output.
- **No `dotnet vstest` capture.** Only `dotnet test` was run. `dotnet vstest`
  takes assemblies rather than projects and skips the build, so its output lacks
  the `Determining projects to restore` / `Shipping.Tests -> …dll` preamble every
  fixture here begins with.
- **No JUnit-XML capture from .NET.** VSTest emits JUnit XML only through the
  third-party `JunitXml.TestLogger` package, which would have needed a network
  fetch this stage did not make. The `.trx` files are C#'s machine-readable
  coverage here; the JUnit-XML rows in this directory come from the Java stage.
- **No `--blame` / `--blame-crash` capture.** `dotnet-testhost-crash.txt` is the
  plain shape; the `--blame` variants write an additional `Sequence.xml`
  naming the test that was in flight when the host died, and that file is not
  here.
- **No genuinely flaky capture, and no retry.** Every fixture is deterministic
  by construction. The one nondeterminism measured — solution-run summary
  ordering — is documented above and both outcomes are shipped, but a test that
  fails then passes on re-run is not represented.
- **No `dotnet test` under a real CI runner.** The `terminallogger-*` fixtures
  are a pty made with `script(1)`; GitHub Actions sets `TERM=dumb` and pipes,
  which selects the piped grammar, but that has not been measured here — it is
  inference, and this directory's whole point is that inference about runner
  output is worth nothing.
- **Truncation: none.** The largest C# fixture is `dotnet-mixed.trx` at 16 KB,
  well under the 60 KB ceiling. Nothing in this section was truncated, edited or
  path-rewritten after capture; every byte is what the process wrote, absolute
  scratch paths, hostname and CRLF line endings in the pty captures included.


### Second pass: the platform the first pass could not reach

The list above ends with *"No Microsoft.Testing.Platform (MTP) capture … Any C#
parser must be assumed unproven against MTP output."* That gap is now closed,
and closing it cost more than adding a row: **MTP is not a variation on the
VSTest grammar, it is two more grammars**, and one of them appears under the
same `dotnet test` command that produces the VSTest one. Which grammar a C#
parser meets is decided by a property in the `.csproj` of the project under
test, which the parser cannot see.

Toolchain added for this pass, on the same machine and the same SDK:

    MSTest runner (MTP)   MSTest 3.6.4 + Microsoft.Testing.Platform 1.4.3,
                          selected by <EnableMSTestRunner>true</EnableMSTestRunner>
                          + <OutputType>Exe</OutputType>
    Microsoft.Testing.Extensions.TrxReport  1.4.3   (for `mtp-mixed.trx` only)
    xunit.v3              4.0.0  — "xUnit.net v3 In-Process Runner v4.0.0+8bf043c053"
    JunitXml.TestLogger   4.1.0  (VSTest logger, for `dotnet-junit-logger-mixed.xml`)

The `Shipping` class and its `Rate`/`IsOversize` methods are the same ones the
first pass used, restated once more per framework, so a difference between an
`mtp-`, `xunit3-` and `dotnet-` row is a runner difference and not a fixture
difference. Exit codes were measured the same way: all 30 commands re-ran in a
single scripted pass that printed the code each returned, and the two pty
captures print theirs from the pty harness. That script and the pty harness
lived in the throwaway capture tree and are **not** preserved here, so
reproducing these bytes means rebuilding the six new projects (MSTest-under-MTP
mixed / green / no-tests, the same with the TRX extension, xunit v3, and the
JUnit-logger project) from the descriptions in the table. The `dotnet-vstest-`,
`dotnet-blame-crash` and `dotnet-terminallogger-piped-` rows re-used the first
pass's `mixed` and `crash` projects unchanged. Nothing below was truncated or
edited — the largest new file is `mtp-mixed.trx` at 11 KB.

| file | produced by | exit |
|---|---|---|
| `mtp-direct-mixed.txt` | `dotnet Shipping.Mtp.dll` — MSTest 3.6.4 under MTP 1.4.3, the same 7-test mixed suite as `dotnet-mstest-mixed.txt` | 2 |
| `mtp-direct-pass.txt` | the same, on the green MTP project | 0 |
| `mtp-direct-zero-tests.txt` | the same, on a project that compiles and declares no `[TestMethod]` | 8 |
| `mtp-direct-detailed.txt` | `dotnet Shipping.Mtp.dll --output Detailed` | 2 |
| `mtp-direct-narrowed.txt` | `… --filter "FullyQualifiedName~Oversize"` — 3 of 7 | 2 |
| `mtp-direct-narrowed-green.txt` | `… --filter "FullyQualifiedName~SmallParcel"` — 1 of 7, green, over a 2-failure suite | 0 |
| `mtp-direct-filter-no-match.txt` | `… --filter "FullyQualifiedName~ThisNameMatchesNothing"` | 8 |
| `mtp-direct-ignore-exit-code.txt` | `… --ignore-exit-code 2` on the mixed suite — 2 failures, exit **0** | 0 |
| `mtp-direct-min-expected.txt` | `… --filter "…SmallParcel" --minimum-expected-tests 8` | 9 |
| `mtp-direct-list-tests.txt` | `… --list-tests` — discovers 7, runs none | 0 |
| `mtp-ansi-color.txt` | the same binary through a 40×160 pty (`pty.fork()`, `TERM=xterm-256color`) — real SGR **and an OSC 8 hyperlink** | 2 |
| `mtp-dotnettest-mixed.txt` | `dotnet test` on the same MTP project | 1 |
| `mtp-dotnettest-pass.txt` | `dotnet test` on the green MTP project | 0 |
| `mtp-dotnettest-zero-tests.txt` | `dotnet test` on the no-tests MTP project | 1 |
| `mtp-dotnettest-narrowed.txt` | `dotnet test -- --filter "FullyQualifiedName~Oversize"` | 1 |
| `mtp-dotnettest-bad-option.txt` | `dotnet test -- --report-trx …` against a project that does **not** reference the TRX extension — the option is rejected inside the test host | 1 |
| `mtp-mixed.trx` | `dotnet Shipping.MtpTrx.dll --report-trx --report-trx-filename mtp-mixed.trx --results-directory ./TestResults`, with `Microsoft.Testing.Extensions.TrxReport` 1.4.3 referenced | 2 |
| `xunit3-mixed.txt` | `dotnet Shipping.V3.dll` — xunit.v3 4.0.0, 8 tests, **ANSI into a pipe** | 1 |
| `xunit3-nocolor-mixed.txt` | `dotnet Shipping.V3.dll -noColor`, same suite | 1 |
| `xunit3-narrowed.txt` | `… -noColor -method "…OversizeRejectsHeavyParcel" -method "…OversizeThreshold"` | 1 |
| `xunit3-narrowed-green.txt` | `… -noColor -method "…SmallParcelUsesFlatBase"` — 1 test, green, over a 2-failure suite | 0 |
| `xunit3-filter-no-match.txt` | `… -noColor -method "…ThisNameMatchesNothing"` | 0 |
| `xunit3-ignore-failures.txt` | `… -noColor -ignoreFailures` — 2 failures, exit **0** | 0 |
| `xunit3-stop-on-fail.txt` | `… -noColor -stopOnFail` — reports `Total: 1` for an 8-test suite | 58 |
| `xunit3-fail-skips.txt` | `… -noColor -failSkips` — the skipped test is recounted as a failure | 1 |
| `xunit3-dotnettest-mixed.txt` | `dotnet test` on the xunit v3 project (MTP path, `TestingPlatformDotnetTestSupport`) | 1 |
| `xunit3-ansi-color.txt` | `dotnet Shipping.V3.dll` through the same 40×160 pty | 1 |
| `xunit3-junit.xml` | `… -noColor -result-junit ./reports/junit.xml` — JUnit XML written by xunit v3 itself | 1 |
| `xunit3-ctrf.json` | `… -noColor -result-ctrf ./reports/ctrf.json` — CTRF, the first machine-readable **JSON** report in this section | 1 |
| `dotnet-vstest-mixed.txt` | `dotnet vstest ./bin/Debug/net9.0/Shipping.Tests.dll` on the first pass's mixed project | 1 |
| `dotnet-blame-crash.txt` | `dotnet test --blame-crash` on the `Environment.Exit(0)` project | 1 |
| `dotnet-terminallogger-piped-mixed.txt` | `MSBUILDTERMINALLOGGER=on dotnet test` on the mixed project, stdout **redirected to a file** — no pty anywhere | 1 |
| `dotnet-junit-logger-mixed.xml` | `dotnet test --logger "junit;LogFilePath=./TestResults/junit-mixed.xml"` with `JunitXml.TestLogger` 4.1.0 | 1 |

### What the second pass establishes

**There are six summary grammars for C#, not three.** The first pass found
three and said so. Adding MTP and xunit v3 adds three more, and the count is a
measurement over the shipped bytes rather than a reading of the docs — grepping
each mixed-run fixture for the three known summary markers:

| fixture | `Passed!`/`Failed!` | `TEST EXECUTION SUMMARY` | `Test (run) summary` |
|---|---|---|---|
| `dotnet-mixed.txt` (VSTest, piped) | 1 | 0 | 0 |
| `dotnet-console-detailed.txt` | 0 | 0 | 0 |
| `dotnet-terminallogger-mixed.txt` (pty) | 0 | 0 | 1 |
| `mtp-direct-mixed.txt` | 1 | 0 | 1 |
| `mtp-dotnettest-mixed.txt` | 1 | 0 | 0 |
| `xunit3-mixed.txt` | 0 | 1 | 0 |

Written out, the six:

    VSTest piped     Failed!  - Failed:     2, Passed:     5, Skipped:     1, Total:     8, …
    VSTest detailed  Test Run Failed. / Total tests: 8 / Passed: 5 / Failed: 2 / Skipped: 1
    TerminalLogger   Test summary: total: 8, failed: 2, succeeded: 5, skipped: 1, duration: 0.4s
    MTP direct       Test run summary: Failed! - …dll (net9.0|arm64)
                       total: 7 / failed: 2 / succeeded: 4 / skipped: 1 / duration: 110ms
    MTP dotnet test  Failed! - Failed: 2, Passed: 4, Skipped: 1, Total: 7, Duration: 77ms
    xunit v3         === TEST EXECUTION SUMMARY ===
                        Shipping.V3  Total: 8, Errors: 0, Failed: 2, Skipped: 1, Not Run: 0, Time: 0.072s

Two of these are new *shapes*, not new spellings. The MTP direct summary puts
its counts on **five following lines**, so a reader that parses one line finds a
verdict word and no numbers. xunit v3's line carries two counters nothing else
has — `Errors:` and `Not Run:` — and names its assembly in the same line, which
means a multi-assembly run emits one such line per assembly and they must be
**summed**. That is `parse_cargo_test`'s rule (*"a workspace run emits one per
test target, and they must be SUMMED"*) arriving in C#.

**The first pass's advice to split on whitespace is what saves the MTP row.**
VSTest pads to eight columns (`Failed!  - Failed:     2,`); MTP does not
(`Failed! - Failed: 2,`). A parser anchored on the literal `"Failed!  - "` — two
spaces — reads every MTP `dotnet test` run as having no summary. The first pass
reached the right rule from the `Skipped!` case; MTP is the second, independent
reason for it.

**`Zero tests ran` is a phrase MTP has and VSTest does not.** `mtp-direct-zero-tests.txt`
and `mtp-direct-filter-no-match.txt` both read `Test run summary: Zero tests ran`
over `total: 0`, and both exit **8**. That is strictly better than the VSTest
behaviour the first pass documented (silence and exit 0), and it means the safe
reading of a C# zero-test run has to cover both a named phrase and an unnamed
absence.

**Under MTP, `dotnet test` on a suite with no tests prints `Failed!` with
`Failed: 0`.** Measured, `mtp-dotnettest-zero-tests.txt`:

    Failed! - Failed: 0, Passed: 0, Skipped: 0, Total: 0, Duration: 36ms

The verdict word and the counts disagree inside one line, in a runner where the
counts are the only thing a parser normally reads. This is the same defect
`parse_cargo_test` already refuses on (*"`FAILED` with nothing counted as failed
is a line disagreeing with itself"*), and it argues that the rule is not
cargo-specific: **whatever the runner, a verdict word that contradicts its own
tally means `unverifiable`.** Reading the tally alone gives `failed=0` for a
suite that failed to be a suite.

**Two runners ship a documented flag that zeroes the exit code of a failing
run.** `--ignore-exit-code 2` (MTP) and `-ignoreFailures` (xunit v3) both leave
the summary intact — `Failed: 2` is right there — and return **0**.
`mtp-direct-ignore-exit-code.txt` and `xunit3-ignore-failures.txt` are those
runs. A verifier that reads exit codes is defeated by a documented flag; a
verifier that reads the summary is not. This is not a hypothetical cheat
invented for a detector, it is in `--help`.

**xunit v3 writes ANSI into a pipe, unasked.** `xunit3-mixed.txt` was captured
with stdout redirected to a file and contains **25** escape sequences; its
`-noColor` twin contains 0. Every other piped capture in this section
(`dotnet-mixed.txt`, `mtp-direct-mixed.txt`, `dotnet-vstest-mixed.txt`) contains
0. `-useAnsiColor`'s help text says why: *"force using ANSI color output on
Windows (non-Windows always uses ANSI colors)"*. So `strip_ansi` is not a
CI-only concern for C# — on macOS and Linux it is the default path for the
newest xunit, and a parser that skips it fails on an ordinary local run.

**And `MSBUILDTERMINALLOGGER=on` selects the TerminalLogger grammar with no pty
at all.** The first pass reached the TerminalLogger grammar only through
`script(1)` and wrote that a CI that pipes *"selects the piped grammar, but that
has not been measured here — it is inference"*. It is now measured on the other
side: `dotnet-terminallogger-piped-mixed.txt` is stdout redirected to a file,
contains **64** escape sequences and **zero** `Passed!`/`Failed!` markers, and
ends `Test summary: total: 8, failed: 2, succeeded: 5, skipped: 1`. `dotnet test
-tl:on` produces the identical grammar and is not shipped separately. The
inference that survives is narrower than the first pass's: a pipe does not imply
the piped grammar — **an environment variable outranks the tty check**, and the
variable is one any CI configuration may set.

**`notExecuted` in a TRX is populated by MTP and not by VSTest.** The first pass
wrote *"TRX has no `skipped` counter, and the one it looks like it has is a
lie"*, on `dotnet-mixed.trx`:

    VSTest  <Counters total="8" executed="7" … notExecuted="0" …/>   1 outcome="NotExecuted" result
    MTP     <Counters total="7" executed="6" … notExecuted="1" …/>   1 outcome="NotExecuted" result

Same extension, same element, same attribute, two writers, two meanings. The
first pass's *derived* rule — skipped is `total - executed` — is the one that
survives both, and this is the evidence for preferring it: it is right for
`notExecuted="0"` and right for `notExecuted="1"`, where trusting the attribute
is right only half the time. The two TRX files also differ in noise:
`dotnet-mixed.trx` carries 12 `outcome="…"` attributes for 8 tests (3 of them on
`<RunInfo>` elements), `mtp-mixed.trx` carries 8 for 7 tests and no `<RunInfo>`
at all — so a document-wide `outcome="Failed"` count is wrong by 1 on one file
and right on the other, which is the worst way for a bug to behave.

**`<testsuites>` and `<testsuite>` disagree about `skipped` in xunit v3's JUnit
XML.** `xunit3-junit.xml` opens:

    <testsuites name="Test results" tests="8" failures="2" errors="0" disabled="0" …>
      <testsuite name="…/Shipping.V3.dll" tests="8" failures="2" errors="0" disabled="0" skipped="1" …>

The wrapper has every counter except `skipped`; only the inner element has it.
A reader that takes the first element whose name starts with `<testsuite` gets
the wrapper and reports `skipped=0, passed=6` for a run with 5 passes and a
skip. `verify::parse_junit_xml` already looks for the inner element for exactly
this class of reason, found in pytest's report; this is a second, independent
document that punishes the same mistake. `dotnet-junit-logger-mixed.xml` (the
third-party VSTest logger) has a bare `<testsuites>` with no attributes at all
and every count on the inner element, which is the same lesson stated more
bluntly.

**`--output Detailed` is a seventh grammar for the per-test lines, and the
lower-case verbs are MTP's, not VSTest's.** `mtp-direct-detailed.txt` names
every test:

    passed SmallParcelUsesFlatBase (1ms)
    failed LargeParcelUsesPerKilo (8ms)
    skipped InternationalSurcharge (0ms)

against VSTest's `  Failed Shipping.Tests.…LargeParcelUsesPerKilo [2 ms]`. Verb
case, name qualification and duration bracket all differ, and MTP's default
(non-detailed) output prints these lines *only for non-passing tests* — the four
passes appear nowhere in `mtp-direct-mixed.txt`. Harvesting `test_ids` from MTP
console output therefore yields the failures and skips and silently omits the
passes, unless `--output Detailed` was passed. The first pass's finding that
`test_ids` are unreliable outside a TRX now has a second runner behind it.

**A rejected command-line option under `dotnet test` produces a failing build
and no counts.** `mtp-dotnettest-bad-option.txt` is `dotnet test -- --report-trx`
against a project without the TRX extension. The test host prints `Unknown
option '--report-trx'` **into its own log file**, and stdout gets only

    …/Shipping.Mtp.dll : error run failed: Tests failed: '…/Shipping.Mtp_net9.0_arm64.log' …

with no summary line anywhere and exit 1 — the identical shape a genuinely
failing run produces. Stable across three consecutive runs. Nothing in the
output distinguishes *"your tests failed"* from *"your arguments were rejected
and no test ran"*, so the honest reading of an MTP `dotnet test` output with no
summary line is `unverifiable`, and the `error run failed` line must not be read
as evidence that tests ran.

**Two more narrowing shapes that the output does not carry.**
`xunit3-narrowed-green.txt` is `Total: 1, Errors: 0, Failed: 0, Skipped: 0, Not
Run: 0`, exit 0, over a suite with two failures — no filter expression appears
anywhere in the file. `mtp-direct-narrowed-green.txt` is the same story in the
MTP grammar. This reproduces the first pass's conclusion for `dotnet test
--filter` in two further runners: **narrowing detection for C# is argv-only.**
`xunit3-stop-on-fail.txt` adds a third form the argv must be read for — an
8-test suite reporting `Total: 1` because `-stopOnFail` ended it early, which is
RSpec's `--fail-fast` under another name. `mtp-direct-list-tests.txt` adds a
fourth: exit 0, seven test names printed, nothing executed.

**`-failSkips` changes the counts without changing the code.**
`xunit3-fail-skips.txt` reports `Failed: 3, Skipped: 0` where
`xunit3-nocolor-mixed.txt` reports `Failed: 2, Skipped: 1`, same binary, same
tests. The skipped test appears as `[FAIL] … FAIL_SKIP : pending the
international rate table`. Any rule that compares a skip count across two runs
has to know whether this flag was in the argv.

**`xunit3-filter-no-match.txt` truncates its own summary line.** A filter that
matches nothing produces:

       Shipping.V3  Total: 0

with no `Errors:`, `Failed:`, `Skipped:` or `Not Run:` clause at all, and exit
**0**. A parser that requires `Failed:` on the summary line finds no summary and
must say so; a parser that defaults the missing clause to zero reports a clean
run of zero tests for a typo'd filter. This is the shape `parse_junit_xml`'s
defaulted-attribute bug had, in a text grammar.

**The pty captures carry an OSC 8 hyperlink, not only SGR.** `mtp-ansi-color.txt`
wraps the assembly path in `ESC ] 8 ; ; file:///… ESC \ … ESC ] 8 ; ; ESC \`
inside the summary line itself, and draws its progress as
`[✓2/x1/↓1] Shipping.Mtp.dll`. A CSI-only stripper leaves `]8;;file://…` sitting
in the middle of the line it is trying to parse. `verify::parse::strip_ansi`
handles OSC already; this is the first captured C# document that requires it.
Both pty captures were taken with a Python `pty.fork()` harness rather than
`script(1)`, because `script(1)` allocated a pty for the first two invocations
of this session and then failed with `tcgetattr/ioctl: Operation not supported
on socket` for every later one — the harness is deterministic where `script` was
not. Their bytes are otherwise the same kind of thing as the first pass's
`terminallogger-*` files, minus `script`'s own `\x04\b\b` preamble.

### States still not captured, and why

The first pass named six gaps. Four are closed above — MTP, `dotnet vstest`,
JUnit XML from .NET, and `--blame-crash`. What remains, plus what this pass
newly could not reach:

- **No `--blame-hang` or `Sequence.xml` capture.** `dotnet-blame-crash.txt`
  differs from `dotnet-testhost-crash.txt` by exactly one line (`Blame:
  Attaching crash dump utility to process dotnet (74142).`) and is otherwise the
  same aborted-run shape. The `Sequence.xml` that `--blame` writes beside it,
  naming the test in flight when the host died, is still not here, and neither
  is the hang-detection variant.
- **No capture under a real CI runner.** Still true, and the first pass's
  inference is now known to be *narrower* than it looked — see the
  `MSBUILDTERMINALLOGGER` finding — but GitHub Actions itself has not been run.
- **No genuinely flaky capture, and no retry.** Unchanged. Every fixture here is
  deterministic by construction except the solution-run ordering the first pass
  documented.
- **xunit v3 writes four more report formats that are not captured.**
  `-result-nunit`, `-result-html`, `-result-trx` and `-result-xml` (xunit's own
  v2+ schema) all work on this machine and were deliberately left out to keep
  this section to formats a verifier would plausibly parse. That is a choice,
  not an obstacle: they can be captured in minutes from the same project.
- **No MTP zero-test capture under `dotnet test` for xunit v3.** The
  `Failed!  - … Total: 0` shape is captured for the MSTest runner only; whether
  xunit v3's MTP host words an empty run the same way under `dotnet test` is
  **not measured**, and must not be assumed from the MSTest row.
- **No compile-error capture under MTP.** `dotnet-compile-error.txt` is the
  VSTest path. An MTP project that fails to compile almost certainly produces
  the same MSBuild `error CS…` lines, since the build precedes the platform
  choice — but "almost certainly" is what this directory exists to refuse, and
  it is not captured.
- **Truncation: none, again.** The largest file added in this pass is
  `mtp-mixed.trx` at 11 KB. Nothing was truncated, edited or path-rewritten;
  absolute scratch paths, the machine's hostname in
  `dotnet-junit-logger-mixed.xml`, and the pty captures' CRLF line endings are
  all as the processes wrote them.


## Java

Toolchain, as reported by the binaries that produced these bytes:

    mvn       Apache Maven 3.9.16 (2bdd9fddda4b155ebf8000e807eb73fd829a51d5)
    gradle    Gradle 9.7.1 (build 92f0512e7f06d84621afba191f75e265363890cf)
    java      openjdk 17.0.19 2026-04-21 (Homebrew, build 17.0.19+0)
    surefire  maven-surefire-plugin 3.5.2   (failsafe 3.5.2, compiler 3.15.0)
    junit     JUnit Jupiter 5.11.4 / JUnit Platform 1.11.4
              junit-platform-console-standalone 1.11.4

**`JAVA_HOME=/opt/homebrew/opt/openjdk@17` was exported for every run.** This is
provenance, not trivia: `java` on PATH is 17, but Maven and Gradle both default
to the Homebrew `openjdk` 26 on this machine, and the two JDKs put different line
numbers in the `java.base/...` stack frames. Every frame in these fixtures is a
17 frame. A parser is not sensitive to that; a reader trying to reproduce a
byte-for-byte capture is.

Every command below ran in a throwaway multi-project tree built for the capture
and kept out of the repo (`mvn-pass`, `mvn-mixed`, `mvn-skipped`,
`mvn-zero-tests`, `mvn-compile-error`, `mvn-failsafe`, `mvn-vm-crash`,
`mvn-flaky`, `mvn-reactor`, and the `gradle-*` equivalents). Each project is one
or two JUnit 5 test classes over trivial arithmetic, so the same three test
methods appear under Maven, under Gradle and under the ConsoleLauncher, and a
difference between the three tables is a runner difference and not a fixture
difference.

The `exit` column is not a memory. Every row except the four named at the end of
this section was re-run in a single scripted pass over **fresh copies of each
project with `target/` and `build/` deleted**, and that pass printed the code
each command returned; all 25 codes matched the capture run. The script lived in
the throwaway tree and is not preserved, so reproducing these bytes means
rebuilding the projects from the descriptions below.

### Maven (Surefire / Failsafe)

| file | produced by | exit |
|---|---|---|
| `maven-all-pass.txt` | `mvn -B test` — 3 green tests in one class, surefire 3.5.2 | 0 |
| `maven-mixed.txt` | `mvn -B test` — 6 tests over 2 classes: 4 pass, 1 assertion failure, 1 escaped `NullPointerException` | 1 |
| `maven-skipped.txt` | `mvn -B test` — 1 pass, 1 `@Disabled("...")`, 1 failed `assumeTrue`; Surefire's own word for both is `Skipped` | 0 |
| `maven-zero-tests.txt` | `mvn -B test` on a project with `src/main` and no `src/test` | 0 |
| `maven-compile-error.txt` | `mvn -B test` where the test source is missing a semicolon and names a class that does not exist | 1 |
| `maven-narrowed.txt` | `mvn -B test -Dtest=CalculatorTest` over the **mixed** project | 0 |
| `maven-ansi-color.txt` | `mvn test -Dstyle.color=always` over the mixed project (no `-B`: batch mode also disables colour) | 1 |
| `maven-verify-it-failure.txt` | `mvn -B verify` — surefire's unit tests all green, failsafe's `CheckoutIT` fails | 1 |
| `maven-vm-crash.txt` | `mvn -B test` where one test calls `System.exit(1)`, killing the forked JVM | 1 |
| `maven-flaky.txt` | `mvn -B test` with `<rerunFailingTestsCount>2</rerunFailingTestsCount>` and a test that fails its first invocation and passes the rerun | 0 |
| `maven-reactor-mixed.txt` | `mvn -B test` on a 2-module reactor: `core` green, `web` failing | 1 |
| `maven-quiet-all-pass.txt` | `mvn -q test` on the all-pass project. **This file is 0 bytes.** That is the capture, not a mistake | 0 |
| `maven-quiet-mixed.txt` | `mvn -q test` on the mixed project | 1 |
| `maven-cold-cache.txt` | `mvn -B test` on the all-pass project against an **empty `~/.m2`** — the same green run as `maven-all-pass.txt` with 300+ lines of dependency resolution in front of it | 0 |
| `maven-surefire-all-pass.xml` | `target/surefire-reports/TEST-demo.CalculatorTest.xml` from the `maven-all-pass.txt` run | — |
| `maven-surefire-mixed.xml` | `target/surefire-reports/TEST-demo.OrderServiceTest.xml` from the `maven-mixed.txt` run | — |
| `maven-surefire-skipped.xml` | `target/surefire-reports/TEST-demo.ShippingTest.xml` from the `maven-skipped.txt` run | — |
| `maven-failsafe-it-failure.xml` | `target/failsafe-reports/TEST-demo.CheckoutIT.xml` from the `maven-verify-it-failure.txt` run | — |

### Gradle

| file | produced by | exit |
|---|---|---|
| `gradle-all-pass.txt` | `gradle --console=plain --no-daemon test` — 3 green tests, `testLogging { events "passed","failed","skipped" }` | 0 |
| `gradle-mixed.txt` | `gradle --console=plain test` — 9 tests over 3 classes: 5 pass, 2 fail, 2 skip | 1 |
| `gradle-skipped.txt` | `gradle --console=plain test` — 1 pass, 1 `@Disabled`, 1 failed `assumeTrue` | 0 |
| `gradle-zero-tests.txt` | `gradle --console=plain test` on a project with `src/main` and no `src/test` | 0 |
| `gradle-compile-error.txt` | `gradle --console=plain test` on the same broken source Maven rejected | 1 |
| `gradle-narrowed.txt` | `gradle --console=plain test --tests 'demo.CalculatorTest'` over the **mixed** project | 0 |
| `gradle-filter-no-match.txt` | `gradle --console=plain test --tests 'demo.CalculatorTest.doesNotExist'` over the mixed project | 1 |
| `gradle-up-to-date.txt` | `gradle --console=plain test` run a **second** time on the unchanged all-pass project | 0 |
| `gradle-ignore-failures.txt` | `gradle --console=plain test` with `ignoreFailures = true` on a suite where 2 of 3 tests fail | 0 |
| `gradle-default-logging.txt` | `gradle --console=plain test` on a green project whose `build.gradle` has **no `testLogging` block** — the shape most real Gradle builds have | 0 |
| `gradle-ansi-color.txt` | `gradle --console=rich test` over the mixed project | 1 |
| `gradlew-bootstrap-failure.txt` | `./gradlew --console=plain test` after `gradle wrapper`, when the distribution download filled the disk | 1 |
| `gradle-junit-all-pass.xml` | `build/test-results/test/TEST-demo.CalculatorTest.xml` from the `gradle-all-pass.txt` run | — |
| `gradle-junit-mixed.xml` | `build/test-results/test/TEST-demo.OrderServiceTest.xml` from the `gradle-mixed.txt` run | — |
| `gradle-junit-skipped.xml` | `build/test-results/test/TEST-demo.ShippingTest.xml` from the `gradle-skipped.txt` run | — |

### JUnit Platform ConsoleLauncher

The third real producer of JUnit XML on this machine, and the one an agent
reaches for when there is no build tool. Run against the `gradle-mixed` project's
compiled test classes, so it executes byte-for-byte the same 9 tests Gradle ran.

| file | produced by | exit |
|---|---|---|
| `junit-console-mixed.txt` | `java -jar junit-platform-console-standalone-1.11.4.jar execute --class-path <gradle-mixed test classes> --scan-class-path --details=tree --disable-ansi-colors --reports-dir=reports` | 1 |
| `junit-console-ansi.txt` | the same command with `--color-palette=/dev/null` in place of `--disable-ansi-colors` | 1 |
| `junit-console-zero-tests.txt` | the same command with `--class-path` pointing at an **empty** directory | 0 |
| `junit-console-discovery-error.txt` | the same jar with `--select-class=demo.NoSuchTest` | 255 |
| `junit-console-report.xml` | `reports/TEST-junit-jupiter.xml` from the `junit-console-mixed.txt` run | — |

### What these captures establish, that a parser written from the docs would miss

**Gradle's stdout carries no test counts on a successful run.** This is the
load-bearing fact for Java. `gradle-default-logging.txt` is a real, green,
3-test run whose *entire* evidence is:

    > Task :test

    BUILD SUCCESSFUL in 48s
    2 actionable tasks: 2 executed

There is no number anywhere in the file. Gradle prints a count line —
`9 tests completed, 2 failed, 2 skipped` — **only when something failed**, and it
prints per-test `PASSED`/`FAILED`/`SKIPPED` lines only when the build script opts
into `testLogging`. A Gradle stdout parser therefore has to return
`unverifiable` for the most common green shape there is; the counts exist only in
`build/test-results/test/TEST-*.xml`. `gradle-all-pass.txt` and
`gradle-default-logging.txt` are the same three passing tests with and without
that opt-in, and they are what a parser must be able to tell apart.

**Five separate captures print a benign result for a run that proved nothing:**

- `maven-quiet-all-pass.txt` is **0 bytes with exit 0**. `mvn -q test` on a green
  build writes nothing at all. There is no output to parse, and "no output plus
  exit 0" is indistinguishable from a command that never ran.
- `gradle-up-to-date.txt` is `> Task :test UP-TO-DATE` and `BUILD SUCCESSFUL`,
  exit 0, with **zero tests executed** — Gradle's incremental check decided the
  inputs had not changed. The only marker anywhere in the bytes is the
  `UP-TO-DATE` suffix on the `:test` task line; every other line is identical in
  shape to a run that did execute.
- `gradle-ignore-failures.txt` is `BUILD SUCCESSFUL` and **exit 0** over
  `3 tests completed, 2 failed`. `ignoreFailures = true` lives in `build.gradle`,
  not in argv, so nothing an agent typed reveals it.
- `gradle-zero-tests.txt` is `> Task :test NO-SOURCE` and `BUILD SUCCESSFUL`,
  exit 0, over a project with no tests at all.
- `maven-zero-tests.txt` is `[INFO] No tests to run.` and `BUILD SUCCESS`,
  exit 0. **There is no `Tests run:` line anywhere in the file.** A parser that
  falls back to a zeroed `Parsed` here reports a clean run of nothing, which is
  exactly the collapse of `unverifiable` into `supported` that §6.2 forbids.

**First-match and last-match parsing of `Tests run:` are both wrong, and they
are wrong in opposite directions in two different fixtures.**

- `maven-verify-it-failure.txt` contains **two `Results:` blocks**. The first is
  Surefire's, and it says `Tests run: 2, Failures: 0, Errors: 0, Skipped: 0`.
  The second is Failsafe's, and it says `Tests run: 1, Failures: 1`. The build
  is `BUILD FAILURE`. Taking the first summary reads a clean run out of a failed
  build.
- `maven-flaky.txt` inverts it. The per-class line says
  `Tests run: 2, Failures: 1, ... <<< FAILURE!` and the final Results block says
  `Tests run: 2, Failures: 0, Errors: 0, Skipped: 0, Flakes: 1` with
  `BUILD SUCCESS` and exit 0. Taking the first summary reads a failure out of a
  green build. `Flakes:` is a sixth field that only appears when
  `rerunFailingTestsCount` is set, and a `Failures: 0, Flakes: 1` run is a run
  in which a test did fail.
- `maven-reactor-mixed.txt` has two `Results:` blocks for the same reason a
  multi-module repo does, plus a `Reactor Summary` naming `core ... SUCCESS` and
  `web ... FAILURE` per module. Nothing aggregates the two test tallies; the
  only aggregate in the file is the per-module SUCCESS/FAILURE list.

**`maven-vm-crash.txt` is the most important file in this section, and it is the
one I described wrongly first.** A test called `System.exit(1)` and took the
forked JVM with it. I expected Surefire to print nothing where the summary goes.
What it actually prints is:

    [INFO] Results:
    [INFO]
    [INFO] Tests run: 0, Failures: 0, Errors: 0, Skipped: 0

**Surefire hands the parser a clean, zeroed, well-formed summary for a run in
which the suite died halfway through.** This is not a parser mistakenly
defaulting `Parsed` to `{total: 0, failed: 0}` — it is the *runner* supplying
exactly those numbers, in exactly the shape a green run supplies them, at
`[INFO]` severity. Any parser that reads the `Results:` block and stops has
already collapsed `unverifiable` into `supported`, and it did so by believing
its input.

The only things in the file that contradict the summary are `BUILD FAILURE`, a
`SurefireBooterForkException: The forked VM terminated without properly saying
goodbye. VM crash or System.exit called?`, a `Process Exit Code: 1`, and a
`Crashed tests:` list naming `demo.SessionTest`. A `Tests run: 0` summary is
therefore never on its own evidence of anything, and the honest reading of
"zero tests, and the build failed" is `unverifiable` — not "0 failed", and not
"no tests exist", which is what `maven-zero-tests.txt` means by the same zero.

The first draft of this paragraph claimed the file had no `Results:` block at
all. It has one. The claim was written from what the failure mode *should* look
like and was caught by grepping the shipped bytes, which is the same mistake, in
the same direction, as the hand-written `pytest -q` fixture that caused this
directory to exist.

`gradle-compile-error.txt`, `maven-compile-error.txt`,
`gradlew-bootstrap-failure.txt` and `junit-console-discovery-error.txt` are four
more shapes of a run that produced no test results — but unlike the VM crash,
none of those four prints a summary line at all, because the process died before
Surefire or the launcher reached one. `junit-console-discovery-error.txt` is
13.5 KB for one missing class because the ConsoleLauncher follows the stack
trace with its **entire `--help` output**, which includes the line
`--fail-if-no-tests  Fail and return exit status code 2 if no tests are found.`
— prose containing the word `tests` that a loose line-matcher can hit. Its exit
code is 255, not 1 and not 2.

**Narrowing is invisible in the output of both build tools.**
`maven-narrowed.txt` (`-Dtest=CalculatorTest`) and `gradle-narrowed.txt`
(`--tests 'demo.CalculatorTest'`) are both fully green — `BUILD SUCCESS` /
`BUILD SUCCESSFUL`, exit 0 — over the *same projects* whose unfiltered runs are
`maven-mixed.txt` and `gradle-mixed.txt`, both failing. Neither output mentions
a filter, a selection, or a count of deselected tests. The narrowing exists
only in argv, which is why §6.5 has to read it from there.
`gradle-filter-no-match.txt` is the one case where the filter surfaces:
`No tests found for given includes: [demo.CalculatorTest.doesNotExist](--tests filter)`,
`BUILD FAILED`, exit 1.

**Maven's colour shreds the per-class lines and leaves the aggregate whole, and
the difference is not uniform.** `maven-ansi-color.txt` contains all three of
these, and they are coloured three different ways:

    ESC[1;31mTests ESC[0;1mrun: ESC[0;1m3ESC[m, ESC[1;31mFailures: ESC[0;1;31m1ESC[m, ESC[1;31mErrors: ESC[0;1;31m1ESC[m, Skipped: 0, ...
    ESC[1;32mTests run: ESC[0;1;32m3ESC[m, Failures: 0, Errors: 0, Skipped: 0, ...
    ESC[1;31mTests run: 6, Failures: 1, Errors: 1, Skipped: 0ESC[m

The failing per-class line splits `Tests` from `run:` and wraps every non-zero
count on its own; the passing per-class line splits only the count; the final
aggregate is wrapped in a single SGR pair and survives intact. So the literal
substring `Tests run: 3, Failures: 1` does **not** occur anywhere in the file,
while `Tests run: 6, Failures: 1, Errors: 1, Skipped: 0` occurs exactly once.
A parser that greps the aggregate without stripping ANSI happens to work here
and would break the moment Surefire colours a digit in it; one that greps the
per-class lines finds nothing. Neither behaviour is a decision anyone made, and
that is the point of keeping the file: stripping ANSI first is the only reading
of these bytes that is not an accident.

`gradle-ansi-color.txt` (`--console=rich`) is a different problem from SGR: it
carries 51 cursor-up, 68 cursor-down, 53 erase-to-end-of-line and 63 cursor-back
escapes (`ESC[1A`, `ESC[2B`, `ESC[0K`, `ESC[37D`) that redraw a progress bar in
place, and the redrawn progress text includes lines like
`> :test > 5 tests completed, 1 failed` which are **not** the summary. Five
distinct such strings appear in the one capture — `0 tests completed`,
`1 test completed`, `4 tests completed`, `5 tests completed, 1 failed`, and
finally `9 tests completed, 2 failed, 2 skipped` — three of which are running
totals from mid-execution, and one of which (`1 test completed`) is singular.
Stripping SGR alone leaves all five in the stream; whatever chooses the summary
line has to pick the last, and has to not be fooled by the singular form.

**The three JUnit XML producers here are three dialects, not one format.**
Over identical test methods:

- Surefire writes `<error>` for the escaped `NullPointerException` and
  `<failure>` for the assertion, and its `<testsuite>` counts them in separate
  `errors=` and `failures=` attributes. `testcase name="looksUpAMissingCustomer"`.
- Gradle writes **both** as `<failure>` and its `<testsuite>` therefore always
  reports `errors="0"`. Counting `failures` alone is exact on Gradle output and
  silently under-counts on Surefire output. `testcase name="looksUpAMissingCustomer()"`
  — Gradle keeps the parentheses, Surefire strips them, so the same test has two
  different names depending on who wrote the report.
- The ConsoleLauncher writes `<error>` like Surefire and keeps the parentheses
  like Gradle.

**Skipped is at least two different states, and only one producer says so.**
A `@Disabled` test and a test whose `assumeTrue` failed are both `Skipped: 2` to
Surefire and both `SKIPPED` to Gradle. In XML they are distinguishable — a
`<skipped message="flaky against the sandbox carrier API"/>` versus a
`<skipped type="org.opentest4j.TestAbortedException">` — but only the
ConsoleLauncher's *text* counts them apart, and its tally does not add up unless
both counters are read:

    [ 9 tests found ]  [ 1 tests skipped ]  [ 8 tests started ]
    [ 1 tests aborted ]  [ 5 tests successful ]  [ 2 tests failed ]

`5 + 2 = 7`, not 9; the missing two are the one skipped and the one aborted. Its
own XML (`junit-console-report.xml`) then flattens both back into `skipped="2"`,
so the report loses a distinction its console output made.

**`[ 0 tests found ]` is exit 0.** `junit-console-zero-tests.txt` is a
successful run of nothing: the tree prints three green engine nodes, every
counter reads `0`, and the process exits 0.

**Cold-cache noise is real and it is enormous.** `maven-cold-cache.txt` is the
same three passing tests as `maven-all-pass.txt` — 1.9 KB becomes 45.7 KB —
because a first run against an empty `~/.m2` prints 300-odd
`Downloading from central: ...` / `Downloaded from central: ... (5.6 kB at 245 kB/s)`
lines, every one of which contains digits, the word `at`, and a unit. The
summary is byte-identical to the warm run; only the volume in front of it
changed. This is what a first run in a fresh container looks like.

### States not captured, and why

- ~~No successful `./gradlew test`.~~ **Closed by the second pass** —
  `gradlew-bootstrap-pass.txt`. The first pass's wrapper run died because
  fetching `gradle-9.7.1-bin.zip` filled the disk (`/` was at 100%, 119 MiB
  free); `gradlew-bootstrap-failure.txt` is that real failure and remains a
  legitimate fixture, a build that never reached a test. The disk had 63 GiB free
  by the second pass and the download succeeded. Note that the first pass's
  *prediction* of the preamble was wrong in one detail, and the second pass says
  where.
- **`maven-cold-cache.txt` was not re-verified either.** `~/.m2` is warm now.
  Its exit code (0) is from the capture run. Reproducing it means deleting
  `~/.m2` first.
- **`maven-quiet-all-pass.txt` and `maven-quiet-mixed.txt`** were captured and
  their exit codes read directly, but they were added after the scripted
  verification pass and so are not among the 25 re-run rows.
- ~~No TestNG capture.~~ **Closed by the second pass**, which also proved the
  sentence that stood here false. Surefire's TestNG provider does **not** print
  `Total tests run: N, Passes: N, …`; it prints Surefire's own grammar, byte-for-byte
  the JUnit provider's. The claim was written from documentation about TestNG's
  standalone launcher and asserted about Maven. See the second pass.
- ~~No JUnit 4 / Vintage capture.~~ **Closed by the second pass**, under both
  runners that can execute a JUnit 4 `@Test` — Surefire's JUnit4 provider
  (`maven-junit4-mixed.txt`) and the Vintage engine on the JUnit Platform
  (`gradle-vintage-mixed.txt`). The guess that the failure text differs was
  right, and the difference is smaller and nastier than expected: it is
  whitespace.
- ~~No parallel-execution capture.~~ **Process-level forking closed by the second
  pass** — Surefire `forkCount=2` (`maven-parallel-forks-mixed.txt`) and Gradle
  `maxParallelForks = 4` (`gradle-parallel-forks-mixed.txt`). Surefire
  `<parallel>methods</parallel>` and JUnit 5 `@Execution(CONCURRENT)`, which
  thread *within* one JVM rather than forking several, are still not captured.
- ~~No `mvn -T` / Gradle `--parallel` reactor capture~~ **Closed by the second
  pass** (`maven-parallel-reactor-mixed.txt`, `gradle-parallel-continue.txt`).
  The interleaving claim was right; the *"prefixes lines with the module name"*
  claim was wrong — Maven 3.9.16 prefixes nothing, which is worse.
- ~~No Gradle `--continue` capture~~ **Closed by the second pass**
  (`gradle-continue-mixed.txt`), together with the far more common shape it has
  to be told apart from: the same build *without* `--continue`
  (`gradle-multiproject-halted.txt`), which stops after the first failing module
  and never mentions that the second one did not run.
- **No Kotlin/Scala/Groovy test capture** — these are Java-language fixtures
  only, even though Gradle and Maven both drive those languages through the same
  Surefire/JUnit-XML plumbing.
- **No Develocity / build-scan output**, which Gradle suggests in every fixture
  here (`Run with --scan ...`) and which replaces the failure footer when used.
- **Truncation: none.** The largest Java fixture is `maven-cold-cache.txt` at
  45.7 KB, under the 60 KB ceiling; the largest the second pass added is
  `maven-testng-surefire.xml` at 12.8 KB. Nothing in this section was truncated,
  edited, or reformatted after capture; every byte is what the process wrote,
  absolute scratch paths, hostname and `~/.m2` timings included.


### Second pass — the gaps the first pass named, and three claims it got wrong

The first pass ended with a list of states it could not reach. Every one of them
that this machine can reach has now been captured, and capturing them falsified
three sentences the first pass had written from documentation rather than from
bytes. Those three are the reason this section exists: a gap that is named is
fine, but a gap filled with a plausible guess is the `pytest -q` failure again.

All 18 rows below were re-run on **fresh copies of each project with `target/`,
`build/` and `.gradle/` deleted**, and that pass printed the code each command
returned; all 18 matched the capture run. `gradlew-bootstrap-pass.txt` was
re-verified too, after deleting `~/.gradle/wrapper/dists` so the download ran
again — so unlike the first pass, **no row in this table has an unverified exit
code.** Same toolchain, same `JAVA_HOME=/opt/homebrew/opt/openjdk@17`.

| file | produced by | exit |
|---|---|---|
| `gradlew-bootstrap-pass.txt` | `./gradlew --console=plain --no-daemon test` on the all-pass project, wrapper generated by `gradle wrapper --gradle-version 9.7.1`, with `~/.gradle/wrapper/dists` absent | 0 |
| `maven-testng-mixed.txt` | `mvn -B test` — TestNG 7.10.2 through Surefire 3.5.2's TestNG provider; 6 tests, 3 pass, 1 assertion failure, 1 escaped `NullPointerException`, 1 `SkipException` | 1 |
| `maven-testng-surefire.xml` | `target/surefire-reports/TEST-demo.OrderServiceTest.xml` from that run | — |
| `maven-testng-results.xml` | `target/surefire-reports/testng-results.xml` from the same run — TestNG's own format, written alongside the JUnit-shaped one | — |
| `testng-standalone-mixed.txt` | `java -cp target/test-classes:<deps> org.testng.TestNG -testclass demo.OrderServiceTest` — TestNG's own launcher, no Maven | 3 |
| `testng-standalone-all-pass.txt` | the same launcher with `-methods demo.OrderServiceTest.addsTwoNumbers,…multipliesTwoNumbers,…comparesGreaterThan` — a **narrowed** run over the same failing class | 0 |
| `maven-junit4-mixed.txt` | `mvn -B test` — `junit:junit:4.13.2` through Surefire's JUnit4 provider; 7 tests, 3 pass, 1 `AssertionError`, 1 `NullPointerException`, 1 `@Ignore`, 1 failed `assumeTrue` | 1 |
| `maven-junit4-surefire.xml` | `target/surefire-reports/TEST-demo.OrderServiceTest.xml` from that run | — |
| `gradle-vintage-mixed.txt` | `gradle --console=plain --no-daemon test` — the **same seven JUnit 4 tests**, run on the JUnit Platform via `junit-vintage-engine:5.11.4` under `useJUnitPlatform()` | 1 |
| `gradle-vintage-junit.xml` | `build/test-results/test/TEST-demo.OrderServiceTest.xml` from that run | — |
| `maven-parallel-forks-mixed.txt` | `mvn -B test` with `<forkCount>2</forkCount><reuseForks>false</reuseForks>` over 4 classes (`Alpha`/`Bravo`/`Charlie`/`Delta`, 2 tests each, staggered `Thread.sleep`), 1 failing | 1 |
| `gradle-parallel-forks-mixed.txt` | `gradle --console=plain --no-daemon test` with `maxParallelForks = 4` and `forkEvery = 1` over the same 4 classes | 1 |
| `maven-parallel-reactor-mixed.txt` | `mvn -B -T 2 test` on the same 2-module reactor as `maven-reactor-mixed.txt` | 1 |
| `gradle-multiproject-halted.txt` | `gradle --console=plain --no-daemon test` on a 2-module Gradle build where **both** modules have a failing test | 1 |
| `gradle-continue-mixed.txt` | the same build with `--continue` | 1 |
| `gradle-parallel-continue.txt` | the same build with `--parallel --continue` | 1 |
| `maven-failure-ignore.txt` | `mvn -B test -Dmaven.test.failure.ignore=true` over the mixed project | 0 |
| `maven-fail-never.txt` | `mvn -B --fail-never test` over the mixed project | 0 |

### What the second pass establishes

**`mvn --fail-never` prints `BUILD FAILURE` and exits 0.** `maven-fail-never.txt`
is a run with one assertion failure and one escaped `NullPointerException`, whose
summary reads `Tests run: 6, Failures: 1, Errors: 1, Skipped: 0` and whose build
verdict line reads, in full, `[INFO] BUILD FAILURE` — and the process exits
**zero**. This is the only fixture in the whole directory where the text and the
exit code disagree in that direction, and it is the one that matters: an
execution attestation that trusts the exit code and never reads the bytes marks
this run green. Its sibling `maven-failure-ignore.txt`
(`-Dmaven.test.failure.ignore=true`) is the inverse — `BUILD SUCCESS`, exit 0,
over the same honest `Failures: 1, Errors: 1` summary — and it is the Maven
counterpart to `gradle-ignore-failures.txt`, with one difference that matters for
§6.5: Gradle's `ignoreFailures` lives in `build.gradle` and is invisible to a
reader of argv, while both Maven spellings are **in argv**, so a narrowing-style
argv check can see them.

**The TestNG claim was wrong, and wrong in the expensive direction.** The first
pass wrote that Surefire's TestNG provider *"prints a different summary
(`Total tests run: N, Passes: N, Failures: N, Skips: N`)"*. It does not.
`maven-testng-mixed.txt` prints

    [ERROR] Tests run: 6, Failures: 2, Errors: 0, Skipped: 1

which is Surefire's grammar, character for character the same as the JUnit
provider's. `Total tests run:` is **TestNG's own launcher's** line and appears
nowhere in a Maven build; `testng-standalone-mixed.txt` is where it lives. So a
Maven parser needs no TestNG branch at all, and a parser that had added one for
`mvn test` would have carried dead code that looked like coverage.

What TestNG *does* change is the numbers. The escaped `NullPointerException`
lands in `Failures: 2, Errors: 0`, where the identical code under JUnit 5 is
`Failures: 1, Errors: 1` — TestNG has no error/failure distinction, so Surefire's
`Errors:` column is structurally always `0` under TestNG. Reading `Failures`
alone is exact on TestNG output and under-counts on JUnit output; reading
`Failures + Errors` is correct on both. `maven-testng-surefire.xml` mirrors it:
`errors="0" failures="2"`, two `<failure>` elements, no `<error>` anywhere.

**`maven-testng-results.xml` is not JUnit XML and must be refused, not read.**
TestNG writes this format under that name in every project; the fixture carries
the `maven-` prefix only to say which run produced it. Its root
element is

    <testng-results ignored="0" total="6" passed="3" failed="2" skipped="1">

— no `<testsuite>`, no `tests=`, no `failures=`. Surefire writes it into the
*same* `target/surefire-reports/` directory as the JUnit-shaped report, so a
reader that globs that directory for XML will find both. A JUnit-XML reader
handed this file finds no recognised element and must say `unverifiable`; if it
sums the `<testsuite>` attributes it finds, it finds none, and a zero total over
a directory that clearly contains a report is exactly the collapse §6.2 forbids.

**TestNG's own launcher exits 3, and says nothing about what failed.**
`testng-standalone-mixed.txt` is 380 bytes for a run with two failures: three
SLF4J warnings, a banner, one tally line, a banner. No test name, no exception
class, no stack trace, no file, no line — grep the file for `appliesTheBulkDiscount`,
for `at demo.`, for `Exception`, and all three come back zero. And the exit code
is **3**, which is neither the 0 a passing run gives nor the 1 every other runner
in this directory gives for failures. `testng-standalone-all-pass.txt` is the
byte-for-byte same file with `Total tests run: 3, Passes: 3, Failures: 0, Skips: 0`
and exit 0 — and it is also a **narrowed** run (`-methods …`) over the same class
whose full run fails, so TestNG joins Maven and Gradle in leaving no trace of the
filter in its output.

**JUnit 4 differs from Jupiter by whitespace, which is the hardest kind of
difference to notice and the easiest to write a parser past.**

    JUnit 4   expected:<90> but was:<85>
    Jupiter   expected: <85.0> but was: <90.0>

A matcher keyed on the literal `expected: <` — the form every existing Java
fixture here shows — matches nothing in `maven-junit4-mixed.txt`. The thrown
class differs too (`java.lang.AssertionError` against
`org.opentest4j.AssertionFailedError`), and JUnit 4 puts the assertion message
before the expectation rather than after a ` ==> ` separator. The summary
grammar, though, is once again Surefire's:
`Tests run: 7, Failures: 1, Errors: 1, Skipped: 2`.

**The parenthesis rule is an engine rule, not a producer rule.** The first pass
recorded that *"Gradle keeps the parentheses, Surefire strips them"*. That is
true of the Jupiter fixtures and false in general. `gradle-vintage-mixed.txt` is
Gradle, same version, same `useJUnitPlatform()`, and its per-test lines read

    OrderServiceTest > comparesGreaterThan PASSED

with no parentheses, because the Vintage engine reports JUnit 4 display names
without them. `gradle-vintage-junit.xml` carries the same
`testcase name="comparesGreaterThan"`. So the same producer emits both forms
depending on which engine ran the test, and a reader that matches a test name
across two reports cannot assume either form.

**A JUnit 4 `@Ignore("reason")` loses its reason, and the loss is asymmetric.**
In `gradle-vintage-junit.xml` the `@Ignore("flaky against the sandbox carrier API")`
test is a bare

    <skipped/>

while the failed assumption beside it keeps everything:

    <skipped message="org.junit.AssumptionViolatedException: no carrier credentials in this environment"
             type="org.junit.AssumptionViolatedException">

The Jupiter fixtures have it the other way round — `@Disabled`'s reason survives
as `<skipped message="…">` and the aborted test is typed
`org.opentest4j.TestAbortedException`. So across the four producers here, neither
the skip *reason* nor the skip *kind* survives uniformly, and a surface that
promises to say why a test was skipped will have nothing to say for a JUnit 4
`@Ignore`. Vintage also collapses the `NullPointerException` into `<failure>`
with `errors="0"`, the same flattening Gradle does for Jupiter.

**Parallelism breaks attribution but not the aggregate — under both build
tools.** This is the reassuring half, and it is worth having measured rather than
assumed. In `maven-parallel-forks-mixed.txt` (`forkCount=2`, `reuseForks=false`)
two `Running demo.…Test` lines appear before any result line, and the per-class
results come back in the order Delta, Bravo, Alpha, Charlie for classes declared
Alpha, Bravo, Charlie, Delta:

    [INFO] Running demo.BravoTest
    [INFO] Running demo.DeltaTest
    [INFO] Tests run: 2, … -- in demo.DeltaTest
    [INFO] Running demo.AlphaTest
    [ERROR] Tests run: 2, Failures: 1, … <<< FAILURE! -- in demo.BravoTest

The final `Tests run: 8, Failures: 1, Errors: 0, Skipped: 0` is still correct and
still last. So an aggregate reader is safe; a reader that pairs each `Running X`
with the next `Tests run:` attributes results to the wrong class — which is why
the `-- in demo.XTest` suffix, not the position, is the only sound way to
attribute a per-class line. `gradle-parallel-forks-mixed.txt` is the same story:
per-test lines interleave across four classes and
`8 tests completed, 1 failed` is still correct and still single.

**`mvn -T` prefixes nothing, and that is worse than the prefix the first pass
predicted.** `maven-parallel-reactor-mixed.txt` announces itself once —

    [INFO] Using the MultiThreadedBuilder implementation with a thread count of 2

— and after that the two modules' output is one undifferentiated stream. Grep the
file for a `[INFO] [core]`-style prefix and the count is zero. What actually
happens is that the two ` T E S T S` banners interleave into a run of four
`-------` rules, both modules' `Running` and `Tests run:` lines land in the same
block, and then two separate `Results:` blocks follow —
`Tests run: 3, Failures: 0, Errors: 0, Skipped: 0` and
`Tests run: 3, Failures: 1, Errors: 1, Skipped: 0`. **Nothing in the file
attributes a test class to a module**, and nothing sums the two tallies. The
`Reactor Summary` still names `core … SUCCESS` and `web … FAILURE`, so the only
per-module truth in the file is a pass/fail word, never a count.

**`gradle-multiproject-halted.txt` has the worst summary-to-truth ratio of any
fixture in this section.** It is `gradle test` over two modules, each containing
one failing test — four tests, two failures, in the repository. `:core:test`
fails, Gradle stops, and `:web:test` never runs. The only count anywhere in the
file is

    2 tests completed, 1 failed

and the string `web` **does not occur in the file at all**. There is no
`SKIPPED`, no `NOT RUN`, no mention that a second module exists. A parser reads
2 and 1 and is not wrong about what Gradle told it; it is wrong about the
repository, and nothing in the bytes can tell it so. The honest reading of a
Gradle multi-project run is that its counts are a lower bound unless every
`:*:test` task in the build appears in the output.

`gradle-continue-mixed.txt` is the same build with `--continue`, and it fixes the
missing module while creating a different trap: it prints
`2 tests completed, 1 failed` **twice**, once per module, with nothing
aggregating them, and closes with

    FAILURE: Build completed with 2 failures.

That `2` is a count of failed **tasks**, not of failed tests. It happens to equal
the number of failed tests here and would not if either module had two failures.
First-match and last-match both read 2 tests where 4 ran; only summing every
occurrence is right, and the number that most looks like a total is a different
quantity. `gradle-parallel-continue.txt` adds `--parallel` and shows that even
the order is unstable — `> Task :web:compileJava` lands before
`> Task :core:compileJava`, so the module that reports first is a race.

**The `./gradlew` preamble is two lines and a row of dots, and there is no
`Welcome to Gradle` in it.** The first pass predicted the wrapper's first run
would differ from `gradle test` by a download line *"+ `Welcome to Gradle x.y!`"*.
Grep `gradlew-bootstrap-pass.txt` for `Welcome to Gradle`: zero. What is actually
there is

    Fetching distribution.
    Downloading https://services.gradle.org/distributions/gradle-9.7.1-bin.zip
    ..............10%..............20%...............30%…100%

— a single progress line carrying digits and `%` signs, ahead of output that is
otherwise byte-identical to `gradle-all-pass.txt`. The warm second run was
captured too and is byte-identical to `gradle-all-pass.txt` apart from the
project path and the elapsed time, so it is **not** shipped as a separate
fixture; the preamble is the entire difference the wrapper makes.

### States still not captured, and why

- **Surefire `<parallel>methods</parallel>` and JUnit 5 `@Execution(CONCURRENT)`.**
  Both thread *within* a single JVM instead of forking several, which interleaves
  output inside one class rather than between classes. The forked shapes are
  captured; the threaded ones are not, and a parser must be assumed unproven
  against them.
- **TestNG suite XML (`-suitexmlfile testng.xml`) with `<groups>`.** Only the
  class-and-method form is captured. A suite file changes the suite and test
  names in `maven-testng-results.xml` away from the `Command line suite` /
  `Surefire suite` defaults these fixtures carry.
- **No Develocity / build-scan output.** Every Gradle fixture here ends with
  `Run with --scan …`, and `--scan` replaces the failure footer with a scan URL.
  Producing one requires accepting Develocity's terms and publishing the build
  to a third-party service, which is not a thing to do to capture a fixture.
- **No Kotlin/Scala/Groovy test capture.** Unchanged from the first pass, and out
  of scope: these are Java-language fixtures, even though the same Surefire and
  JUnit-XML plumbing carries those languages.
- **Truncation: none in this pass either.** The largest file added is
  `maven-testng-surefire.xml` at 12.8 KB. Every byte is what the process wrote.
