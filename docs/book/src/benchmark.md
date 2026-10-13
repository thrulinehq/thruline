# Benchmark methodology

The honest state of this chapter: **no headline number is published, and the
reason is written down.** The tables below are the shape a published result will
take; the cells are empty because nothing has filled them.

## What would be measured

Two claims, from the build spec's own targets:

| metric | target | reported |
|---|---|---|
| AUROC of Not-proven verdicts against false-success labels | ≥ 0.85 (published LLM-judge baseline ≤ 0.65) | *not measured* |
| catch rate on test-modification cheats | ≥ 80% | *not measured* |
| false-positive rate on honest passes | < 5% | *not measured* |

And, per detector, an ablation — remove one detector and re-measure, so the
headline is attributable rather than collective:

| detector | catch rate with | without | Δ |
|---|---|---|---|
| `assertion_removed` | — | — | — |
| `assertion_weakened` | — | — | — |
| `skip_marker_added` | — | — | — |
| `narrowed_run` | — | — | — |
| `tests_ran_before_final_edit` | — | — | — |
| `returns_test_expected_constant` | — | — | — |
| `branch_on_test_literal` | — | — | — |
| `test_environment_sniffing` | — | — | — |

## Provenance classes

A number's meaning depends on where its corpus came from, so every measurement
carries a class, and each class carries the evidence its claim implies rather
than a word someone typed.

**Class 1 — external, labelled.** Third-party trajectories with third-party
labels. Holds the publisher, dataset, version, and *the named field the labels
were read from*. The artifact's sha256 is recorded at fetch and checked at load;
bytes that do not match are not the published corpus.

**Class 2 — generated under protocol.** A third-party harness and task set, run
here, under a protocol hashed **before the first trajectory exists**. The hash
covers the harness, its version, the model, the task ids, the holdout, the
parameters, and the detector configuration. Pre-registering the corpus while
leaving the instrument free would make the whole protocol decorative.

**Class 3 — self-authored.** Hand-written fixtures. Never counted toward a
published target, because a detector measured against fixtures its own author
wrote reports on the fixtures.

A published headline states its class. A number without one is not publishable.

## What is enforced, not promised

- Every trajectory carries the protocol hash in force when it was produced, so a
  corpus assembled from two runs is refused.
- A holdout consumed by an earlier run refuses the next one; the record is an
  append-only ledger, not a flag.
- A run must account for every task the protocol declared. A crash is recorded as
  an unlabelled trajectory, never omitted — silent absence is the selection
  effect pre-registration exists to prevent.
- A label is a *reading*: the field it came from, the raw value, and the label
  derived from them. For class 1 the field must be the one the provenance
  declared.
- Ground truth is three-state. A trajectory the oracle never scored is
  `unlabelled`, and every headline carries the fraction the oracle actually
  labelled, so a catch rate over a third of a corpus cannot read as a catch rate
  over the corpus.
- A false-positive rate is reported per shape of honest work, and refused when
  one shape holds more than half the honest set. A pooled rate over a corpus one
  shape dominates is that shape's rate wearing the corpus's name.

## Why there is no number yet

```sh
thruline bench
```

exits **3** and names every source it probed. As of the last probe:

| source | state |
|---|---|
| τ2-bench | class 1, downloadable — and **zero executed code checks** |
| terminal-bench-1 | third-party harness and oracle, no published runs; the class 2 candidate |
| ImpossibleBench | harness only; the logs come from running it yourself |
| impossible_swebench / impossible_livecodebench | task sets only — no trajectories, no cheat labels |
| AgentLens-Bench | HTTP 401 — gated or absent from this machine |
| EvilGenie | HTTP 404 — the repository named in the build spec is not there |
| AppWorld | reachable, ships zero trajectory files |

τ2-bench is the one that genuinely downloads. Measured over two of its domains:
656 trajectories, 4,993 tool calls, and **zero** executed code checks. Every
verifier here decides from an executed code check bound to a git tree, and
τ2-bench is a tool-calling dialogue benchmark — its tools are
`get_reservation_details` and `toggle_airplane_mode`. An AUROC over it would be a
number about detectors that never ran.

So the blocker is not effort. Of the seven sources the build spec names, **none
currently supplies labelled trajectories containing executed code checks**. The
class 2 route through terminal-bench is open and expensive: it means running a
third-party harness against a pre-registered protocol and paying for the model
calls.

*What would prove it:* a corpus of labelled trajectories in which code checks
actually executed, loaded under a protocol hashed before the first trajectory
existed, with the fraction the oracle labelled printed beside every figure.

## Reproducing this

```sh
fixtures/bench/download.sh   # fetch what is fetchable, digest-pinned, never committed
thruline bench               # load it and print what it contains
thruline bench --html        # the Overclaim Index page, when there is one
```

`thruline bench` reports the counts it computed rather than a number it cannot
honestly produce. That is the whole of its current behaviour, and it is the
correct behaviour: the alternative is a benchmark page with a figure on it.

## The Overclaim Index

The intended public artifact: per harness, per model, per claim kind, the rate of
claims made without evidence. It needs a corpus. It does not have one, and this
page will carry it when it does — with its provenance class, its protocol hash,
and the fraction of the corpus the oracle actually labelled printed beside every
figure.
