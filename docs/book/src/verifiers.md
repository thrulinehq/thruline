# Verifier reference

A verifier decides one claim kind from the execution record. It never reads the
agent's prose to decide anything — the prose is what is being checked.

Every verifier is a pure function of `(claim, events, snapshots, checks,
policy)`. No model is anywhere in this path.

## The claim kinds, and what decides each one

| claim kind | decided from | verifier |
|---|---|---|
| `tests_pass` | the test command that ran, its exit code, its parsed summary, the diff, and any re-execution | `tests_pass@1` |
| `build_ok` | a captured build command and its exit code | `build_ok@1` |
| `lint_ok` | a captured linter run and its exit code | `lint_ok@1` |
| `typecheck_ok` | a captured type-checker run and its exit code | `typecheck_ok@1` |
| `behavior_unchanged` | the diff, the public API surface, the dependency manifests, and both trees' pass sets | `behavior_unchanged@1` |
| `migration_added` | the changed-path set | `migration_added@1` |
| `docs_updated` | the changed-path set | `docs_updated@1` |
| `dependency_added` | the changed-path set and its lockfiles | `dependency_added@1` |
| `secrets_absent` | the added lines of the diff | `secrets_absent@1` |
| `file_changed`, `did_x` | the named file or symbol against the changed-path set | `file_changed@1` |
| `task_complete` | nothing, when a phrase matched it — see below | `task_complete@0` |
| *(no claim at all)* | the file changes, and the fact that nothing matched | `unclaimed_completion@1` |

`task_complete` always returns **couldn't check**. Deciding it would need the
harness's task events, which this build does not subscribe to. A verifier with
no evidence returns "couldn't check"; it does not return "fine".

## When nothing was claimed at all

A session that **changed files and made no claim this build recognises** does
not get a clean sheet. `unclaimed_completion@1` synthesises one claim over the
agent's last sentence and returns **couldn't check**:

> this session changed 3 file(s) and stopped without a completion claim this
> build recognises. Handing the work back is itself a completion assertion, and
> nothing in the record proves it: no claim was extracted, so no verifier ran.
> The absence of a claim is not the absence of a problem, and this session may
> not read as verified.

Without it, such a session produced `success` and a green badge — identical to a
proven-good run — because there was nothing to be unsupported. The verifier
string is deliberately its own, not `task_complete@0`, so a reader can tell from
the receipt alone that **no phrase matched** rather than that a phrase matched
and could not be decided.

It fires only when files changed. A session that changed nothing and claimed
nothing is not floored.

## The three states

| in the JSON, and on screen | this page's gloss | means |
|---|---|---|
| `supported` | **Proven** | the record backs the claim |
| `unsupported` | **Not proven** | the record contradicts it, or the evidence that would settle it is absent where it could have been produced |
| `unverifiable` | **Couldn't check** | not enough was observed to decide |

The JSON words are the wire contract and do not change, and they are also the
words a reader sees: `thruline claims`, `explain`, `demo` and the TUI all print
the slug verbatim. The bold words are this page's plain-language gloss for each
state, not a label any surface displays. **Couldn't check never becomes
Proven** — not in a verifier, not in a receipt, not in a policy decision, not in
a check run. Combining verdicts moves away from Proven, never toward it.

## How a finding changes a verdict

A finding is one named observation attached to a verdict.

- **fail** — on its own, enough to hold the claim at **Not proven**.
- **warn** — worth a reader's attention. It does not sink the claim by itself.
- **info** — context.

Several findings change severity by context rather than carrying a fixed one.
`assertion_weakened` and the three source-side findings under "code that knows
it is being tested" are noted at their own entries below. Two rules are here:

- `narrowed_run` and `narrowed_paths` are **fail** when the claim was about
  *everything* ("all tests pass") and **warn** when it named a scope a narrowed
  run can honestly cover.
- `skip_marker_added` starts at **warn**, becomes **fail** when the run reported
  skipped tests (the marker took effect), and drops to **info** when it did not
  (the marker left nothing unrun).

That second rule exists because the detector sees a marker, and only the run
says whether it did anything. Firing `fail` on the marker alone accused every
deliberate `@pytest.mark.skipif(sys.platform == "win32")` of cheating.

---

## The findings

Every finding a verifier in this build can emit, with a real example of its
output. The examples are the detectors' own sentences, with paths and names
filled in.

### About the run itself

**`test_run_failed`** · fail · `tests_pass`
The run the claim rests on exited non-zero — or exited 0 while its own parsed
summary reported failing tests, in which case the finding still reads
`exited 0` and the failure count is what decided it.
> `pytest -q` exited 1

*What would prove it:* a captured run that exits 0 on the final tree.

**`build_failed`, `lint_failed`, `typecheck_failed`** · fail · the tool verifiers
The captured command for that category exited non-zero. The name follows the
category, so a new category brings its own finding.
> `cargo clippy --all-targets` exited 101

*What would prove it:* the same command, exiting 0, on the final tree.

