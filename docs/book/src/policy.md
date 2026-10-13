# Policy reference

`.thruline/policy.yml` decides what fails a check run. Start from the shipped
example:

```sh
thruline policy --init
```

```yaml
version: 1
require_receipt: true
claims:
  tests_pass:
    required_when: { paths: ["src/**", "lib/**"] }
    require_reexecuted: true
    min_changed_line_coverage: 70
    allow_skips: false
  behavior_unchanged:
    require_evidence: true
  test_file_changes:
    require_human_ack: true
promises:
  block_on_open: false
  comment_open: true
stop_gate:
  enabled: true
  max_stop_blocks: 2
reexec:
  mode: claimed
  reruns: 2
  image: auto
```

## The schema is closed

An unrecognised key is an **error**, naming the line, not a key that gets
ignored. This matters more than it sounds:

```yaml
version: 1
require_reciept: true    # <- typo
```

Under a permissive reader that file means `require_receipt: false` and your
gate is off. Under this one it fails to load and says
`line 2: unknown key 'require_reciept'`.

The same closure means a policy written for a newer ThruLine fails loudly on an
older one rather than quietly enforcing less than its author wrote.

## It reads a subset of YAML, deliberately

A policy file decides whether a merge is blocked, so the parser's failure mode
matters more than its coverage. Supported: two-space indentation, `key: value`,
nested maps, inline flow lists (`["src/**"]`) and flow maps
(`{ paths: [...] }`), and `#` comments.

Refused **by name**, with the line number, when the construct opens a line:
block sequences (`- item`), merge keys (`<<`), multi-document streams (`---`),
and a leading `&` or `*`. Refused by name with the line number wherever they
appear: tab characters, quoted keys, and a value that is exactly `|` or `>`.
YAML's implicit booleans (`yes`, `no`, `on`, `off`) are refused too, but as a
type error that names the key rather than the line. Implicit typing is where a
file stops saying what its author wrote; `true` and `false` are the only
booleans.

Those tests are narrower than the constructs they name: anchors and aliases are
only looked for at the start of a line, and a block scalar only when the value
is exactly `|` or `>`. So `image: &img auto`, `image: *img` and `image: |-` all
parse, and the value is read as the literal string `&img auto`, `*img` or `|-`
rather than refused.

*What would prove it:* a refusal that names the construct wherever it stands,
and a parser test that feeds each of those three lines in and expects an error.

## Keys

| key | default | meaning |
|---|---|---|
| `version` | *required* | must be `1` |
| `require_receipt` | `false` | fail when `.thruline/receipts` is not a directory. The test is the directory's existence, not any receipt in it, so `mkdir -p .thruline/receipts` satisfies the gate |
| `claims.<kind>.required_when.paths` | always applies | globs; the rule applies when the change touches one |
| `claims.<kind>.require_reexecuted` | `false` | the check must have been re-executed |
| `claims.<kind>.require_evidence` | `false` | evidence must be bound to the claim |
| `claims.<kind>.require_human_ack` | `false` | a label or approving review is required |
| `claims.<kind>.min_changed_line_coverage` | none | percentage, on changed lines only |
| `claims.<kind>.allow_skips` | `false` | whether skipped tests are acceptable |
| `promises.block_on_open` | `false` | fail when a promise is still open |
| `promises.comment_open` | `true` | parsed and folded into the policy digest; nothing reads it yet — no comment lists open promises under either setting |
| `stop_gate.enabled` | `false` | enforce a stop-block limit |
| `stop_gate.max_stop_blocks` | `2` | the limit |
| `reexec.mode` | `claimed` | `claimed` or `full` |
| `reexec.reruns` | `2` | how many times to re-run |
| `reexec.image` | `auto` | container image for re-execution |
| `decisions` | absent | optional closed block enabling the Decisions gate on accepted person decisions; an absent block leaves the gate off |
| `decisions.require_hold` | `blocking` | `blocking` checks accepted `blocks` decisions; `all` also checks `warns`; `none` disables this gate |
| `decisions.on_unverifiable` | `block` | `block` fails on missing evidence; `warn` produces a neutral result, never a hold |
| `decisions.on_stale` | `block` | `block` refuses a stale exact-version receipt; `recompute` runs the check again on the candidate head |
| `templates` | absent | optional map of named proposals for a Change's done criteria; does not change gate verdicts |
| `templates.*` | absent | a template map with required `lines`; its nonempty name may contain ASCII letters, digits, `_` and `-` |
| `templates.*.lines` | *required per template* | inline list of 1–40 nonblank lines, each at most 240 characters |

Globs use `*` within a path segment and `**` across segments. Nothing else is a
metacharacter — a policy glob that needed a regex engine would be a policy
nobody could read.

HQ reads templates from `.thruline/policy.yml` on the repository's base branch
for preview. Nonempty templates are included in the policy digest; their lines
remain proposals until a person accepts them. Reading a template writes nothing
and accepts no criteria. Offer
and Stop remain **Not yet** in v0.2; they require the reviewed policy
edit-and-commit flow planned for 0.2.1.

## What policy is not allowed to see

**Policy evaluates rules-extracted claims and exact-version person-accepted
Decisions.** A claim a model added is advisory: it shows up in `thruline
explain`, it may read `unsupported`, and it can never fail a gate. No policy
key can reference the extractor, a score, a
confidence or a threshold — the schema has no such key, and a test asserts that
it never gains one.

The reason is measurable rather than philosophical: this repository's own
records show 27 of 30 model-sourced emitted lines were false accusations. A
model that wrong must not be able to block a merge.

Claims that were filtered out are **counted and reported**, so a reader can see
that a model-sourced finding existed and was advisory, instead of wondering why
it never appeared.

## Three conclusions, not two

- **failure** — a claim the policy requires is contradicted by the record.
- **neutral** — something the policy asks about could not be decided. Nothing
  is blocked, and the check says what would settle it.
- **success** — everything the policy requires is supported.

An unmeasured input is neutral, never a pass. A policy asking for 70% coverage
that receives no measurement has not been satisfied, and calling that green is
the collapse the verdict model exists to prevent. The same holds for a skip
count that could not be parsed and for a human acknowledgement this run cannot
see.

If the changed-path list itself could not be determined, `required_when` rules
still apply. A gate that switches itself off when it cannot see the diff is a
gate that switches itself off exactly when something has gone wrong.
