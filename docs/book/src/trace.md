# Mapping to TRACE

The Linux Foundation's [TRACE](https://github.com/agentrust-io/trace-spec)
(Trust, Runtime Attestation and Compliance Evidence) is the runtime and hardware
layer for agentic AI. ThruLine interoperates with it and does not duplicate it.

The full field-by-field mapping is
[`docs/spec/trace-trust-record-mapping.md`](https://thrulinehq.com/spec/execution-check/v0.1),
read against `schema/trace-claim.json` at TRACE v0.2 on 2026-09-05. The short
version:

## They answer different questions

TRACE answers **"was this a trustworthy execution environment?"** — which
hardware, which model weights, which policy bundle, which data class, appraised
by which verifier.

A receipt answers **"did the result the agent claimed actually come from a
computation, over this exact source?"**

The gap between them is the whole reason both exist:

> A TRACE record can be **perfect** while an agent deletes a test's assertion,
> runs one narrowed check, and writes "all tests pass." Nothing in the TRACE
> record is false. The enclave really did attest; the weights digest really does
> match. TRACE describes the *environment* of the computation. It does not
> evaluate the *claim*.

A receipt is the converse: it says nothing about hardware, and it will hold the
claim "all tests pass" at `unsupported`, naming the narrowed run and the removed
assertion. It does not say a test failed — it says the record does not support
the claim.

## Joining them

TRACE's optional `references` array points at a receipt by digest. Both records
travel; neither is merged into the other. A single document carrying a hardware
measurement next to a verdict invites a reader to believe the hardware attested
the verdict, and it did not.

## What is not built

A TRACE export **does not exist.** The build spec names an
`export --format trace`; the shipped `export` verb is a different command that
derives a project into the UI's JSON contract, and it has no such flag. No TRACE
record has been produced from a receipt here, and none has been validated against
TRACE's schema.

It is written that way on purpose. A test walks these chapters and fails if they
type a command the binary will refuse — including in a sentence explaining that
it will refuse it.

*What would prove it:* an emitted TRACE record that validates against a pinned
`trace-claim.json`, in a test, with the schema vendored at a recorded commit.

The mapping document also carries four open questions for the TRACE workstream —
including that `tool_transcript` commits to a transcript's contents but cannot
detect a truncated one, which is a gap a hash alone does not close.
