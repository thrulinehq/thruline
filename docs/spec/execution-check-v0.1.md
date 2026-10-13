# execution-check/v0.1 — an in-toto predicate that binds a claim to the run that backs it

| | |
|---|---|
| **Predicate type** | `https://thrulinehq.com/attestation/execution-check/v0.1` |
| **JSON Schema** | `https://thrulinehq.com/spec/execution-check/v0.1` |
| **Statement type** | `https://in-toto.io/Statement/v1` |
| **DSSE payload type** | `application/vnd.in-toto+json` |
| **Status** | v0.1. Stable enough to emit against; the version is in the URI so a breaking change is a new URI, never a silent change of meaning. |
| **License** | Apache-2.0 |

This document specifies the predicate on its own terms. A producer that has
never run ThruLine can read it and emit a receipt that ThruLine's verifier
accepts, and a consumer that has never run ThruLine can read it and check one.
Where the reference implementation is the only authority for a detail, this
document says so rather than leaving the reader to infer it.

---

## 1. What this predicate is for

Existing attestation predicates say **an artifact was produced by a builder**.
SLSA Provenance names the build platform, the source, and the parameters; the
consumer learns where a binary came from.

This predicate says something narrower and, for an autonomous agent, more
useful: **a stated result came from a named computation, over an exact tree.**
An agent finishes a task and writes "all tests pass." That sentence is a claim.
Somewhere there was — or was not — a process that exercised the tests, exited
zero, and ran against the same source the agent is asking you to merge. This
predicate carries the claim, the process, and the arithmetic that relates them,
so that a reviewer or a CI gate can decide without re-reading a transcript.

Three parts, and the relation between them is the content:

- **claims** — typed assertions extracted from what the agent wrote.
- **checks** — commands that actually ran, each bound to the tree it ran on.
- **verdicts** — for each claim, the deterministic answer computed from the
  checks, with the evidence named.

A receipt with claims and no verdicts is a transcript. A receipt with verdicts
and no checks is an opinion. The predicate is the three together.

### What it deliberately does not carry

- **No file contents, no prose from the transcript, no prompts.** Digests and
  bounded, redacted identifiers only. A receipt is designed to be published.
- **No narrative about the agent's honesty.** The reference implementation
  extracts "honest failure" language for its human-facing report and keeps it
  out of the predicate on purpose: a receipt is a gating artifact, and any
  field a policy engine can read is a field an agent can learn to write.
- **No hardware or runtime attestation.** That layer is TRACE's; see
  [the TRACE mapping](./trace-trust-record-mapping.md).

---

## 2. Conformance language

**MUST**, **MUST NOT**, **SHOULD**, and **MAY** are as in RFC 2119/8174.

A **producer** builds and signs receipts. A **verifier** consumes them.

Two conformance levels:

- **Emitting conformance** — the receipt validates against the JSON Schema and
  satisfies every invariant in §6. A verifier will parse it and check it.
- **Binding conformance** — additionally, every digest in the receipt was
  computed from the thing it names, and every check record describes a process
  that really ran. No format can enforce this; §7 lists what a verifier can
  recompute and what it can only take on the signature.

A verifier MUST NOT report a receipt as verified on emitting conformance alone.

---

## 3. Envelope

A receipt is a [DSSE](https://github.com/secure-systems-lab/dsse) envelope.

```json
{
  "payload": "<base64 of the Statement's UTF-8 JSON bytes>",
  "payloadType": "application/vnd.in-toto+json",
  "signatures": [ { "sig": "<base64 of the raw signature>", "keyid": "sha256:…" } ]
}
```

- `payload` and `sig` use **standard base64 with padding** (RFC 4648 §4), not
  base64url.
- `keyid` is a hint. A verifier MUST still check the signature, and MUST NOT
  select a key by `keyid` alone.
- `signatures` MUST contain at least one entry. An empty array satisfies "all
  signatures are valid" vacuously, which is how an unsigned artifact gets
  treated as signed; a verifier MUST reject it.

### 3.1 The signed bytes (PAE)

Signatures cover the DSSE Pre-Authentication Encoding of the **raw** payload
bytes — not the base64 text that travels in the envelope.

```text
PAE(type, body) = "DSSEv1" ‖ SP ‖ LEN(type) ‖ SP ‖ type ‖ SP ‖ LEN(body) ‖ SP ‖ body
```

`SP` is a single 0x20 space. `LEN` is the **byte** length in ASCII decimal, with
no padding and no sign. Concatenation is over bytes.

Worked test vector, for `payloadType` `application/vnd.in-toto+json` and a
payload of the three bytes `{}\n`:

```text
DSSEv1 28 application/vnd.in-toto+json 3 {}\n
```

That is `DSSEv1`, space, `28`, space, the 28-byte type, space, `3`, space, then
the three payload bytes. A producer that signs the base64 form instead produces
envelopes nothing else verifies, and the mistake is invisible until a second
implementation reads one — so implement PAE against this vector before
implementing anything else.

### 3.2 Signature algorithms

| algorithm | `sig` contents | status in the reference implementation |
|---|---|---|
| Ed25519 | the raw 64-byte signature | **implemented** |
| ECDSA P-256 / Sigstore keyless | as DSSE specifies | **not implemented** — see below |

The reference implementation signs and verifies **Ed25519 only**, with a local
PKCS#8 PEM key. Sigstore keyless signing (Fulcio certificates, Rekor inclusion
proofs) is named in the ThruLine build spec and is not built; a producer that
emits a Sigstore-signed envelope is emitting a conforming envelope that
ThruLine's current verifier cannot check. That is a limitation of the
implementation, not of the format.

