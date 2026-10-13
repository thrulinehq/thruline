# The gate in CI

## Producing a receipt

The receipt has to exist before CI can check it. Either commit one:

```sh
thruline receipt --out .thruline/receipts/latest.json
git add .thruline/receipts/latest.json
```

or run the agent under ThruLine inside CI and build the receipt there.

## The Action

**The Action does not exist yet, and it has no name.** The organisation named in
earlier drafts — `thruline/action` — is not a repository; `git ls-remote` answers
*Repository not found*. The public repository is a launch decision the operator
has reserved, and it is expected to be a fresh-history snapshot, so the Action's
eventual address is not something this page can honestly guess.

The block below is therefore the **shape** the workflow will take, with the
Action's address left as a placeholder you cannot paste into something that
half-works. What to run today is under
[Until it is published](#until-it-is-published).

```yaml
# NOT YET RUNNABLE. `<thruline-action>` has no address: see above.
- uses: actions/checkout@v4
  with:
    # REQUIRED. The default is 1, and a shallow clone does not have the
    # objects a receipt names, so the subject checks come back SKIPPED and
    # the conclusion is neutral.
    fetch-depth: 0

- uses: <thruline-action>@v1
  with:
    receipt: .thruline/receipts/latest.json   # optional; newest is the default
    key: .thruline/signing.pub                # optional; the verifying key
    policy: .thruline/policy.yml              # optional; off unless set
    comment: 'true'                           # PR comment with the claims table
    require-full-clone: 'true'                # fail early on a shallow checkout
    fail-on: failure                          # or `neutral` for strict repos
    version: v0.2.0                           # optional; installs a pinned build
    sha256: <the release archive's sha256>    # required whenever version is set
```

It posts a check run named `thruline/receipt` and, on pull requests, a comment
with the claims table.

**`policy` is off by default.** A repository that has not written a policy must
not acquire one from an action's default.

### Until it is published

The Action is unpublished for the same reason no release binary is, and
unnamed for the same reason the install URL is provisional. Run the same
commands in a `run:` step — `thruline verify-receipt --format check-run` emits
the exact payload the Action would post, so the check run is identical.

*What would prove it:* a published tag on an action repository, and a workflow
run in another repository that consumed it and posted a check run.

## The policy is read from the base branch, and the first PR is not gated

`thruline policy` reads `.thruline/policy.yml` from the branch you are merging
**into**, not from the checkout under review — so a change cannot weaken the
gate that judges it. The ref comes from `--base`, defaulting to
`GITHUB_BASE_REF`.

With neither, a `pull_request` run **refuses**:

```text
error: this is a `pull_request` run and no base ref is available (neither
`--base` nor `GITHUB_BASE_REF`). Policy for a pull request is read from the base
branch; refusing rather than judging this change by the policy it may have just
written.
```

Exit 1. It is a refusal and not a pass, because a gate that cannot find its
policy has not agreed with you.

**The pull request that first adds a policy is therefore ungated.** Its base
branch has no `.thruline/policy.yml`, so there is nothing to read and nothing to
enforce — the policy takes effect from the *next* change onward. That is the
correct behaviour, and it is worth knowing before you rely on the first one:
merge the policy on its own, then gate.

## Three axes, combined downward only

| axis | question | answered by |
|---|---|---|
| integrity | does the receipt reproduce here? | `thruline verify-receipt` |
| claims | did every claim in it get proved? | the receipt's own verdicts |
| policy | may this merge anyway? | `.thruline/policy.yml` |

No axis can lift another. A receipt can be perfect on integrity and carry a Not
proven claim; the check run may **never** conclude `success` while any claim is
`unsupported`. Without a policy that case is `neutral`, with the verdict table
rendered. `failure` comes only from policy.

That rule is written down because the alternative was measured: a green
`verified` badge over a receipt containing an unsupported claim, with only the
opt-in policy step red — two verdicts on one pull request, and the louder one
was wrong.

## What fails the job

- A check that **contradicts** the receipt.
- A policy rule that requires a claim the record does not support.
- A shallow checkout, when `require-full-clone` is left at its default — and the
  message is about the CI configuration, not about your receipt.
- Anything that stops the gate from running at all: a policy file that does not
  parse, no execution store. A gate that cannot run has not passed.

A **neutral** conclusion does not fail the job by default. Set `fail-on: neutral`
if your repository requires full verification, and note that it needs the ledger
to be present.

## The conclusion is never re-derived

Both the check run and the comment consume a conclusion the binary printed.
Neither reconstructs one from the parts, and a test walks the action file and
fails if it ever reads `.checks`, `.outcome` or the exit code to decide an
outcome.

The reason is a measured one. A `--json` consumer using the obvious idiom
`all(c.outcome != "fail")` read a **partially verified** receipt as verified,
because such a receipt contains no failing check. That is Couldn't-check
collapsing into Proven, on the most public surface the product has.

## Locally, first

```sh
thruline policy                 # the table, and exit 3 on failure
thruline policy --conclusion    # just `success`, `failure` or `neutral`
```

`--conclusion` prints the one word and nothing else, which is what a shell
script wants — and it is the same word the check run posts, so a local gate and
a remote one cannot disagree.

Same evaluator, same claims, same answer. It exits **3** on a policy failure and
**0** when it passes or is neutral. A developer who can only find out by pushing
has a lottery, not a gate.
