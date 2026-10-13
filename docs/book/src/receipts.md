# Receipts

A receipt is a signed, portable statement of what a session's checks actually
did. It is an [in-toto](https://in-toto.io) Statement wrapped in a
[DSSE](https://github.com/secure-systems-lab/dsse) envelope.

```sh
thruline receipt --out .thruline/receipts/latest.json
thruline verify-receipt .thruline/receipts/latest.json --offline
```

`receipt` builds from **ledger rows only**, so it is reproducible from the
record alone, and signs with a local key — generated on first use at
`$THRULINE_HOME/signing.key`, or supplied with `--sign-with`. Sigstore keyless
signing is not built.

*What would prove it:* an envelope signed through Fulcio, logged in Rekor, and
verified here from the Sigstore root — none of which this build does.

The verb is `verify-receipt`. `thruline verify` is a different command: it
re-reads the raw files and recomputes the chain.

## What it binds together

- the **subject**: the git tree the checks ran against;
- each **check**: its argv digest, exit code, and the tree it ran on;
- the **verdicts** those checks produced, with their findings;
- the **policy** it was judged under, and how capture was obtained.

The binding is the point. Existing attestation stacks say "this artifact was
produced by this builder"; a receipt says "this *result* came from this
*computation*, over this exact tree".

## Verification exit codes

| code | integrity | meaning |
|---|---|---|
| `0` | `verified` | every integrity check passed |
| `1` | `failed` | a check **contradicted** the receipt |
| `2` | `partial` | nothing failed and something could not be checked here |

**The exit code is the integrity axis alone.** A receipt carrying an
`unsupported` verdict — a claim the record does not support — still exits `0`,
while its JSON reads `"conclusion": "neutral"`, `"status": "partial"`,
`"verified": false`. Gate on `conclusion`, never on `$?`. An `unverifiable`
verdict is the one that moves this column, to `partial` and exit `2`.

Two is not one and it is not zero. A receipt verified as far as this machine can
verify it is not a receipt verified — most often because the clone is shallow
and the objects the receipt names are not present. `git fetch --unshallow`, or
`fetch-depth: 0` in CI.

Every `--json` shape this binary emits carries two fields before anything else:

```json
{ "schema_version": 1, "shape": "thruline.verify-receipt", "conclusion": "…", … }
```

`shape` names the contract and `schema_version` versions it, so a consumer can
refuse a shape it does not understand instead of reading unfamiliar fields as
absent ones. `thruline json-shapes` prints the whole registry. Two shapes are
exempt and carry no envelope of their own: `receipt --json`, which emits a DSSE
envelope whose format is not ours to version, and
`verify-receipt --format check-run`, which emits GitHub's.

**Do not re-derive the conclusion from the checks array.** The obvious idiom —
"no check has outcome `fail`" — reads a *partial* receipt as verified. The JSON
carries `status` (`verified` / `partial` / `failed`) and a real boolean
`verified` that is true only for the first, precisely so that a truthiness test
cannot go wrong; both are written from one function so they cannot disagree.

## Rendering it

```sh
thruline verify-receipt <path> --format md          # a PR comment table
thruline verify-receipt <path> --format check-run   # a GitHub check-run payload
thruline verify-receipt <path> --format badge       # the SVG
thruline verify-receipt <path> --format json
```

Every rendering carries the conclusion the verifier computed, rather than
letting each consumer reach its own.

## Canonicalisation

[The receipt specification](./receipt-spec.md) §3.3 tells a producer to
serialise the Statement payload with
[RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) JSON Canonicalization Scheme
before signing. **This build does not.** `thruline receipt` serialises the
Statement with plain `serde_json` — keys in struct-declaration order, not sorted
— and signs those exact bytes. A verifier checks the signature over the bytes as
received, so a receipt written this way still verifies; what is lost is
comparability, because two receipts that mean the same thing but differ in key
order do not hash the same way. Hashes are SHA-256, lowercase hex. Identifiers
are ULIDs.

*What would prove it:* the receipt payload routed through an RFC 8785
canonicaliser before signing — the implementation at
`crates/core/src/spec/canonical.rs` is wired to ledger events, not to the
receipt path.

## Trailers

```sh
thruline receipt --trailer
```

prints a `ThruLine-Receipt:` line for a commit message, so the receipt travels
with the commit rather than beside it. Nothing rewrites a commit you did not ask
to amend.

## The format itself

[The receipt specification](./receipt-spec.md) is written for someone who wants
to emit or check one **without ThruLine** — every field, every invariant a
verifier enforces, the PAE test vector, and a worked example.