**`narrowed_run`** · fail when the claim was about everything, else warn · `tests_pass`
The run exercised a subset of the suite. The archetype of a true sentence about
a smaller thing than the claim.
> `pytest -q -k test_rate_small` narrows the run with `-k`, so it exercised a subset of the suite

*What would prove it:* the suite run without a selection flag, on the final tree.

**`narrowed_paths`** · fail when the claim was about everything, else warn · `build_ok`, `lint_ok`, `typecheck_ok`
The command was limited to part of the tree.
> `ruff check src/api` was limited to src/api, so it exercised part of the tree

*What would prove it:* the same command over the whole tree.

**`narrowing_unchecked`** · warn · all check verifiers
The command behind the check is not in the record, so the narrowing detector
could not look at it.
> the command behind this lint check is not in the record, so §6.4's narrowed-path detector could not look at it — an absence of `narrowed_paths` here is not evidence

**An absence of `narrowed_run` under this finding is not evidence that the run
was not narrowed.** This is the difference between a detector that looked and
found nothing, and a detector that could not look; a report that spelled them
the same way would be lying by omission.

*What would prove it:* capture that records argv — the `PATH` shim, or the run
under `thruline run`.

**`check_bypassed`** · fail · `build_ok`, `lint_ok`, `typecheck_ok`
The command carries a flag that suppresses the very check the claim is about.
> `ruff check --exit-zero .` carries `--exit-zero`, which suppresses the very check the claim is about — exiting 0 says nothing

*What would prove it:* the same command without the bypass flag.

**`ran_before_final_edit`, `tests_ran_before_final_edit`** · fail · the first
from the tool verifiers, the second from `tests_pass`
The check ran, and then the code changed. What passed is not what is being
claimed for.
> superseded: `src/rate.py` changed at seq 42, after this check ran

*What would prove it:* the same check re-run after the last edit.

**`nondeterministic`** · fail · `tests_pass`
The same check ran more than once on the same tree with different outcomes. A
flaky run is not evidence that the suite passes, in either direction.
> `pytest -q` ran twice on the same tree with different outcomes; a flaky pass is not evidence

*What would prove it:* consecutive agreeing runs on the final tree.

**`skipped_tests`** · warn · `tests_pass`
The run skipped tests.
> 1 skipped and 0 xfailed of 2 — they were not run

*What would prove it:* a run in which they execute, or a claim scoped to what did.

**`test_changed_without_source_change`** · warn · `tests_pass`
Test files changed with nothing in the source to necessitate it.
> test file(s) changed with no non-test source change to necessitate it: tests/test_rate.py

### About re-execution

These appear when a check was re-run in a clean container at the final tree. A
re-execution can **confirm** or **contradict**; it can never promote a verdict
on its own. Running `thruline reexec` on an unsupported session does not improve
it — that would make re-execution a laundering step.

**`reexecution_confirms`** · info
> a re-execution of `pytest -q` in sha256:9f2c… agreed with the agent's own run

**`reexecution_contradicts`** · fail
> a re-execution of `pytest -q` in sha256:9f2c… disagreed with the agent's own run

*What would prove it:* nothing in this session. The clean-room run and the
agent's run report different things about the same tree.

**`reexecution_failed`** · fail
> `pytest -q` exited 1

**`reexecution_flaky`** · fail
Re-executions disagreed with *each other*, so no run of that command — the
agent's included — is evidence.
> re-executions of `pytest -q` in sha256:9f2c… did not agree with each other, so no run of it — the agent's included — is evidence that the suite passes

*What would prove it:* a deterministic test. Flakiness is the finding.

**`reexecution_superseded`** · fail
The re-execution is itself evidence for a tree that is not the final one.
> the re-execution of `pytest -q` is itself evidence for a tree that is not the final one — superseded: `vendor/sitecustomize.py` changed at seq 42, after this check ran

**`reexecution_unbindable`** · warn
> the re-execution of `pytest -q` could not be bound: the check ran on tree 4f2c1a9b3e07 and the record does not place it in the chain, so nothing can be ordered against it

**`reexecution_not_comparable`** · warn
> a re-execution of `pytest -q` ran and could not be compared with the agent's run, so it neither confirms nor contradicts it

**`reexecution_uncompared`** · warn
Ran under `--mode full`, and the agent never ran it, so there was nothing to
compare against.
> `pytest -q` was re-executed under `--mode full`; the agent never ran it, so no comparison was made

**`reexecution_outcome_unrecognised`** · fail
> a re-execution of `pytest -q` recorded comparison outcome "partial", which this verifier does not know how to read — so it neither confirms the agent's run nor can be set aside

An outcome the verifier cannot read is refused rather than ignored. Ignoring it
would make an unrecognised value the safest thing to write.

**`reexecution_did_not_promote`** · warn
> 2 re-execution(s) ran at the final tree, and none of them promotes this verdict: the session's own test evidence does not bind to the tree the claim is about

### About the tests themselves

**`assertion_removed`** · fail
Every assertion in a test is gone.
> `test_rate_large` had 1 assertion(s) at the base tree and 0 at the final tree

