# execution-check/v0.1 ↔ TRACE Trust Record

**What TRACE is checked against.** Every TRACE field name, enum and requirement
below was read from `agentrust-io/trace-spec` on **2026-09-05**:
`schema/trace-claim.json` and the repository's quick-start example, at
`eat_profile` `tag:agentrust-io.com,2026:trace-v0.2`. TRACE v0.2 is a
**Developer Preview**, and the Linux Foundation announced the contribution on
25 August 2026 — TRACE is hosted as its own series, "TRACE Specification, a
Series of LF Projects, LLC", its `GOVERNANCE.md` says the technical home for
ongoing specification work is not yet settled, and CoSAI WS4 is named only as
an interoperability target — so the schema will move. Nothing here is
transcribed from memory; anything this document could not read is marked
**unread** rather than filled in.

---

## 1. Why both, and why neither subsumes the other

TRACE answers **"was this a trustworthy execution environment?"** — which
hardware, which model weights, which policy bundle, which data class, appraised
by which verifier. It is hardware-backed and it composes RATS, EAT, SLSA, SCITT
and SPIFFE.

execution-check answers **"did the result the agent claimed actually come from a
computation, over this exact source?"** — which claims, which commands, which
tree, and the deterministic verdict relating them.

The distinction is sharp and worth stating plainly:

> A TRACE record can be **perfect** while an agent deletes a test's assertion,
> runs one narrowed check, and writes "all tests pass." Nothing in TRACE is
> false in that scenario. The enclave really did attest, the weights digest
> really does match, the policy really was enforced. TRACE describes the
> *environment* of the computation; it does not evaluate the *claim* the agent
> made about the result.

execution-check is the converse: it says nothing about hardware, and it returns
`unsupported` for that claim, with findings naming the deleted assertion and
the narrowed run. It does not say the tests failed — the narrowed run exited
zero. It says the claim was made and the record holds no proof of it.

Neither is a superset. A relying party that wants both asks for both, and §4
below is how one record points at the other.

---

## 2. Field mapping

TRACE fields are as read from `schema/trace-claim.json` at v0.2. "—" means the
concept does not exist on that side, which is the interesting part of the table.

| TRACE field | required in TRACE | execution-check counterpart | how they relate |
|---|---|---|---|
| `eat_profile` | yes | `predicateType` | Both identify the document's own contract. Both are versioned so a breaking change is a new identifier. |
| `iat` | yes | `session.ended` | TRACE takes Unix epoch seconds; execution-check takes RFC 3339 UTC. Convert; do not carry both spellings of one instant in one record. |
| `subject` | yes | — | TRACE's subject is the **agent's workload identity** (SPIFFE SVID or DID). execution-check's subject is a **git tree**. These are different nouns and MUST NOT be conflated: one names who ran, the other names what was examined. |
| `model` | yes | `session.model`, `claims[].asserted_by.model` | execution-check carries a bare model string and no `weights_digest`. Where both records exist, TRACE's `model` is authoritative; execution-check's field is a label. |
| `runtime` | yes | — | No counterpart, deliberately. execution-check makes **no hardware or runtime attestation claim**. Where a check was re-executed, `checks[].container_image_digest` names the image — that is a software identity, not a platform measurement, and it MUST NOT be mapped into `runtime.measurement`. |
| `policy` | yes | `verifier.policy_hash` | The closest true pairing. `policy.bundle_hash` ← `verifier.policy_hash`. `policy.enforcement_mode` follows how the receipt was used: **`enforce`** when a gate blocked on it, **`advisory`** when it only annotated, **`declared`** when a receipt was produced and no gate consumed it. See §3.2 — this is the mapping most likely to be got wrong. |
| `data_class` | yes | — | No counterpart. execution-check carries no data classification. Producers set it from their own governance, not from the receipt. |
| `build_provenance` | yes | — | SLSA provenance about a build artifact. execution-check is not about a build artifact, and `slsa_level` MUST NOT be inferred from anything in a receipt. Set it from real SLSA provenance or set `0`. |
| `appraisal` | yes | roll-up of `verdicts[].verdict` | §3.1. The single most dangerous cell in this table. |
| `cnf` | yes | the DSSE `signatures[].keyid` | Both bind the record to a key. TRACE wants a JWK confirmation claim; a receipt carries `sha256:` over raw public key bytes. A producer emitting both MUST publish the JWK — the digest is not convertible into one. |
| `tool_transcript` | no | `session.event_chain_head`, `session.event_count` | §3.3. The natural join, and it needs one field TRACE does not have. |
| `transparency` | no | — | SCITT receipt URI. execution-check has no transparency-log field; Rekor is named as compatible and is not implemented. |
| `delegation` | no | — | Sub-agent lineage. execution-check records sub-agent boundaries in its event log, not in the predicate. |
| `origin` | no | `capture.sources`, `capture.capture_degraded` | §3.4. The one place TRACE's vocabulary is *better* than the receipt's. |
| `references` | no | **the join** | §4. |
| `signature` | no | the DSSE envelope | TRACE puts a base64url signature in the record; execution-check wraps the statement in DSSE and signs the PAE. Not interchangeable — different signed bytes. |
| — | | `claims[]` | **No TRACE counterpart.** TRACE has no notion of a statement the agent made about its own work. |
| — | | `checks[]` | **No TRACE counterpart.** `tool_transcript.hash` commits to a transcript; it does not carry per-command exit codes, tree bindings or parsed results. |
| — | | `verdicts[].findings[]` | **No TRACE counterpart.** Assertion removed, run narrowed, test skipped — TRACE has no field for evidence about the *tests*. |
| — | | `promises` | **No TRACE counterpart.** |

