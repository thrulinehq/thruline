# The ThruLine receipt Action

BUILD-SPEC §9's zero-install path: verify a ThruLine receipt in CI, post a
**check run** and a PR comment table.

```yaml
- uses: actions/checkout@v4
  with:
    # REQUIRED for a green check run. The default is 1, and a shallow clone
    # does not have the objects a receipt names, so the subject checks come
    # back SKIPPED and the conclusion is neutral.
    fetch-depth: 0

# The Action lives in this repository, not in one of its own. GitHub resolves
# `owner/repo/path@ref`, so a consumer references the subdirectory directly.
# This said `thruline/action@v1`, which is not a repository that exists — Track
# Q reproduces it as `git ls-remote` returning `Repository not found` (D02).
#
# The repository is public (FSL-1.1-MIT), so `@main` resolves for any workflow.
# There is no `@v1` tag to point at yet, and inventing one would be a URL that
# 404s the first time anyone copies this block — which is the defect being
# fixed, not a smaller version of it.
- uses: thrulinehq/thruline/packaging/action@main
  with:
    receipt: .thruline/receipts/latest.json   # optional; newest is the default
    key: .thruline/keys/signing.pub           # optional
```

Without `fetch-depth: 0` the action **fails the job** with a message naming
that setting. That failure is about the CI configuration, not about your
receipt, and it happens before anything is verified — set
`require-full-clone: false` to accept the weaker neutral conclusion instead.

## What the conclusion means

The conclusion is combined from **three axes**, and combined *downward only* —
no axis can lift another (operator ruling, 2026-09-04):

| axis | asks | decided by |
|---|---|---|
| integrity | does the receipt reproduce against this repository? | `thruline verify-receipt` |
| claims | did every claim inside it get proved? | the receipt's own verdicts |
| policy | may this merge anyway? | `.thruline/policy.yml`, when `policy:` is set |

| conclusion | what happened |
|---|---|
| `success` | every check the receipt asserts was confirmed here, **and** no claim in it is `unsupported` or `unverifiable`, **and** the policy (if any) passed |
| `neutral` | nothing contradicts the receipt, and something is unproved: a check could not be made here, or a claim the agent made is not supported by the record — **not the same as verified** |
| `failure` | a check contradicted the receipt, **or** `.thruline/policy.yml` refused the change |

`neutral` fails the job only with `fail-on: neutral`.

**The check run can never conclude `success` while any claim is `unsupported`.**
This was measured doing exactly that at `10dae44`: the receipt was genuine, so
the integrity axis was green, and the check run said `success` with a green
badge while only the opt-in policy step went red. Two verdicts on one pull
request, and the louder one was wrong. Without a policy file an unsupported
claim is now `neutral` **with the verdict table rendered**, so the reviewer can
see which claim and why; `failure` comes only from policy, because deciding
that an unproved claim may not merge is the repository's call, not this
action's.

## The one rule

The action **never derives a verdict**. `thruline verify-receipt --format
check-run` emits the conclusion the verifier computed, and the action posts
that value verbatim.

That rule is not stylistic. A `--json` consumer using the standard idiom
`all(c.outcome != "fail")` was measured reading **exit 2 as success**, because
a partially verified receipt contains no failing check. A check run reporting
"verified" for a partially verified receipt is §6's `unverifiable` →
`supported` collapse on the most public surface the product has.

`crates/cli/tests/m2_action.rs` walks `action.yml` and fails if it ever reads
`.checks`, `.outcome` or the exit code to decide an outcome.

The same rule governs the policy: `Evaluate the policy` runs **first** and hands
its word to the receipt step in `THRULINE_POLICY_CONCLUSION`, so
`thruline policy --conclusion` stays the single producer of a policy verdict and
the check run cannot disagree with it. An unrecognised word there is a hard
error, never a shrug — a gate that reads a word it does not understand as
"fine" passes exactly when it has broken.

### `verify --json`

`thruline verify-receipt --format json` states the answer rather than leaving it
to be counted:

```json
{
  "conclusion": "neutral",
  "verified": false,
  "status": "partial",
  "exit": 0,
  "integrity": { "status": "verified", "exit": 0 },
  "claims": { "total": 1, "supported": 0, "unsupported": 1,
              "unverifiable": 0, "other": 0, "all_supported": false,
              "unsupported_claim_ids": ["clm_…"] },
  "policy": { "evaluated": false, "conclusion": null },
  "checks": [ … ]
}
```