*What would prove it:* the assertion restored, and a run that covers it.

**`assertion_count_decreased`** · warn
Some, not all, of a test's assertions are gone.
> `test_rate` had 3 assertion(s) at the base tree and 1 at the final tree

**`assertion_weakened`** · warn, or fail when the assertion is left vacuous
An assertion still exists and checks less than it did. Weakened all the way to
"truthy" is a removal wearing an assert's clothes — `assert rate(2) == 12.5`
becoming `assert rate(2) > 0` — and that case is **fail**.
> `test_rate_small`'s strongest assertion went from Equality to Containment

*What would prove it:* the original assertion, or a stated reason it should be
looser.

**`assertion_swallowed`** · fail
The body was wrapped so a failure no longer propagates.
> `test_rate`'s body was wrapped so a failure no longer propagates

**`skip_marker_added`** · warn, escalating — see above
> `test_rate_large` gained a skip/ignore marker in this session — and the run left 1 test(s) unrun, so the marker took effect

**`test_deleted`** · fail
> `test_rate_large` existed at the base tree and does not at the final tree, and no added test carries its body

**`test_renamed`** · info
A rename, not a deletion — the body is substantially the same. Separated from
`test_deleted` so a legitimate rename is not reported as a removal.
> `test_rate` was renamed to `test_shipping_rate` — the body is substantially the same, so this is a rename and not a deletion

**`test_added`** · info
> `test_rate_zero` is new in this session

**`mocked_unit_under_test`** · warn
The test replaced the thing the change is about.
> `test_rate` gained a mock of `rate`, which resolves to a symbol changed in this session — the test no longer exercises the code the change is about

**`sleep_or_retry_added`** · warn
> `test_rate` gained a sleep or retry loop

### About code that knows it is being tested

These read the *source* side of the diff. Each is **warn** alone, and escalates
to **fail** when a fail-severity test finding appears in the same change set.

**`test_environment_sniffing`** · warn
> a branch reading `PYTEST_CURRENT_TEST` was added to source in this session: `if os.environ.get("PYTEST_CURRENT_TEST"):` — code that behaves differently under test proves nothing when tested

The environment names this detector looks for include `PYTEST_CURRENT_TEST`,
`NODE_ENV` and `CI`. Those are **strings it searches for**, not findings — the
finding is always `test_environment_sniffing`.

**`branch_on_test_literal`** · warn
> a branch keyed on `12.5` — a literal that appears verbatim in this session's tests — was added to source: `if n == 2: return 12.5`

**`returns_test_expected_constant`** · warn
> a `return 12.5` was added, and `12.5` is a value this session's tests expect

### About a refactor claim

`behavior_unchanged` is the claim most often made without evidence, so it is
verified strictly: any one of these holds it at **Not proven**.

**`test_files_changed`** · fail
> a refactor claim cannot rest on tests that were themselves edited: tests/test_rate.py

**`dependencies_changed`** · fail
> a dependency manifest changed, so the code being compared is not the same code: Cargo.toml

**`public_api_changed`** · fail
> `rate` changed in the public interface of src/rate.py

**`pass_set_changed`** · fail
The suite reported different results on the two trees.
> the suite reported different results on the two trees: {(4, 3), (4, 4)}

Each element is one parsed run's `(tests_total, passed)` pair. The detector
compares those two counts across the trees; it does not name which test moved.

*What would prove it:* the same pass set on the base tree and the final tree,
with no test file edited and no public signature changed.

### About coverage and the diff

**`coverage_not_measured`** · info
No coverage artifact named any changed file. **Not the same as low coverage**,
and reported separately for exactly that reason: one is a measurement, the other
is the absence of one.
> changed-line coverage was not measured: no coverage artifact was found (looked for lcov.info, coverage.xml, cobertura.xml)

*What would prove it:* a run that writes `lcov.info`, `coverage.xml` or
`cobertura.xml`.

**`low_changed_line_coverage`** · warn
> 34 of 51 changed executable lines are covered (66.66%), below the 70% floor

**`lockfile_not_updated`** · warn
> Cargo.toml changed and none of its lockfiles (Cargo.lock) did

**`secret_in_diff`** · fail · `secrets_absent`
> a github-shaped value appears in src/config.py

*What would prove it:* the value out of the diff and the credential rotated.

---

## What the absence of a finding means

Nothing on its own.

A finding is a detector's positive report. Its absence means one of two things —
the detector looked and found nothing, or the detector could not look — and
those are different facts. Where the second is true, the record says so with its
own finding (`narrowing_unchecked`, `coverage_not_measured`,
`reexecution_unbindable`). Where a detector does not exist for a language or a
runner at all, the claim comes back **Couldn't check** rather than **Proven**.

Read a clean verdict as "the record backs this claim", never as "nothing is
wrong with this code."

## Reading a verdict

```sh
thruline explain --last
```

Every finding names the event it came from. Every event names the file and line
of the raw transcript or spool it was captured into. There is no step in that
chain where you have to take ThruLine's word for it.
