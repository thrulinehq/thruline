# Quickstart

[Install first](./install.md) — it takes a C++ toolchain that nothing else on
this page mentions.

## See what it does, with nothing set up

```sh
thruline demo
```

That builds a real git repository in a temporary directory, lets a scripted
agent edit a source file, weaken a test rather than fix it, delete another
test's assertion, run **one** narrowed check for real, and then say *"All tests
pass."* Then it prints what the record shows.

It touches no ledger, no config and no network, and it needs **pytest** on
`PATH`, because it runs a real test ([Install](./install.md) says how to get
it). When it cannot find it, it says so and stops rather than simulating a
run — a simulated test run is the exact thing this product exists to catch.

## Turn on capture

Capture has two halves and you want both.

```sh
thruline init                 # the hooks: what the agent did
thruline init --yes           # ... or this, in a script: no consent prompt
thruline shim install         # the shim: what actually ran
```

`init` sets up **Claude Code** and prints exactly which files it touched. It asks
once, for consent to the speaker hooks; `--yes` answers it, which is what a
script or a container image needs and what this book's own acceptance walk uses.
Without a terminal to ask, `init` installs no speaker hooks unless `--yes` is
given, and says so in one line.
`thruline init --undo` reverses precisely that. For other harnesses, see the
per-harness pages below — the paths differ, and one of them is not built yet:

- [Claude Code](./quickstart-claude-code.md) — `thruline init`
- [Codex](./quickstart-codex.md) — `thruline hook-install --harness codex`
- [Gemini CLI](./quickstart-gemini.md) — **not installable yet**, and the page
  says what that means
- [Any agent, under `thruline run`](./quickstart-run.md) — no hooks at all

`shim install` prints a `PATH` line. Export it, or the shim is installed and not
in use. `thruline shim status` answers "is it installed, unmodified, and
actually first on `PATH`?" — three questions, because the first two can be yes
while the third is no.

## Run your agent, then fold

```sh
thruline fold
```

A hook writes one line and exits: its budget is 50 ms, and a hook that does real
work is a hook that gets disabled. `fold` is where the chain, the snapshots and
the checks are built. It is idempotent — folding the same spool twice writes
nothing the second time — and it exits 1 when a line could not become an
event, a snapshot or a check could not be built, the quarantine file could not
be written, or one session turned up in two spools. One failure is outside that
guarantee: an `fs_change` event that was due and could not be built is counted
in the fold report's `fs_changes_failed`, which nothing prints and the exit code
does not consult, so that fold exits 0 and says nothing.

*What would prove it:* a fold over a spool holding a failed `fs_change` that
names it in the report and exits non-zero.

### Ignore your build artefacts, or they will supersede your evidence

A snapshot's tree hash covers everything in the working directory that git would
add — **including files that are untracked but not ignored**. So a build
directory, a `__pycache__`, a coverage file or a compiled asset written *after* a
check ran counts as the tree changing under that check, and the check stops being
evidence for the tree you are asking about.

The verdict you get is `unsupported`, and it reads like this:

> superseded: the tree shows `src/__pycache__/rate.cpython-313.pyc` changed after
> this check ran, and NO edit event named it — a write through a tool that
> reports no path

Nothing has been tampered with. The rule is doing its job — a write nothing
reported is exactly what it is built to catch — and a compiled cache happens to
look like one. **Put your build artefacts in `.gitignore`**, which any
established repository already does; a fresh one often does not, and this is the
first thing it costs you.

`.thrulineignore` works the same way for paths you want excluded from snapshots
without ignoring them in git.

## Read the session

```sh
thruline claims          # what was claimed, and what the record proves
thruline explain         # the same, with the evidence and the timeline
thruline promises        # what it said it would do later
thruline tui             # the flight recorder
```

`claims` and `explain` exit **3** when a claim's verdict is `unsupported` —
the word they print — so CI can tell a refuted claim from a broken command.

## Gate it, and make a receipt — in this order

The three commands are order-dependent in both directions, and neither
direction is obvious:

```sh
thruline policy --init                                   # 1. write the policy
mkdir -p .thruline/receipts                              # `--out` will not create it
thruline receipt --out .thruline/receipts/latest.json    # 2. bind a receipt to it
thruline policy                                          # 3. the gate asks for one
thruline verify-receipt .thruline/receipts/latest.json --offline
```

**The policy must exist before the receipt.** A receipt records the digest of
the policy it was judged under, and "no policy file" is a real digest, not a
blank. Build the receipt first and it records *that* — then writing a policy
makes the receipt fail verification, with `policy_hash` reporting a mismatch
against a policy that did not exist when the receipt was made.

**The receipt must exist before the gate — though the gate checks less than
that.** The shipped example policy sets `require_receipt: true`, so in a
repository that has never made one, `thruline policy` fails on *"policy requires
a receipt and none was found"*. Know what that rule tests: `receipt_present` is
`.thruline/receipts/` being a directory and nothing else
(`crates/cli/src/policy_cmd.rs`), so an empty one — the `mkdir` above, or a
directory whose receipt you deleted — already satisfies it, and no receipt is
ever opened. Passing this rule is evidence that the directory exists, not that a
receipt does. `verify-receipt` is what reads the receipt itself.

Get it the other way round in either direction and the failure is real,
correct, and about nothing you did wrong.

`thruline policy` exits **3** when policy fails and **0** when it passes or is
neutral. Couldn't-check is neutral, which is the third verdict surviving all the
way to the merge button instead of being rounded away.

Note the verb on the last line: **`verify-receipt`**. There is also a
`thruline verify`, and it does something else — it recomputes the hash chain
over the raw files.

### Expect `partial`, not `verified`, the first time

> The receipt is built **after** the session ends, which is why
> `capture_fidelity` passes above. A mid-session receipt is degraded and neutral
> and should be: work done after it can still falsify a claim inside it. The
> core is separately improving the *sentence* a live receipt gives — it will say
> the session is still running rather than that a hook is missing — which
> changes no conclusion here.


`verify-receipt` on a freshly made receipt usually exits **2** — *partially
verified* — and that is the honest answer rather than a fault. Two causes are
routine:

**`head_binding` skips**, because the tree you are reviewing differs from the
tree your checks ran on:

> the tree being reviewed differs from the tree the checks ran on at 2 path(s):
> `.thruline/policy.yml`, `src/rate.py`. Nothing in a receipt records which
> paths its checks covered, so this build cannot say those changes are harmless
> — only that the evidence does not reach them.

**The order above guarantees it.** `thruline policy --init` writes
`.thruline/policy.yml` after your tests have already run, so the final tree
always carries at least one path the checks never saw. Your own edits since the
test run add the rest.

**`capture_fidelity` skips too if your session never ended.** A receipt built
while the session is still running is degraded — a record that stops without
saying so is indistinguishable from a crashed agent, a killed terminal, a hook
over its timeout, or a session still going. Build the receipt *after* your agent
finishes and this one passes; the flow above does.

Neither is a contradiction, which is why the exit code is 2 and not 1. Read the
per-check lines: `ok` is verified, `skip` is *not checked here*, and only `FAIL`
means the record disagrees with the receipt.

Then [put it in CI](./ci.md).
