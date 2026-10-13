# Predicate type: Execution Check

Type URI: `https://thrulinehq.com/attestation/execution-check/v0.1`
Version: 0.1
Status: draft
Author: ThruLine (Apache-2.0)
Full specification: <https://thrulinehq.com/spec/execution-check/v0.1>

## Purpose

Binds a completion claim made by an autonomous coding agent to the execution
that does or does not back it, over an exact git tree.

Where SLSA Provenance attests that a build platform produced an artifact,
Execution Check attests **what was asserted, what actually ran, and what the
relation between them comes to** — including the case where the relation cannot
be determined, which is a first-class outcome rather than an omission.

## Prerequisites

The producer observed the session's process executions and file changes directly
— through harness hooks, a `PATH` shim, or by running the agent under a wrapper
— and recorded them in an append-only, hash-chained log. A producer that only
read the agent's own transcript sets `capture.capture_degraded` to `true`, and
every claim in such an attestation is at most `unverifiable`.

## Model

The subject is a git tree, and usually an uncommitted one: agents are judged
before they commit.

```
claim ──asserted by the agent──▶  "all tests pass"
check ──observed by the producer──▶ pytest -q -k one_test, exit 0, on tree c324df6…
verdict ──computed deterministically──▶ unsupported, because the run covered one of two
                                        tests and the other lost its assertion
```

No model is in the verdict path. Claim *extraction* may use one; a verdict is
produced only by deterministic code over recorded evidence, and a producer that
cannot decide emits `unverifiable`.

## Schema

```jsonc
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [{
    "name": "git+<remote-or-path>",
    "digest": { "gitTree": "<hex>", "gitCommit": "<hex>" }   // gitTree required
  }],
  "predicateType": "https://thrulinehq.com/attestation/execution-check/v0.1",
  "predicate": {
    "session": {
      "id": "…", "harness": "claude-code|codex|gemini|generic",
      "model": "…",                                          // optional
      "started": "<RFC 3339 UTC>", "ended": "<RFC 3339 UTC>", // optional
      "event_chain_head": "sha256:…",
      "event_count": 1042
    },
    "claims": [{
      "id": "…", "kind": "tests_pass", "text": "All tests pass.",
      "asserted_by": { "harness": "…", "model": "…" }
    }],
    "checks": [{
      "id": "…", "category": "test", "runner": "pytest",
      "argv_hash": "sha256:…", "env_hash": "sha256:…", "cwd_hash": "sha256:…",
      "input_tree_hash": "<hex>",
      "exit_code": 0,
      "parsed": { "tests_total": 42, "passed": 42, "failed": 0, "skipped": 0 },
      "reexecuted": false,
      "container_image_digest": "sha256:…"                   // iff reexecuted
    }],
    "verdicts": [{
      "claim_id": "…",
      "verdict": "supported|unsupported|unverifiable",
      "verifier": "tests_pass@1",
      "evidence": ["chk_…"],
      "reasons": ["ran on the final tree", "42/42 passed"],
      "warnings": ["1 test skipped"],                        // optional
      "findings": [{ "severity": "fail|warn|info",
                     "detector": "narrowed_run",
                     "path": "tests/test_rate.py",           // optional
                     "detail": "…" }]                        // optional
    }],
    "promises": { "open": ["…"], "fulfilled": ["…"] },
    "verifier": { "thruline_version": "…", "mode": "local|clean-sandbox-reexec",
                  "policy_hash": "sha256:…" },
    "capture": { "sources": ["claude-code-hook", "exec-recorder"],
                 "transcript_hash": "sha256:…",              // optional
                 "redactions_applied": 3,
                 "capture_degraded": false,
                 "content_filters": "…",                     // iff degraded by filters
                 "git_env_digest": "sha256:…" }              // optional
  }
}
```

## Fields that carry the security weight

**`checks[].input_tree_hash`** — a check that ran against a different tree than
the subject is evidence about that other tree. A verifier accepts it for the
subject only when the hashes match, or when nothing relevant changed between
them, and must record which rule applied.

**`verdicts[].verdict`** — three values. `unverifiable` MUST NOT be collapsed
into `supported` at any layer. Combining verdicts moves away from `supported`,
never toward it.

**`checks[].reexecuted`** — `true` requires `container_image_digest`, which
comes from an execution event inside the hash chain the attestation names. A
bare boolean is a claim; the digest is what makes it evidence. The inverse also
holds: a digest with `reexecuted: false` is refused.

**`verifier.policy_hash`** — three distinct states. A digest; **absent**, which
is refused; and **empty string**, which is refused under its own reason. "No
policy" is said with the well-known digest of the empty policy, never with `""`.

**`capture.capture_degraded`** — states on the attestation's face that the
producer did not observe enough to be trusted about the rest of it. Consumers
cap every claim in a degraded attestation at `unverifiable`.

**`checks[].parsed`** — never `{}`. An empty object reads as a clean run with no
tests; an unparsed run states that it is unparsed.

## Signing and canonicalization

DSSE envelope, `payloadType` `application/vnd.in-toto+json`. The Statement is
serialized with RFC 8785 before signing. Signatures cover the DSSE PAE of the
raw payload bytes, not the base64 transport form.

## What this predicate does not attest

- Hardware or runtime environment. That is TRACE's layer; the two compose.
- That a check is *the right check*. It attests that a check ran and what it
  reported. Whether the test tests the right thing is a human question.
- Correctness of the code. `supported` means the record backs the claim, not
  that the software is correct.

## Example

See <https://thrulinehq.com/spec/execution-check/v0.1> §8 for a complete signed
attestation in which a passing, exit-zero test run yields `unsupported` —
because it covered one of two tests and the other lost its assertion in the same
change set.