Gate on `conclusion`. **Not on `exit`** — ruling point 1 keeps `verify`
integrity-only and non-blocking, so an unsupported claim does not move the
process's exit status. The four `claims` buckets always sum to `total` (a
verdict word this build does not recognise lands in `other` and is named), and
`all_supported` is a stated boolean so nothing has to be added up.

## The badge

The action renders it: `outputs.badge` is the path to a self-contained
`verified-by-thruline` SVG with no external font, image or script. It is a
fourth rendering of the **same** conclusion — `thruline verify-receipt --format
badge` — never a word this action chose.

| receipt | badge | colour | job |
|---|---|---|---|
| verified here, every claim proved | `verified` | green `#2ea043` | passes |
| nothing contradicted, something unchecked or unproved | `partial` | amber `#bf8700` | passes (fails with `fail-on: neutral`) |
| a check contradicted it, or policy refused it | `not verified` | red `#cf222e` | fails |
| no receipt in the repository | **no badge at all** | — | fails |

Every row above was observed, not asserted: `packaging/action/selftest.py`
produces each one from a real signed receipt and prints what it saw.

Two rules the badge step enforces, both of which can only ever turn a green
into an error:

- Any badge already in the workspace is **deleted before verification starts**,
  so a stale green SVG cannot be published as this run's answer.
- If the SVG says "verified" while the conclusion is not `success`, the step
  **refuses** and publishes neither.

The **hosted** badge endpoint belongs to §9's GitHub App, which is M6. What
exists today is the route function (`thruline_attest::report::badge_route`),
tested, with no server mounting it yet.

## Proving it

`packaging/action/selftest.py` runs the step bodies out of `action.yml` — the
same shell, the same order, the same inputs — over receipts it really signs,
and fails unless it observes: verified, an `unverifiable` verdict held at
neutral, partial, not verified, no-receipt, the shallow-clone refusal, the
badge/conclusion disagreement guard **firing**, an `unsupported` claim held at
neutral with its table **with no policy file present**, and §9's policy gate
turning that same session red.

`cargo test -p thruline-cli --test m2_action` runs it too, on any machine with
python3+PyYAML and a `pytest` on PATH, and prints a loud skip naming what was
not checked otherwise. Until that was added the only runner was
`.github/workflows/receipt.yml`, **which has never executed on GitHub** — and
the harness was found red at `10dae44`, still expecting `success` over a receipt
whose only verdict was `unverifiable`, an expectation the Rust suite had
abandoned three commits earlier. A guard nobody runs goes stale silently.

```
cargo build -p thruline-cli --no-default-features
python3 packaging/action/selftest.py --thruline target/debug/thruline
```

`.github/workflows/receipt.yml` runs it on ubuntu **and macOS**, then runs this
action for real against a receipt captured in the job. Running it found four
defects that reading it had not, one of which — `"${key_arg[@]}"` on an empty
array under bash 3.2 — only appears on macOS runners.

**What still needs GitHub:** the `POST /repos/{owner}/{repo}/check-runs` call
and `gh pr comment`. The harness stubs `gh` and asserts the payload it would
send; that GitHub accepts the payload is unverified until the workflow has run
there once.

## The policy gate, and the trap in the shipped example

`policy: .thruline/policy.yml` turns on §9's gate, which is the **only** thing
here that can *fail* a job for an unsupported *claim*. Receipt verification
answers "is this receipt about this repository and unaltered" — a receipt can be
entirely genuine and contain `tests_pass: unsupported`. It no longer reads as
green when it does: without a policy the conclusion is `neutral`, with the
verdict table naming the claim.

**Known: a pull request that edits `.thruline/policy.yml` reddens any receipt
signed before the edit.** §8 binds the policy digest into the receipt, so the
`policy_hash` check reports a mismatch and the conclusion is `failure` — the
message says "contradicted", and nothing was. Rebuild the receipt after changing
the policy. Pinned as finding F1 in `crates/attest/tests/false_accusation.rs`,
open for an operator ruling because relaxing it would move a verdict up.

The example `thruline policy --init` writes (BUILD-SPEC §9's own) requires
`tests_pass` only `required_when: { paths: ["src/**", "lib/**"] }`. A repository
with a flat layout matches neither, so the rule never fires and the gate passes
a session it should have failed. Observed on `thruline demo`'s cheating session:
with the example policy the gate says ✅; with `paths: ["**"]` it says
`tests_pass · verdict · the claim is unsupported by the execution record` and
fails the job. Write the paths your repository actually has.