A `keyid`, where present, SHOULD be `sha256:` followed by the lowercase hex
SHA-256 of the raw public key bytes.

### 3.3 Canonical bytes

The Statement payload MUST be serialized with
[RFC 8785 JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785)
before signing: keys sorted by UTF-16 code unit, no insignificant whitespace,
shortest round-tripping number forms. Two receipts that mean the same thing then
hash the same way, and a receipt hash is stable enough to put in a commit
trailer.

**The reference implementation does not do this** — the same shape of
divergence §3.2 records for Sigstore. `thruline receipt` signs
`serde_json::to_vec(&statement)` (`crates/cli/src/receipt_cmd.rs`), which emits
Rust struct-declaration order — `_type`, `subject`, `predicateType`,
`predicate` — where JCS would emit `_type`, `predicate`, `predicateType`,
`subject`; `crates/attest` carries no canonicalizer at all. Such a receipt still
verifies, because a verifier checks the bytes as received (below); what is lost
is hash comparability, so the receipt hash of a ThruLine-emitted receipt cannot
be compared with that of a conforming producer's receipt for the same statement.
`emit_example.py` in this directory does canonicalize.

The requirement stays a MUST, and the divergence is recorded rather than the
requirement relaxed. Canonicalization is what makes a receipt hash mean the same
thing to two producers, which is the whole point of putting one in a commit
trailer; a specification that weakened the rule because its first implementation
had not got there yet would be letting the implementation define the format.

Verifiers MUST verify the signature over the bytes as received, not over a
re-canonicalized re-serialization. Re-canonicalizing before verifying turns a
signature failure into a silent pass whenever the producer's canonicalizer
disagrees with the verifier's.

---