---

## 3. The four mappings that are easy to get wrong

### 3.1 `appraisal.status` — the collapse waiting to happen

TRACE's enum is `affirming` · `warning` · `contraindicated` · `none`.
execution-check's is `supported` · `unsupported` · `unverifiable`.

They are close enough to invite a three-into-four mapping that quietly loses the
one distinction the receipt exists to preserve. The correct mapping:

| execution-check roll-up | `appraisal.status` | why |
|---|---|---|
| every verdict `supported` | `affirming` | the record backs every claim |
| any verdict `unsupported` | `contraindicated` | the record contradicts a claim |
| any verdict `unverifiable`, none `unsupported` | **`none`** | **not** `affirming` — see below |
| no verdicts at all | `none` | nothing was appraised |
| `capture.capture_degraded: true` | `none`, whatever the verdicts say | the producer did not observe enough to be trusted about the rest |

**`unverifiable` MUST NOT map to `affirming`,** and it must not map to
`warning` either. `warning` reads as *appraised, with reservations*;
`unverifiable` means *not appraised*. TRACE's own `none` is the honest cell,
and it is the one a hurried mapping skips because it looks like an omission.

Roll up by moving **away** from `affirming`, never toward it. One
`contraindicated` claim beats a hundred `affirming` ones.

### 3.2 `policy.enforcement_mode` — describes use, not existence

`verifier.policy_hash` is always present in a conforming receipt, including the
well-known digest of the empty policy. Its presence therefore says **nothing**
about whether anything was enforced, and a producer that maps
"policy_hash present → `enforce`" is asserting an enforcement that may never
have happened.

Take the mode from what the gate actually did:

- `enforce` — a gate evaluated this receipt and could have blocked the merge.
- `advisory` — a gate evaluated it and could not block.
- `declared` — a receipt exists and no gate consumed it.
- `silent` — not reachable from a receipt; do not emit it from this mapping.

### 3.3 `tool_transcript` — the join needs a field TRACE does not have

`tool_transcript.hash` is the natural home for `session.event_chain_head`, and
`tool_transcript.call_count` for a count of tool invocations.

Two cautions:

**`call_count` is not `event_count`.** execution-check's `event_count` counts
*every* event in the chain — prompts, snapshots, file changes, compaction
boundaries, the stop. Tool calls are a subset. Writing `event_count` into
`call_count` overstates the tool count, often by a large factor. Count tool
events, or omit the field.

**TRACE has nowhere to put `event_count`.** And in execution-check the count is
part of the session's *identity*, not a statistic: a prefix of a hash chain is
itself a valid hash chain, so a truncated session verifies as a chain of its own.
A TRACE record that carries only `tool_transcript.hash` therefore **cannot detect
a truncated transcript**. Carry the count in `references[].digest`'s neighbourhood
(§4) or accept the gap knowingly. Do not pretend the hash alone closes it.

### 3.4 `origin.kind` — where TRACE is more precise than the receipt

TRACE's `origin.kind` enum is `self` · `third-party-control-plane` ·
`log-import`, and it draws exactly the line execution-check draws with
`capture_degraded` — but with more resolution:

| how the record was obtained | `origin.kind` | `capture.capture_degraded` |
|---|---|---|
| the producer observed the executions itself | `self` | `false` |
| a control plane reported them | `third-party-control-plane` | `true` |
| parsed from a harness's own log after the fact | `log-import` | `true` |

`log-import` is the case that matters: a harness's rollout file is the harness's
account of what it believes happened, not evidence that it happened. Both formats
mark it; TRACE names it. A producer emitting both SHOULD set `origin.kind`
precisely and let `capture_degraded` follow.

