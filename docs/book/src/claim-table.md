# The claim table

A claim is only found if some pattern recognises the sentence the agent wrote.
That pattern table is built in, and a repository may replace it.

```text
.thruline/claims.yml
```

Absent — the normal case — the built-in table is used. It is the one place that
decides what counts as a claim, and it is covered by tests.

## What it looks like

```yaml
version: 1
patterns:
  - kind: tests_pass
    scope: all
    phrases:
      - "all tests pass"
      - "the full test suite passes"
      - "all green"
  - kind: tests_pass
    scope: named
    phrases:
      - "this test passes"
```

`kind` is a claim kind from [the verifier reference](./verifiers.md). `scope` is
`all` or `named`, and it is load-bearing: a narrowed run can honestly support a
`named` claim and never an `all` one, so mis-scoping a phrase changes verdicts.
`phrases` are matched against the agent's final message.

A file that does not parse is an **error**, never a quiet fall back to the
defaults. The reason someone writes this file is that the built-in table misses
something of theirs, and ignoring it silently would drop exactly the claims they
asked to have caught.

## What it does today, and what the ruling says it must do

These are two different things, and the difference is the whole reason this
section exists.

**Since the core's `533b76f` it ADDS to the table.** `Table::load` is
`DEFAULT_TABLE ∪ your patterns`, and a repo table that tries to shadow a default
phrase is a hard error rather than a silent override. The paragraph below
described the behaviour before that landed, and is kept because the hazard it
names is the reason the union exists.

**Before `533b76f`, it replaced the table.** `Table::load` parses your file into a fresh
pattern list; the built-in phrases are not merged in. A repository that writes
`claims.yml` to add one phrase of its own, and does not copy the defaults across,
**silently stops recognising every default phrase** — including "all tests
pass", the sentence this product is chiefly about. Nothing warns you. The
session simply reports fewer claims, which reads exactly like an agent that
claimed less.

**The ruling is that a repo-supplied table may only add, never remove.** A file
that omits a default must not thereby disable it.

**The digest now exists, and is not where a consumer would look for it.** The
effective table carries a `sha256:`, and it reaches the receipt inside
`verdicts[].warnings` as a sentence, with the file's path — not as a field of
the predicate. A consumer wanting to key on which patterns were in force must
parse prose out of a warnings array, where `verifier.policy_hash` is a field one
level up. That half of the ruling is filed as IR-2 and still open.

**The ruling requires the table's digest in the receipt.** A receipt is
supposed to say what it was judged under, and which patterns were in force is
part of that — the same reason `verifier.policy_hash` exists. A non-default
table currently leaves no trace in the receipt at all, so two receipts judged
under different tables are indistinguishable.

Neither half is implemented. Filed as IR-2 in `docs/tracks/D/INTERFACE-REQUESTS.md`, because
both live in `crates/claims` and `crates/attest`.

*What would prove it:* a `claims.yml` containing one phrase, and a session in
which a default phrase is still recognised — plus that table's digest visible in
the resulting receipt.

## Until then

If you write this file, **copy the built-in table and add to it.** Do not write
a short file expecting it to extend the defaults; today it will not.

```sh
thruline claims --last
```

is the check: if a claim you expected is missing after adding `claims.yml`, the
table replaced something you needed.