## 4. Statement

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    {
      "name": "git+<remote-url-or-path>",
      "digest": {
        "gitTree": "<40 or 64 lowercase hex>",
        "gitCommit": "<40 or 64 lowercase hex>"
      }
    }
  ],
  "predicateType": "https://thrulinehq.com/attestation/execution-check/v0.1",
  "predicate": { … }
}
```

- `subject` MUST be non-empty, and **every** entry MUST carry `gitTree`.
  in-toto legitimately allows several subjects, and a reader shown a verified
  receipt believes all of them were attested — so a verifier MUST check every
  subject, not `subject[0]`.
- `gitCommit` is OPTIONAL: work is often attested before it is committed.
- Digests are lowercase hex, 40 characters for SHA-1 repositories and 64 for
  SHA-256 repositories. A verifier MUST NOT accept mixed case.
- `name` is advisory and carries no security weight. The binding is the digest.

### 4.1 The tree hash is the whole binding, and it has one sharp edge

`gitTree` MUST be the hash of a git tree object covering the working state the
checks ran against — including files that are untracked but not ignored. The
reference implementation computes it by staging everything into a **temporary
index** (`GIT_INDEX_FILE` pointed at a scratch file) and calling
`git write-tree`, so the user's real index is never touched.

The edge: git applies `.gitattributes` **clean filters** on the way into the
object database. Where a filter is in scope, the tree names the *filtered*
bytes, and the working tree holds the originals. The tree hash then does not
name the bytes anyone looked at. A producer that snapshots under a clean filter
MUST set `capture.capture_degraded` to `true` and describe the filters in
`capture.content_filters` (§5.7). `filter=lfs` is the commonest
`.gitattributes` line in the world, so this is a routine condition, not an
accusation.

### 4.2 The empty tree is not a subject

A verifier MUST reject a subject whose `gitTree` is git's empty tree
(`4b825dc642cb6eb9a060e54bf8d69288fbee4904` for SHA-1,
`6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321` for SHA-256).
The empty tree is byte-identical in every repository on earth, so such a receipt
verifies everywhere and attests to nothing.

---

## 5. The predicate

Every field below is REQUIRED unless marked optional. Unknown fields MUST be
preserved by a producer that round-trips a receipt and MUST NOT be rejected by a
verifier — but see §5.9 on why a verifier must not read meaning into them
either.

```json
{
  "session":   { … },
  "claims":    [ … ],
  "checks":    [ … ],
  "verdicts":  [ … ],
  "promises":  { "open": [], "fulfilled": [] },
  "verifier":  { … },
  "capture":   { … }
}
```

### 5.1 `session`

| field | type | required | meaning |
|---|---|---|---|
| `id` | string | yes | the harness's session identifier, or the producer's own |
| `harness` | string | yes | `claude-code`, `codex`, `gemini`, `generic`, or a producer-chosen name |
| `model` | string | optional | the model that drove the session, where known |
| `started` | string | optional | RFC 3339 UTC, `Z` suffix |
| `ended` | string | optional | RFC 3339 UTC, `Z` suffix |
| `event_chain_head` | string | yes | `sha256:` + 64 lowercase hex |
| `event_count` | integer ≥ 0 | yes | how many events the chain covers |

`event_chain_head` is the head of a hash-chained append-only event log. It is
what makes the receipt checkable against a record rather than merely
self-consistent: a verifier holding the same log can recompute the head and see
whether the predicate describes the record it has.

`event_count` is part of the identity, not a statistic. A **prefix** of a hash
chain is itself a valid hash chain, so a truncated session verifies as a chain
of its own. A verifier that compares the head and not the count accepts a
session with its inconvenient tail removed.

Producers that keep no event chain: see §9.

### 5.2 `claims[]`

| field | type | required | meaning |
|---|---|---|---|
| `id` | string | yes | unique within the receipt |
| `kind` | string | yes | see the table below |
| `text` | string | yes | the agent's own words for this claim |
| `asserted_by.harness` | string | yes | who said it |
| `asserted_by.model` | string | optional | which model said it |

`kind` is an open vocabulary with a well-known core. A verifier that does not
recognize a kind MUST treat its claim as undecidable (§5.4's `unverifiable`),
never as satisfied.

| kind | the assertion |
|---|---|
| `tests_pass` | the test suite passes |
| `build_ok` | the project builds |
| `lint_ok` | the linter is clean |
| `typecheck_ok` | the type checker is clean |
| `behavior_unchanged` | the change is a refactor; observable behavior is the same |
| `migration_added` | a database migration was added |
| `docs_updated` | documentation was updated |
| `dependency_added` | a dependency was added |
| `secrets_absent` | no credential was committed |
| `file_changed` | a named file was changed |
| `did_x` | a named piece of work was done |
| `task_complete` | the assigned task is finished |

`text` is the agent's sentence, carried so a human can see what was actually
written. It is display material: **a verifier MUST NOT parse `text` to reach a
verdict.** The prose is what is being checked.

### 5.3 `checks[]`

| field | type | required | meaning |
|---|---|---|---|
| `id` | string | yes | unique within the receipt; referenced by `verdicts[].evidence` |
| `category` | string | yes | `test`, `build`, `lint`, `typecheck`, `coverage`, `custom` |
| `runner` | string | yes | `pytest`, `cargo test`, `go test`, `jest`, …, or `unknown` |
| `argv_hash` | string | yes | `sha256:` of the canonicalized argument vector |
| `env_hash` | string | yes | `sha256:` of the environment as it affected the run |
| `cwd_hash` | string | yes | `sha256:` of the working directory path |
| `input_tree_hash` | string | yes | the git tree the check ran against |
| `exit_code` | integer | yes | the process's exit status |
| `parsed` | object | yes | the runner's own summary; see below |
| `reexecuted` | boolean | yes | whether this row is a clean-room re-run |
| `container_image_digest` | string | conditional | `sha256:` of the image; see §6.2 |

**`argv_hash` is a hash, and the argv is not in the receipt.** That is
deliberate: a command line can carry a token. It is also a real limitation —
a consumer cannot see from the receipt alone that a test run was narrowed. The
producer is expected to have detected narrowing itself and said so in the
verdict's findings (§5.4). A consumer that needs the argv must go to the
producer's own record.

**`parsed` MUST NOT be `{}`.** An empty object reads as a clean run with no
tests. A check whose output could not be parsed MUST carry a positive statement
that it could not be, for example:

```json
"parsed": { "unverifiable": "no parser for runner `gradle`" }
```

For a run that was parsed, the shape is:

```json
"parsed": {
  "tests_total": 42, "passed": 42, "failed": 0,
  "skipped": 0, "xfail": 0, "duration_ms": 1234, "test_ids": []
}
```

`test_ids` MAY be empty; a producer that can enumerate them SHOULD, because it
lets a consumer detect a shrinking pass set across receipts.

**`input_tree_hash` is what makes a check evidence.** A check that ran on a
different tree than the subject is evidence about that other tree. §6.1 governs
when it may still count.

### 5.4 `verdicts[]`

| field | type | required | meaning |
|---|---|---|---|
| `claim_id` | string | yes | the claim this decides |
| `verdict` | string | yes | `supported` \| `unsupported` \| `unverifiable` |
| `verifier` | string | yes | `<name>@<version>`, e.g. `tests_pass@1` |
| `evidence` | string[] | yes | check and event ids this rests on |
| `reasons` | string[] | yes | why, in plain sentences |
| `warnings` | string[] | optional | things a reader should know |
| `findings` | object[] | optional | typed detector output; see §5.5 |

The three verdicts, and the rule that holds the whole design up:

- **`supported`** — the evidence in this receipt meets the verifier's criteria
  on the subject tree.
- **`unsupported`** — the evidence contradicts the claim, or the claim required
  evidence that is absent where it could reasonably have been produced.
- **`unverifiable`** — not enough was observed to decide.

**`unverifiable` MUST NOT be collapsed into `supported`,** at any layer: not by
a verifier, not by a receipt consumer, not by a policy engine, not by a CI
check. A claim nobody could check is not a claim that passed. This is the single
rule most likely to be violated by an implementation that is otherwise correct,
because every convenient default — a missing field, an empty list, an `all()`
over an empty set — points the wrong way.

A consumer combining verdicts MUST move away from `supported`, never toward it.
The safe ordering is `supported < unverifiable < unsupported`, combined with
`max`.

`verifier` MUST carry a version, written `<name>@<version>`. A stored verdict
has to say which code produced it, or a detector improvement silently rewrites
history.

Neither half is otherwise constrained: the name may carry any case, and the
version may be `1`, `1.0` or `2026-09-05`.

**The published schema does not currently enforce the version**, and this
document is the stricter of the two. `verdictEntry.verifier` is a non-empty
string there, so `"verifier": "tests_pass"` — a stored verdict that no longer
says which code produced it — is schema-valid. The gap is filed as IR-1 in
`docs/tracks/D/INTERFACE-REQUESTS.md` with the diff that would close it; a producer should
write the version regardless, because a consumer that resolves verdicts across
detector versions has nothing else to key on.

### 5.5 `findings[]`

A finding is one named observation, attached to the verdict it bears on.

```json
{ "severity": "fail" | "warn" | "info",
  "detector": "narrowed_run",
  "path": "tests/test_rate.py",
  "detail": "`pytest -q -k test_rate_small` narrows the run with `-k`, so it exercised a subset of the suite" }