---

## 4. Joining the two records

Use TRACE's optional `references` array. Each entry takes `rel`, `id` and
`resolver`, with optional `retention` and `digest`.

```json
"references": [
  {
    "rel": "execution-check",
    "id": "https://thrulinehq.com/attestation/execution-check/v0.1",
    "resolver": "https://ci.example.org/receipts/{sha256}",
    "digest": "sha256:<of the DSSE envelope bytes>"
  }
]
```

`digest` over the **envelope** bytes, not the statement: the envelope is what
travels, and it is what a commit trailer names.

The reverse direction — pointing at a TRACE record from a receipt — has no
dedicated field in v0.1. Use an extension field under `predicate.capture`, and
note §5.9 of the specification: an extension carries information and never
permission. A verifier will ignore it, and it MUST NOT be able to satisfy any
check.

**Do not merge the two records.** A single document with a hardware measurement
and a verdict in it invites a reader to believe the hardware attested the
verdict. It did not: the verdict is arithmetic over a software record, and the
enclave has no opinion about whether the tests passed.

---

## 5. What `thruline export --format trace` should emit, and what it does today

**Today: it does not exist.** BUILD-SPEC §8.1 names
`thruline export --format trace`, and the shipped `thruline export` derives a
project into the §9 JSON contract — a different command with the same name. No
TRACE record has been emitted by this codebase, and no output of it has been
validated against `schema/trace-claim.json`.

That is stated here rather than in an issue because this document would
otherwise read as describing a working export. It describes a mapping.

When it is built, it must:

1. Emit every TRACE-required field: `eat_profile`, `iat`, `subject`, `model`,
   `runtime`, `policy`, `data_class`, `build_provenance`, `appraisal`, `cnf`.
2. **Refuse rather than default** on the four fields no receipt can supply —
   `subject`, `runtime`, `data_class`, `build_provenance`. A default
   `runtime.platform: "software-only"` is a truthful-looking value that means
   "no attestation", and emitting it silently from a command the user did not
   configure is exactly the class of quiet substitution this project exists to
   catch. Take them from configuration or refuse.
3. Validate its own output against the pinned `schema/trace-claim.json`, in a
   test, with the schema vendored at a recorded commit.
4. Pin the profile string in **one** constant, the way `PREDICATE_TYPE` is
   pinned — TRACE v0.2 is a Developer Preview and `eat_profile` will change.

---

## 6. Open questions for the TRACE maintainers

Recorded here so they can be taken to the TRACE maintainers rather than worked
around.

1. **Is there a place for a claim?** TRACE attests the environment of an agent's
   execution but has no field for what the agent *said it accomplished*.
   `references` is a workaround. A first-class `result_claim` relation would
   make the composition explicit instead of conventional.
2. **`tool_transcript` cannot express truncation.** A hash over a transcript
   commits to a transcript; it cannot distinguish a complete one from a prefix.
   A `call_count` is optional and, being a count of calls rather than of chain
   entries, does not close it either. Consider a required length alongside the
   hash for any transcript that is a chain.
3. **`appraisal.status` has no "not appraised for this reason".** `none` is
   correct for `unverifiable` and carries no reason, so a relying party cannot
   tell "no verifier ran" from "a verifier ran and could not decide". Those are
   very different for a merge decision.
4. **Which `provenance_depth` applies** when the appraised object is a source
   tree rather than a build artifact? `surface` / `builder` / `transitive` are
   build-shaped, and a source-tree appraisal fits none of them cleanly.

---

## 7. Verification status of this document

| claim | status |
|---|---|
| TRACE field names, requiredness and enums | **read** from `schema/trace-claim.json`, 2026-09-05 |
| TRACE quick-start example shape | **read** from the repository README, 2026-09-05 |
| `spec/trace-v0.2.md` prose semantics | **unread.** The mapping is derived from the schema and the example; where the prose spec constrains a field further, the prose wins and this table must be re-derived. |
| The mapping itself | **unimplemented and untested.** No TRACE record has been produced from a receipt and validated. §5 says so. |
| Whether the LF contribution changed the schema | **unread.** The contribution was announced in August 2026 and the technical home for ongoing work is not yet settled; the schema read here is the `agentrust-io` repository's. Re-check before relying on the profile string. |

Sources:
[TRACE specification repository](https://github.com/agentrust-io/trace-spec) ·
[Linux Foundation announcement](https://www.linuxfoundation.org/press/linux-foundation-welcomes-trace-to-advance-verifiable-runtime-evidence-for-ai-workloads)
