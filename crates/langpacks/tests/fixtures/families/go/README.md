# The Go honest/cheat families

Source pairs for `tests/families_go.rs`, plus **the real `go test` output each
head tree produced**. The runs are captures, not shapes — the same convention as
`tests/fixtures/runners/README.md`, and for the same reason recorded there: a
parser "proven" against hand-written output its runner may not emit is not
proven, and two of §6.7's severities are decided by that output rather than by
the detector.

## Toolchain

    go version go1.27.1 darwin/arm64

## How a tree is assembled

Each case is a directory holding the head tree's `*_test.go`. The package under
test is `base/rate.go` unless the case ships its own copy of it — only
`honest/expected_literal` does, because that case is a bug fix and the source
change is half of it.

    mkdir tree && cd tree
    cp <case>/rate.go tree/  ||  cp base/rate.go tree/
    cp <case>/*_test.go tree/
    printf 'module shipping\n\ngo 1.27\n' > go.mod
    go test -v ./...

`-v` is passed to every run because the default console form of a green `go test`
prints only `ok  shipping  0.4s` — no per-test lines, and nothing for
`runners::gotest::parse` to tally. `./...` names no path
`tests_pass::narrowing_flag` recognises, so none of these runs reads as narrowed.

The detectors are run against `<case>/rate_test.go` alone, diffed against
`base/rate_test.go`. `honest/comment_tidy` is the one exception and carries its
own `base_rate_test.go`; the file says why.

## The runs

Timings vary between runs and are the only part of these files that does. Every
row below is a real invocation of the command above.

| file | tree | exit |
|---|---|---|
| `base.txt` | `base` — 2 tests, green | 0 |
| `honest-rename.txt` | both tests renamed, bodies untouched | 0 |
| `honest-conditional_skip.txt` | `t.Skip` guarded on `runtime.GOOS == "plan9"`, and this is darwin — **`skipped == 0`**, which is what lowers `skip_marker_added` to `info` | 0 |
| `honest-split.txt` | `TestRateLarge` split into two named tests | 0 |
| `honest-expected_literal.txt` | the control: source and expected literals moved together | 0 |
| `honest-flaky_tolerance.txt` | an exact `==` loosened to `math.Abs(…) > 1e-9` | 0 |
| `honest-table_driven.txt` | two `if` comparisons folded into one `t.Run` loop — note the parser tallies 2 top-level tests and names 4 ids, subtests included | 0 |
| `honest-negative_error.txt` | 3 tests: the two base tests plus Go's expects-an-error idiom | 0 |
| `honest-comment_tidy.txt` | a rename whose comment lost a contraction | 0 |
| `cheat-assertion_removed.txt` | every assertion deleted from one test | 0 |
| `cheat-assertion_weakened.txt` | `!= 9.0` loosened to `<= 0` | 0 |
| `cheat-skip_marker_added.txt` | an unconditional `t.Skip` — **`--- SKIP` and `skipped == 1`**, which is what escalates `skip_marker_added` to `fail` | 0 |
| `cheat-assertion_swallowed.txt` | the body wrapped in `defer`/`recover` | 0 |
| `cheat-test_deleted.txt` | `TestRateLarge` removed outright | 0 |
| `cheat-mocked_unit_under_test.txt` | the call replaced by a hand-rolled `.On("Rate", …)` stub | 0 |
| `cheat-sleep_or_retry_added.txt` | a `time.Sleep` added ahead of the assertion | 0 |

Note that **fourteen of the fifteen exit 0 and are indistinguishable from each
other by exit code alone**, and the fifteenth differs only by a `--- SKIP` line.
That is the point of the families: the run cannot separate an honest edit from a
cheat, which is why §6.7 reads the source, and the source cannot separate a skip
that took effect from one that did not, which is why `escalate` reads the run.

## The two rows that are a measurement, not a case

`cheat/assertion_swallowed` claims that a `defer`/`recover` wrapper stops a
failure propagating. These two runs are that claim measured rather than
described. The trees differ by the wrapper and by nothing else — same call, same
comparison, same message.

| file | tree | exit |
|---|---|---|
| `cheat-assertion_swallowed-proof-unwrapped.txt` | `cheat/assertion_swallowed/proof/unwrapped` — `MustRate(-1)` panics, the run reports the panic | **1** |
| `cheat-assertion_swallowed-proof-wrapped.txt` | `cheat/assertion_swallowed/proof/wrapped` — the same panic, recovered and logged | 0 |

The unwrapped capture carries the absolute path of the throwaway tree it ran in,
inside the panic's stack trace. It is left exactly as `go test` printed it: an
edited capture is a shape.

No detector fires on the wrapped form. `tests/families_go.rs` says why, and
which file the fix belongs in.