```

`severity` and `detector` and `detail` are REQUIRED; `path` is optional.

`detector` is an open vocabulary. A consumer MUST NOT assume that an unknown
detector is benign, and MUST NOT assume that the **absence** of a detector means
the condition is absent — only that this producer did not report it. The
reference implementation makes that distinction explicit by emitting positive
"could not look" findings such as `narrowing_unchecked`; a producer SHOULD do
the same. The full reference vocabulary is in
[the verifier reference](../book/src/verifiers.md).

Severity is advisory to the consumer. It is the **verdict** that a gate acts on.

### 5.6 `promises`

```json
{ "open": ["I'll add a regression test for the large-weight case in a follow-up."],
  "fulfilled": [] }
```

Forward-looking statements the agent made. Both arrays are REQUIRED and MAY be
empty. An open promise is a report of something said and not yet done; it is
**not** evidence that it was abandoned, and a consumer SHOULD NOT present it as
one.

### 5.7 `capture`

| field | type | required | meaning |
|---|---|---|---|
| `sources` | string[] | yes | how the record was captured, e.g. `claude-code-hook`, `exec-recorder` |
| `transcript_hash` | string | optional | `sha256:` of the harness transcript, if one was read |
| `redactions_applied` | integer ≥ 0 | yes | how many secret-shaped values were masked before hashing |
| `capture_degraded` | boolean | yes | whether capture was incomplete or untrustworthy |
| `degraded_causes` | string[] | **required when `capture_degraded` is true**, forbidden when false | why, one entry per reason |
| `content_filters` | string | conditional | present exactly when clean filters were in scope (§4.1, §6.3) |
| `git_env_digest` | string | optional | `sha256:` over the git environment that produced the tree hashes |

**Redaction runs before hashing.** A producer MUST mask secret-shaped values
before any digest is computed, so that digests are stable and are never digests
over a credential.

`capture_degraded` is the honest-failure switch for the whole receipt. Set it
when hooks were missing, a shim was bypassed, the record was imported from a
harness's own log rather than observed, or a content filter was in scope. A
consumer MUST treat every claim in a degraded receipt as at most `unverifiable`.

**Degraded capture MUST say why.** `degraded_causes` is required whenever
`capture_degraded` is true, non-empty, and MUST be absent when it is false —
a stated cause with the flag down is a record disagreeing with itself. Each
entry is a machine-readable token, a colon, and a sentence:

```json
"degraded_causes": [
  "hook_events_missing: the Stop hook did not fire for this session",
  "content_filters: filter=lfs is in scope for *.bin"
]
```

The token is what a consumer matches on; the sentence is what a person reads.
**The token set is open by design**, and a consumer that meets one it does not
recognise MUST report it rather than drop it — a cause you cannot parse is still
a reason the capture was incomplete, and silently discarding it turns an
incomplete record into a clean-looking one.

A bare `capture_degraded: true` leaves a reader nothing to search for. That was
the defect this closes.

### 5.8 `verifier`

| field | type | required | meaning |
|---|---|---|---|
| `thruline_version` | string | yes | the producer's version string |
| `mode` | string | yes | `local` or `clean-sandbox-reexec` |
| `policy_hash` | string | yes, and see below | `sha256:` of the policy this receipt was judged under |

The field is named `thruline_version` for the reference implementation's sake; a
third-party producer SHOULD put its own name and version there, as a single
string, so a reader can tell who computed the verdicts.

**`policy_hash` has three states and they are three different things.**

- **stated** — `sha256:…`. The digest of the policy in force. "No policy file"
  is itself a stated digest, over the well-known empty-policy document; it is
  not an absence.
- **absent** — the field is omitted. A verifier MUST fail the receipt. Every
  session is judged under *some* policy, defaults included, so silence is a
  missing assertion rather than an inapplicable axis.
- **empty** — `""`. A verifier MUST fail the receipt, under a reason distinct
  from *absent*, so the two can never be read as one thing.

That distinction is not pedantry. In the reference implementation these were one
state, and a policy rewritten to its literal opposite produced byte-identical
`Verified.` output at exit 0.

#### Computing the digest

A verifier compares `policy_hash` against the policy it resolves locally, so a
producer that computes it differently fails against every verifier. The
construction is therefore part of the wire contract, and it is given here in
full — this paragraph exists because an earlier draft said "hash your
empty-policy document" without saying which document, and the first
independent implementation written from that draft produced a digest no
verifier would ever match.

Domain tag, which separates a policy digest from every other SHA-256 in the
system and separates the two states below from each other:

```text
thruline.policy.v1
```

**No policy file** — a well-known constant:

```text
sha256( "thruline.policy.v1\nabsent\n" )
  = sha256:cfbdf195a01ee22251ddcbc3c0bd23c88bff89a265080934cb00099d329d62ad
