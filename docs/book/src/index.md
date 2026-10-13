# What ThruLine is

An AI coding agent finishes a task and tells you it is done. ThruLine reads what
actually happened — the commands that ran, their exit codes and output, the
files that changed, the git trees before and after — and tells you which of
those claims the record supports.

It is not a reviewer and it is not an opinion. Every verdict is computed from
captured execution evidence, and every line of output traces back to an event
you can go and read.

## The one-minute version

```sh
thruline demo
```

That builds a real git repository in a temporary directory, lets a scripted
agent edit a source file, weaken a test rather than fix it, delete another
test's assertion, run **one** narrowed check for real, and then say *"All tests
pass."* Then it prints what the record shows:

```text
The agent said: "Added the handling fee. All tests pass. I'll add a
regression test for the large-weight case in a follow-up."

What the execution record shows:
  tests_pass: unsupported
      · narrowed_run
      · skipped_tests
      · coverage_not_measured
      · assertion_removed
      · assertion_weakened

Open promises (said, not done):
  · I'll add a regression test for the large-weight case in a follow-up.
```

Nothing there is canned. The check really ran, the shim really captured it, and
the verdict was computed by the same verifiers a real session goes through. The
demo needs a real test runner on `PATH`; when it cannot find one it says so and
stops, rather than simulating a run — a simulated test run is the exact thing
this product exists to catch.

Read that output carefully: **the check exited zero.** The tests it ran passed.
The claim is still not proven, because the run covered one of two tests and the
other one lost its assertion in the same change set. Exit code zero is not the
evidence. The relation between the claim and the run is.

## Three states, and the third one matters most

| in the JSON, and on screen | in this book | means |
|---|---|---|
| `supported` | **Proven** | the record contains evidence for the claim |
| `unsupported` | **Not proven** | the record contradicts it, or the evidence that would settle it is absent where it could have been produced |
| `unverifiable` | **Couldn't check** | the record does not contain enough to decide |

The JSON word is also the word you see: `thruline demo`, `claims`, `explain`
and the TUI all print the slug itself, which is why the demo output above reads
`tests_pass: unsupported`. "Proven", "Not proven" and "Couldn't check" are this
book's plain-English names for the three states — they are how these pages
explain the words, not a second vocabulary the tool emits.

**Couldn't check never becomes Proven.** A claim nobody can check is not a claim
that passed, and the whole system is built so that missing evidence cannot
quietly turn into a green check.

The distinction runs the other way too. **Not proven is not an accusation.** It
means the agent claimed something and no proof was found in the record — which
is a statement about the record, not about the agent's honesty. No surface here
collapses that difference, and every negative state says what would settle it.

## What leaves your machine

Nothing. The engine makes no network calls. The only egress in the entire system
is a model provider you point it at yourself, off by default, and never in the
verdict path — see [Privacy](./privacy.md).

## Where to go next

- [Install](./install.md) — including the prerequisite nothing else mentions
- [Quickstart](./quickstart.md) — capture on, first verdict
- [The gate in CI](./ci.md) — the receipt as a PR check
- [Verifier reference](./verifiers.md) — every finding, with an example
- [The receipt specification](./receipt-spec.md) — the format, for people who
  are not using ThruLine
