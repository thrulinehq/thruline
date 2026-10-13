# Draft pull request — in-toto/attestation

**Target:** <https://github.com/in-toto/attestation>
**Branch:** `predicates/execution-check-v0.1`
**Adds:** `spec/predicates/execution-check.md`
**Status:** DRAFT. Not opened. See "Before opening this" below — three
preconditions are unmet, and opening it before they are met would put a
registry entry behind a URL that does not resolve.

---

## Title

`Add execution-check predicate: binding an agent's completion claim to the execution behind it`

## Body

### What this adds

A predicate, `https://thrulinehq.com/attestation/execution-check/v0.1`, for
attestations produced when an autonomous coding agent finishes a task and states
a result.

Existing predicates in this repository answer *where an artifact came from*.
SLSA Provenance names the builder, the source and the parameters. This one
answers a different question, and the difference is the reason for a separate
predicate rather than a Provenance extension:

> An agent wrote "all tests pass." Is there a process that ran, exited zero, and
> ran against **this** tree — and what does the arithmetic between those two
> facts come to?

The predicate carries three lists and the relation between them:

- `claims` — typed assertions extracted from what the agent wrote, with the
  agent's own sentence retained for display.
- `checks` — commands that actually executed, each bound to the git tree it ran
  against, with argv/env/cwd digests, exit code and a parsed runner summary.
- `verdicts` — for each claim, a deterministic `supported` / `unsupported` /
  `unverifiable`, naming the checks it rests on.

The claim is not the attestation. The relation is.

### Why it is not SLSA Provenance with extra fields

Provenance attests that a build platform produced an artifact from named inputs.
Its subject is an output artifact, and its trust model rests on the builder's
integrity.

This predicate's subject is a **git tree**, usually uncommitted, and its trust
model assumes the party being described may be motivated to look successful. So
its content is adversarial where Provenance's is descriptive:

- Every check carries `input_tree_hash`, and a check bound to a different tree
  than the subject is not evidence for the subject. "The tests passed" and "the
  tests passed on this code" are different statements, and agents routinely
  produce the first while meaning to produce the second.
- `verdicts` has a third value. `unverifiable` is a first-class outcome that
  MUST NOT collapse into `supported`. Provenance has no equivalent, because it
  has no notion of a claim that could not be decided.
- `findings` carry detector output about the *tests themselves* — assertions
  removed, skip markers added, a run narrowed to one test — which is meaningless
  for a build and central here.
- `capture.capture_degraded` states, on the artifact's face, that the producer
  did not observe enough to be trusted about the rest of it.

Squeezing this into `buildDefinition.externalParameters` would put an
adversarial evidence model inside a descriptive one and lose the third verdict
value, which is the part that does the work.

### Relationship to TRACE

The Linux Foundation's TRACE Trust Record (2026) covers the runtime and hardware
layer: which model, in which enclave, on which hardware, under which policy. It
does not bind a *result claim* to a *computation*. The two compose — a TRACE
record can say the agent ran in an attested environment while this predicate
says what the agent then claimed and what actually ran. A field-by-field mapping
ships alongside the specification, and this predicate makes no hardware or
runtime attestation claims of its own.

### Specification

Full specification, JSON Schema, worked example and a validator with negative
controls:

- Specification: <https://thrulinehq.com/spec/execution-check/v0.1>
- Repository: `docs/spec/` in the ThruLine repository (Apache-2.0)

### Implementation status

One implementation, ThruLine, produces and consumes these attestations today:
DSSE envelopes over in-toto Statements, Ed25519 local-key signing, offline
verification. Two gaps against this specification are recorded here rather than
left for review to find. §3.3's RFC 8785 canonicalization is **not
implemented**: `thruline receipt` serializes the Statement with
`serde_json::to_vec`, which emits the fields in struct-declaration order —
`_type`, `subject`, `predicateType`, `predicate` — not the RFC's sorted order.
The envelopes are conforming DSSE and verify anywhere, because signer and
verifier both work over the bytes as received; what is not yet true is that two
receipts meaning the same thing hash the same way across producers. The
canonicalizer exists in the same codebase and carries the event chain
(`thruline_core::spec::canonical`); the receipt path does not call it. Sigstore
keyless signing is specified as compatible and is **not implemented** — stated
plainly here rather than implied, because the format allows it and the
implementation does not yet do it.

### Checklist from CONTRIBUTING

- [x] Predicate type URI is versioned and under a domain we control
- [x] Specification is public and versioned
- [x] JSON Schema provided
- [x] Worked example provided
- [ ] URI resolves to the specification *(see preconditions)*
- [x] At least one implementation
- [x] Apache-2.0

---

## Before opening this

Three things are not true yet, and each would make the PR wrong on the day it
was opened. Recorded here rather than discovered in review.

1. **`https://thrulinehq.com/spec/execution-check/v0.1` does not resolve.** The
   registry's whole value is that a `predicateType` can be looked up. A PR whose
   URI 404s is asking reviewers to take the specification on faith, which is a
   poor look for a predicate about not taking things on faith. Serve the
   specification and the schema first.

   *Routed 2026-09-05:* serving that URL is the marketing site's job and sits
   with Track A, not with this track. The document to serve is
   `spec/execution-check-v0.1.schema.json` at the repository root — the
   code-pinned one — and `docs/spec/execution-check-v0.1.md` beside it.

2. **The reference implementation is not published.** No release exists, and the
   branch carrying the attestation crate has never been pushed.

   *Partly met, 2026-09-06:* the operator ruled the core be pushed to a
   **private** repository with full history, releases private until launch. A
   private repository does not satisfy a public registry — a reviewer cannot
   obtain the implementation — so this precondition stands for the PR and is
   met only for internal purposes, once the core names the pushed ref. Making
   it public is a separate launch decision and not this track's. "At least one
   implementation" is checked above because the implementation exists and runs;
   it is not yet something a reviewer can obtain. Publish, then open.

3. **`in-toto/attestation`'s current contribution process has not been read at
   the version in force.** The checklist above is reconstructed from the
   repository's conventions as understood here, not transcribed from
   `CONTRIBUTING.md` at a pinned commit. Read the real one — the process for
   third-party predicates has changed more than once — and reconcile before
   opening.

All three preconditions, and the in-tree/by-reference choice below, are the
operator's call — recorded 2026-09-05, not this track's to make.

A fourth thing worth deciding deliberately: the registry distinguishes predicates
hosted in-tree from third-party predicates listed by reference. This draft
assumes **listed by reference**, with the specification hosted at
`thrulinehq.com`, which keeps versioning under our control. In-tree hosting
would mean the in-toto repository owns the version cadence. The draft file in
`predicate.md` is written to work either way.