```

**A policy file** — the digest is over the *parsed* policy rendered
canonically, not over the file's bytes. Hashing bytes would move the digest on
a whitespace edit, and would happily produce a digest for a file that does not
parse — that is, for rules nothing could evaluate.

```text
sha256( "thruline.policy.v1\npresent\n" + canonical )
```

where `canonical` writes every field on its own line, in this fixed order,
whether or not the source file mentioned it — so a rule left out of the file
and a rule written with its default value produce the same text, which is
correct, because they are the same rules:

```text
version: <n>
require_receipt: <true|false>
claims:
  <kind>:                                   ← ascending by kind
    required_when_paths: [<glob>,<glob>]    ← sorted; or the word `always`
require_reexecuted: <true|false>
require_evidence: <true|false>
require_human_ack: <true|false>
min_changed_line_coverage: <n|none>
allow_skips: <true|false>
promises:
  block_on_open: <true|false>
  comment_open: <true|false>
stop_gate:
  enabled: <true|false>
  max_stop_blocks: <n>
reexec:
  mode: <claimed|full>
  reruns: <n>
  image: <string>
```

Five of the six `claims` rule lines — `require_reexecuted` down to
`allow_skips` — are written at column 0, not indented under the kind; only
`required_when_paths` is indented, by four spaces, exactly as the block above
shows. That asymmetry is the reference implementation's rendering and it is what
the digest is over; a producer that regularizes the indentation for readability,
in either direction, produces a different digest and fails verification.

**A policy file that exists and does not parse is an error, not an absence.**
Producing `absent` for a broken policy hides a policy the repository intended to
enforce. Refuse to emit a receipt instead.

`absent` and a file that happens to parse to the defaults are deliberately
different digests. The rules coincide, but deleting the policy file is exactly
the mutation this check exists to catch.

### 5.9 Extension fields

A producer MAY add fields. A verifier MUST ignore fields it does not know and
MUST NOT fail on them.

A verifier MUST NOT let an unknown field weaken a check. Concretely: an unknown
field can never supply a missing required field, satisfy an invariant, or turn a
`Skipped` outcome into a pass. Extensions carry information, never permission.

---

## 6. Invariants a verifier enforces

These are the checks that distinguish a receipt from a well-formed lie. Each is
a self-consistency rule: the receipt is refused because its own halves disagree,
which needs no outside knowledge at all.

### 6.1 Tree binding

A check counts as evidence for the subject when **either**:

1. `check.input_tree_hash` equals the subject's `gitTree`; **or**
2. nothing relevant changed between the check and the subject tree — no change
   to a non-test, non-ignored path.

A producer MUST record which rule applied, because rule 2 is a relaxation and a
consumer who cannot tell them apart cannot audit the verdict. The reference
implementation carries this as a `binding` field on its internal verdict record;
at the predicate layer it belongs in `verdicts[].reasons` as an explicit
sentence (`"ran on the final tree"` versus `"no relevant change since"`).

A check that ran **before** the last source edit is not evidence for the subject
tree. The correct verdict is `unsupported`, not `supported` with a note.

### 6.2 `reexecuted` must be derivable, never asserted

- `reexecuted: true` MUST be accompanied by `container_image_digest` matching
  `^sha256:[0-9a-f]{32,}$`. A bare boolean is a claim; the image digest is what
  makes it evidence, because it comes from an exec event inside the chain the
  receipt names.
- `reexecuted: false` MUST NOT carry a `container_image_digest`. A check whose
  two halves disagree about whether re-execution happened is refused.

A verifier MUST fail on either violation.

The reference verifier is weaker than that first rule, and the gap is written
down rather than discovered: `crates/attest/src/verify.rs` tests only the
`sha256:` prefix and a 32-character minimum, never the hex character class —
no hex validation exists anywhere in `crates/attest` — so `sha256:` followed by
32 non-hex characters passes the step. The published schema does apply the
pattern; the recomputation does not.

### 6.3 Capture fidelity

- `content_filters` present with `capture_degraded: false` MUST fail: the
  receipt disagrees with itself about whether the tree hash names the bytes on
  disk.
- `capture_degraded: true` with no `degraded_causes` MUST fail, and
  `degraded_causes` present with `capture_degraded: false` MUST fail. Both are
  the same defect from opposite sides: a flag and its reason that do not agree.
- `capture_degraded: true` MUST NOT produce a *verified* conclusion. The
  reference implementation reports it as **not checked here** rather than as a
  failure, deliberately: degraded capture means "could not observe", and the
  most expensive mistake an evidence system makes is accusing honest work.

### 6.4 Verdict consistency

- Every `verdicts[].claim_id` MUST resolve to a `claims[].id`.
- Every id in `verdicts[].evidence` SHOULD resolve to a `checks[].id`; ids that
  do not are event references and MUST NOT be counted as check evidence.
- A receipt containing an `unverifiable` verdict MUST NOT be reported as
  verified. The receipt says in writing that a claim could not be decided;
  printing "everything checks out" over it is the collapse §5.4 forbids.
- An `unsupported` verdict does **not** make a receipt invalid. An honestly
  recorded refutation is a correct receipt. Whether it may merge is a policy
  question, not a verification one.
- **A receipt with no verdicts at all is not the same as one whose verdicts all
  passed,** and this specification does not yet require a verifier to say so.
  The rule above is written as "contains an `unverifiable` verdict", and an
  empty list satisfies it vacuously — the same shape §3 refuses for an empty
  `signatures` array. Measured 2026-09-05: the reference verifier prints
  *"Verified. Every check the receipt asserts was confirmed here."* at exit 0
  for a receipt carrying zero claims and zero verdicts.

  A conformant verifier SHOULD distinguish the two in its conclusion. This is
  marked SHOULD rather than MUST because the sentence is literally true — such
  a receipt asserts nothing about claims — and because tightening it changes
  the conclusion for every existing consumer. It will be MUST in the next
  version unless the argument against it survives.

  **The reference implementation now closes the common path rather than the
  rule.** Since 2026-09-05 a session that changed files and matched no claim
  phrase carries one synthetic `unverifiable` verdict instead of none, so the
  zero-verdict receipt no longer arises from ordinary use. A producer may still
  emit one, and this rule still governs it: closing a path is not closing a
  hole.

---

## 7. Verification

A verifier performs these steps in order and reports each independently. **There
are exactly three outcomes per step: `pass`, `fail`, and `skipped` — and
`skipped` is not a pass.**

| # | step | how it can be skipped |
|---|---|---|
| 1 | signature over PAE | no verifying key supplied |
| 2 | `payloadType` and Statement schema | never — a structural failure is a failure |
| 3 | every subject's `gitTree` (and `gitCommit`) resolves in the local repository | no local repository; **a shallow clone** |
| 4 | `event_chain_head` and `event_count` recompute from the local record | no local record — **but see §7.1** |
| 5 | §6.2 re-execution consistency | never |
| 6 | `policy_hash` matches the policy resolved here | no policy resolvable here |
| 7 | §6.3 capture fidelity | filters in scope on either side |
| 8 | no verdict is `unverifiable` | one or more verdicts are `unverifiable` — the receipt itself says those claims could not be decided, so the reference verifier reports this step **skipped**, never failed; nothing else failing, the conclusion is `partial` at exit 2; see §9.1 |

The conclusion is one value, and a verifier MUST state it rather than leave it
to be derived:

| conclusion | meaning | exit code |
|---|---|---|
| `verified` | every step passed | 0 |
| `failed` | at least one step failed | 1 |
| `partial` | nothing failed, and something could not be checked here | 2 |

**A consumer MUST NOT re-derive the conclusion from the per-step array.** The
obvious idiom — "no step has outcome `fail`" — reads a *partial* result as
verified. If the conclusion is carried in JSON, it SHOULD appear both as a
string field whose spellings cannot be confused (`verified` / `partial` /
`failed`) and as a real boolean that is true only for `verified`; the field name
`verified` holding the string `"partial"` is truthy in Python and JavaScript,
which is the same collapse re-entering through a field name.

### 7.1 Third-party producers and the chain-head step

Step 4 has three outcomes in the reference implementation, and the third is a
known interoperability limitation:

| the verifying machine | outcome |
|---|---|
| holds the session's record, and it recomputes to the stated head and count | **pass** |
| holds no record at all | **skipped** — conclusion `partial` |
| holds a record store that does not contain this session | **fail** — conclusion `failed` |

The third row is correct for its intended case: a receipt claiming to describe a
session that this machine's own store does not have is a receipt about a record
nobody here can produce. But it also means **a receipt from a producer that
keeps no ThruLine ledger fails on any machine that has one** — verified
2026-09-05 with an independent emitter, which passed signature, payload type,
statement schema, subject tree and subject commit, and failed here.

Until a verifier can tell "a foreign producer's record" from "a session missing
from mine", a third-party producer should expect `failed` rather than `partial`
on a machine with a ledger. Nothing in the *format* requires this; it is where
the reference verifier currently stands, and it is written down rather than
discovered.

### 7.2 Shallow clones

`actions/checkout` defaults to `fetch-depth: 1`. A shallow clone does not
contain the objects a receipt names, so step 3 is **skipped**, and the
conclusion is `partial` — not `failed`, and emphatically not `verified`. A CI
configuration that wants the subject checked must set `fetch-depth: 0`.

---

## 8. Complete example

A real receipt, produced by the reference implementation from a session in which
an agent edited a source file, weakened one test, deleted another's assertion,
ran a single narrowed check, and wrote "All tests pass." The verdict is
`unsupported`, and the findings say exactly why.

Base64 and signature bytes are elided with `…` for width; everything else is
verbatim. The envelope below, the decoded statement after it, and the committed
`example-receipt.json` / `example-statement.json` are all one run —
`validate.py` decodes the receipt and requires it to equal both, so the three
cannot drift apart again.

```json
{
  "payload": "eyJfdHlwZSI6Imh0dHBzOi8vaW4tdG90by5pby9TdGF0ZW1lbnQvdjEi…",
  "payloadType": "application/vnd.in-toto+json",
  "signatures": [
    { "sig": "y567iSf6qX3sQrcSDX2uNZV3…", "keyid": "sha256:b12dafb601c476bdc…" }
  ]
}
```

The decoded payload:

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    {
      "name": "git+/tmp/demo/repo",
      "digest": {
        "gitCommit": "c2dac1e2ed75b897d8bcfb99a86b0048bead7a45",
        "gitTree": "c324df66d87ae811b42f2f3e395988361be76959"
      }
    }
  ],
  "predicateType": "https://thrulinehq.com/attestation/execution-check/v0.1",
  "predicate": {
    "session": {
      "id": "d3m0d3m0-0000-4000-8000-000000000001",
      "harness": "claude-code",
      "started": "2026-09-06T08:23:44Z",
      "ended": "2026-09-06T08:23:45Z",
      "event_chain_head": "sha256:a2075644391b14ad3e3c83666a2f2c870bb5626b2ae959204dd6592c25390f94",
      "event_count": 16
    },
    "claims": [
      {
        "id": "clm_3957JNEM04KCP2NAC0B2E1MWFW",
        "kind": "tests_pass",
        "text": "All tests pass.",
        "asserted_by": {
          "harness": "claude-code"
        }
      }
    ],
    "checks": [
      {
        "id": "chk_3E6AFW7VHZ4JCXA94XGHPWJR6J",
        "category": "test",
        "runner": "pytest",
        "argv_hash": "sha256:568669b0700f7fd297cfa98990a699279a612c4d0f1923e0526316dbaba97a94",
        "env_hash": "sha256:c5bb81ba27d0e3841e0f8c0736196888a26a30be09da3ebf579c40a0e06ad9d3",
        "cwd_hash": "sha256:ce08311861404b607960f9b24a504741a407bce14ed28add5bc42fceba78d4e7",
        "input_tree_hash": "c324df66d87ae811b42f2f3e395988361be76959",
        "exit_code": 0,
        "parsed": {
          "duration_ms": 0,
          "failed": 0,
          "passed": 1,
          "skipped": 1,
          "test_ids": [],
          "tests_total": 2,
          "xfail": 0
        },
        "reexecuted": false
      }
    ],
    "verdicts": [
      {
        "claim_id": "clm_3957JNEM04KCP2NAC0B2E1MWFW",
        "verdict": "unsupported",
        "verifier": "tests_pass@1",
        "evidence": [
          "chk_3E6AFW7VHZ4JCXA94XGHPWJR6J"
        ],
        "reasons": [
          "ran on the final tree",
          "1/2 passed"
        ],
        "warnings": [
          "1 skipped and 0 xfailed of 2 \u2014 they were not run",
          "`test_rate_small`'s strongest assertion went from Equality to Containment"
        ],
        "findings": [
          {
            "detail": "`pytest -q -k test_rate_small` narrows the run with `-k`, so it exercised a subset of the suite",
            "detector": "narrowed_run",
            "severity": "fail"
          },
          {
            "detail": "1 skipped and 0 xfailed of 2 \u2014 they were not run",
            "detector": "skipped_tests",
            "severity": "warn"
          },
          {
            "detail": "changed-line coverage was not measured: no coverage artifact was found (looked for lcov.info, coverage.xml, cobertura.xml)",
            "detector": "coverage_not_measured",
            "severity": "info"
          },
          {
            "detail": "`test_rate_large` had 1 assertion(s) at the base tree and 0 at the final tree",
            "detector": "assertion_removed",
            "path": "test_shipping.py",
            "severity": "fail"
          },
          {
            "detail": "`test_rate_small`'s strongest assertion went from Equality to Containment",
            "detector": "assertion_weakened",
            "path": "test_shipping.py",
            "severity": "warn"
          }
        ]
      }
    ],
    "promises": {
      "open": [],
      "fulfilled": []
    },
    "verifier": {
      "thruline_version": "0.1.0",
      "mode": "local",
      "policy_hash": "sha256:cfbdf195a01ee22251ddcbc3c0bd23c88bff89a265080934cb00099d329d62ad"
    },
    "capture": {
      "sources": [
        "claude-code-hook",
        "exec-recorder"
      ],
      "redactions_applied": 0,
      "capture_degraded": false,
      "git_env_digest": "sha256:769c04406a41b5c73b12c5df017c18f75529ffbe9da22f05e9c38f849fc4b62c"
    }
  }
}
```

