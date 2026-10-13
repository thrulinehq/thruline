# The receipt specification

The receipt format is a public specification, not an internal detail. It has its
own document, its own JSON Schema, and a worked example taken from a real
session:

| | |
|---|---|
| Predicate type | `https://thrulinehq.com/attestation/execution-check/v0.1` |
| Specification | [`docs/spec/execution-check-v0.1.md`](https://thrulinehq.com/spec/execution-check/v0.1) |
| JSON Schema | `docs/spec/execution-check-v0.1.schema.json` |
| Example | `docs/spec/example-receipt.json`, `docs/spec/example-statement.json` |
| Validator | `docs/spec/validate.py` |

## Why it is specified separately

A receipt is meant to outlive the tool that wrote it and to be checked by
software that is not ThruLine. That only works if the format is written down to
the level where a second implementation can be built from the document — the
DSSE PAE byte layout with a test vector, the three states of `policy_hash`, the
exact conditions under which a verifier refuses a receipt for disagreeing with
itself.

Read it if you are:

- writing a CI job that emits receipts without installing ThruLine;
- writing a verifier, a policy engine or a dashboard that consumes them;
- checking whether the claim ThruLine makes about a receipt is one you can
  independently confirm.

## Running the validator

```sh
python3 docs/spec/validate.py                       # self-test
python3 docs/spec/validate.py path/to/receipt.json  # check one
```

With no argument it validates the committed example and then makes ten targeted
mutations, each of which must be caught, plus a positive control that the
unmutated example still passes. A schema that matches everything passes
vacuously; the negative controls are how the word "valid" is earned.

It checks **shape**. The cross-field invariants — the tree binding, the resolution
of claim ids, the rule that a Couldn't-check verdict cannot produce a verified
conclusion — are a verifier's job, and the schema names each one it cannot
express rather than leaving the gap silent.

## Standardisation

A draft pull request to the [in-toto attestation predicates
registry](https://github.com/in-toto/attestation) is written and **not opened**;
it lives at `docs/spec/in-toto-pr/`, together with the three preconditions that
are not met yet — the specification URL does not resolve, the implementation is
not published, and the registry's current contribution process has not been read
at the version in force.

Publishing a registry entry whose `predicateType` URL 404s would be an odd move
for a predicate about not taking things on faith.

## Relationship to TRACE

[Mapping to TRACE](./trace.md) — field by field, including the four places the
mapping is easy to get wrong.