Read the verdict in that receipt against §5.4: the check exited 0, it ran on the
final tree, and the claim is still `unsupported` — because the run covered one
of two tests and the other one lost its assertion in the same change set. Exit
code zero is not the evidence. The relation between the claim and the run is.

---

## 9. Emitting a receipt without ThruLine

Nothing in this format requires ThruLine. A CI job, a harness plugin, or a
different agent framework can emit a conforming receipt. The minimum:

1. **Record the tree.** Stage everything into a temporary index and
   `git write-tree`. Keep the hash.
2. **Record each check.** Its argv (hash it), cwd (hash it), environment (hash
   it), exit code, and the tree hash it ran against. Parse the runner's output
   into `parsed`, or state that you could not.
3. **Record the claims.** Whatever your agent asserted, typed to §5.2's
   vocabulary, with the agent's own sentence in `text`.
4. **Compute verdicts deterministically** from 1–3. Do not ask a model. Use
   `unverifiable` freely; it is the honest answer far more often than it feels
   like it should be.
5. **Fill `capture`.** `sources` names how you observed. Set
   `capture_degraded: true` unless you actually observed the executions —
   reading an agent's own log is not observation.
6. **State a `policy_hash`,** computed exactly as §5.8 specifies. With no
   policy that is the well-known `absent` constant, which §5.8 gives in full.
   Never the empty string, and never a digest of your own devising — a verifier
   compares it against the policy it resolves locally, so a construction it does
   not share is a construction it will reject.
7. **Canonicalize** with RFC 8785, **sign** the PAE, and emit the envelope.

### 9.1 This has been done, and here is what it took

`emit_example.py` in this directory is a producer written from **this document
only**. It imports nothing of ThruLine's; it uses the standard library and one
Ed25519 library. Run it against a git repository and check the result with
ThruLine's own verifier:

```sh
python3 emit_example.py <a git repo> receipt.json
thruline verify-receipt receipt.json --key receipt.pub
```

Re-measured 2026-09-06 at the core's `fad0e00`, against a machine with no
ThruLine ledger. Three checks are newer than the first run —
`verdict_derivation`, `check_provenance` and `head_binding` — and the emitter
gained `capture.degraded_causes`, which the schema now requires whenever capture
is degraded:

```text
ok   signature          signature 0 verifies over the DSSE PAE
ok   payload_type       application/vnd.in-toto+json
ok   statement_schema   in-toto Statement v1, ThruLine predicate
ok   subject_tree       ded384cb… is a tree in this repository
ok   subject_commit     988e6b72… is present locally
skip chain_head         no ledger on this machine — NOT a pass
ok   reexecution        no check claims to have been re-executed
ok   policy_hash        the policy this receipt was judged under is the policy here
skip capture_fidelity   the receipt records DEGRADED capture: no_execution_capture: … — NOT a pass
skip verdicts           1 of 1 verdict(s) are `unverifiable` — NOT a pass
ok   verdict_derivation 0 `supported` verdict(s) re-derived from the checks cited beside them
skip check_provenance   no ledger here holds this session, so the check rows were
                        compared against nothing — NOT a pass
ok   head_binding       the head under review carries the tree this receipt is about

PARTIALLY verified: nothing contradicts the receipt, and 4 check(s) could not be
made on this machine. That is not the same as verified.
```

`partial` is the **correct** conclusion, not a shortfall. Four checks are
skipped now rather than three: `check_provenance` was added, and it is the one
worth reading — *"they are a COPY of the record, not the record, and a re-signed
receipt can carry a check that was never run."* A third-party receipt cannot
pass it on a machine that does not hold the session, and should not. This producer did not
observe the session, so it said so (`capture_degraded: true`) and returned
`unverifiable`, and the verifier declined to call that verified. A `verified`
here would be the collapse §5.4 forbids.

**Two defects in this document were found by writing that file, and both were
found by the verifier rejecting its output:**

1. §5.8 said "hash your empty-policy document" and never said which document.
   The emitter hashed the empty byte string and produced a digest no verifier
   will ever match. The full construction is now in §5.8.
2. §7's table said the chain-head step skips when there is "no local record",
   which is true only when there is no store at all. On a machine that *has* a
   ThruLine ledger it **fails**. §7.1 now says so.

Both are the kind of gap only an independent implementation finds, which is why
this section exists as a runnable file rather than as a promise.

The path runs end to end. The same third-party receipt, rendered as the GitHub
check-run payload the reference implementation posts:

```json
{
  "conclusion": "neutral",
  "name": "thruline/receipt",
  "output": { "title": "Receipt PARTIALLY verified — not the same as verified", … }
}
```

`neutral`, not `success` and not `failure` — a receipt from a producer that did
not observe the session, carried all the way to a pull-request check, without
losing the fact that nothing about it was actually verified.

### 9.2 The minimum honest receipt

A producer with no event chain, no re-execution, and no policy engine can still
emit something useful. Set `event_chain_head` to the digest of whatever record
you do keep and `event_count` to its length; set `capture_degraded` to `true`;
state §5.8's `absent` digest. Read §7.1 first: on a machine that has a ThruLine
ledger, the chain-head step will currently fail rather than skip. Every claim will come back `unverifiable`
against a strict verifier, which is the correct answer for a producer that did
not observe the executions — and it is a far more useful artifact than a green
check with nothing behind it.

Do not reach for `supported` to make the output look better. A receipt whose
verdicts are honest and mostly `unverifiable` is the thing this format is for.

---

## 10. Versioning

The version is in the predicate type URI. A change that could make an existing
verifier read an existing receipt differently is a **new URI**, not an edit to
this one.

Within `v0.1`, these are compatible changes: adding an optional field; adding a
claim kind; adding a detector name; adding a `capture.sources` value.

These require a new version: removing or renaming a field; changing a field's
type; changing an invariant in §6; changing the meaning of a verdict.

`v0` signals that the *shape* may still move before 1.0. It does not signal that
the semantics are casual — §5.4 and §6 are the point of the format and will not
be relaxed.

---

## 11. Security considerations

- **The signature covers the statement, not the truth of it.** A valid signature
  means a key-holder said this. Every invariant in §6 exists because a producer
  can be wrong or dishonest, and each one is checkable without trusting them.
- **Receipt replay.** The subject digest binds a receipt to a tree. A verifier
  MUST recompute the subject against the repository in front of it rather than
  reading the name field. A receipt for another commit is a receipt about
  another commit.
- **Digests over secrets.** Redaction before hashing (§5.7) means a digest is
  never a digest over a credential — which also means it is never an oracle for
  one.
- **`argv_hash` hides the command.** A consumer cannot audit narrowing from the
  receipt alone (§5.3). Treat the absence of a narrowing finding as the absence
  of a *report*, not the absence of narrowing.
- **A receipt is publishable, not public.** It carries no file contents and no
  transcript text, but it does carry claim sentences the agent wrote, promise
  text, file paths in findings, and a repository path in `subject.name`. Read
  one before publishing it.

---

## 12. References

- in-toto Attestation Framework — <https://github.com/in-toto/attestation>
- DSSE — <https://github.com/secure-systems-lab/dsse>
- RFC 8785, JSON Canonicalization Scheme — <https://www.rfc-editor.org/rfc/rfc8785>
- RFC 3339, date and time on the Internet
- SLSA Provenance, the predicate this one is deliberately not — <https://slsa.dev/provenance>
- TRACE Trust Record mapping — [`trace-trust-record-mapping.md`](./trace-trust-record-mapping.md)
