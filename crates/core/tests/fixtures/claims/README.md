# Synthetic claim-bearing sessions

Five spool files, one session each. **Synthetic, and written for this
directory:** every prompt, message, command, path and output here was composed
for these fixtures. Nothing is copied from a real session, from `../eval/`, or
from `decorator_payload.json`.

They exist so that a fixture session carries claims. The transcript fixtures
beside this directory reach the execution store through `import-legacy`, which
records no `stop` event, and claims are extracted only from a `stop` event's
`last_assistant_message` (`crates/cli/src/claims_cmd.rs`, `extraction_for`), so
every one of those sessions has zero claims.

Two tests hold them:

- `crates/core/tests/claim_fixtures_synthetic.rs` checks every line against the
  conventions below, with a negative control per rule;
- `crates/cli/tests/claim_fixture_sessions.rs` folds each file through the real
  binary and pins what `claims --json` and `explain --json` say about it.

## How they are read

These are **spool lines**, not transcripts:

1. copy the file to `<work>/…/spool/claude-code.jsonl`. Never fold the committed
   file in place: a held-back line writes `<spool>.quarantine` beside whatever
   was folded;
2. `thruline fold --spool <copy> --exec-db <store>/exec.db --repo <work repo> --json`;
3. `thruline claims --last --exec-db <store>/exec.db --repo <work repo> --json`,
   then `thruline explain --session <id> … --json`.

The work repository is the one `scripts/hq-invariance.sh` builds:
`git init --template=`, one file `README` holding exactly
`hq-invariance work repository: one fixed commit, so receipts name a non-empty tree.`
followed by one newline (84 bytes), one commit under a fixed identity and date.
Its tree is `c4eeb65ef9132009ad33aeb19abd8785cad50d8c`: blob
`1195965aa50c258fbed227afffb66f8d9360e91d`, mode `100644`, name `README`.

Every exec line names that tree as its `input_tree_hash`, so each check binds to
the final tree by §6.1 rule 1. **If those README bytes change, the tree changes,
and every `supported` below silently becomes `unverifiable`.**
`claim_fixture_sessions.rs` asserts the tree before it folds anything.

Because the work repository's start tree and final tree are the same tree, the
session diff is empty. So these fixtures do **not** exercise F5's
`unclaimed_completion@1` floor, tamper detection, coverage, the diff-based claim
kinds, §6.1 rule 2 or supersession. The `fs_change` paths name files the tree
does not hold; nothing on this branch cross-checks them.

## Conventions

| field | value |
|---|---|
| session id | `cccccccc-000N-4000-8000-000000000000`, N = the file's number |
| tool use ids | `toolu_claimsNN_edit`, `toolu_claimsNN_bash` |
| `received_at` | `2026-01-02T00:MM:SSZ`, second precision, as `ingest::iso8601_utc_now` writes it. Line n is at minute n; the exec line is at `00:06:40Z`, with `started` `00:06:10Z` and `ended` `00:06:40Z` equal to its `received_at` |
| absolute paths | `/Users/dev/demo` and paths under it, nothing else |
| line bytes | compact JSON, keys sorted at every level, one object per line, `\n`-terminated |
| order | `SessionStart` first, `Stop` last, exactly one `Stop` |

Every session has the same eight lines: SessionStart · UserPromptSubmit ·
PreToolUse Edit · PostToolUse Edit · PreToolUse Bash · exec · PostToolUse (or
PostToolUseFailure) Bash · Stop. `fold` numbers line n as seq 4n
(`SPOOL_SEQ_STRIDE`). The Edit's `fs_change` is seq 17 and its snapshot 18, and
the Bash snapshot is seq 29. That makes 11 events, 2 snapshots, 1 exec and 1
check per session, and the Stop (seq 32) comes after the last activity, so
capture is not degraded.

### Hashes: every one derived, none typed

- **Hook line `raw_sha256`.** The hook takes sha256 of the raw stdin delivery
  (`crates/capture/src/claude_code.rs`, `prepare_for`). A synthetic line has no
  stdin, so its raw delivery is defined as the payload's canonical JSON
  (compact, sorted keys), with no redaction applied (`redactions_applied: 0`).
- **Exec line `raw_sha256`.** sha256 of `"{session_id}|{received_at}|{spec_payload}"`,
  the recorder's own material (`crates/capture/src/exec.rs`, `spool`), with
  `spec_payload` in canonical JSON.
- **`argv_hash`.** `sha256:` + sha256 of the argv joined with U+0001 (`exec.rs`).
- **`env_hash`.** `sha256:` + sha256 of the synthetic environment `LANG=C`,
  `TERM=dumb`, `TZ=UTC`, sorted and joined with `\n` (`exec.rs`, `env_hash`).
- **`stdout_hash` / `stderr_hash`.** `sha256:` + sha256 of the excerpt, which
  stands for the whole stream; an empty stream hashes the empty string.
- **No `stdout_store` / `stderr_store`.** The check builder reads runner output
  only from those files (`crates/capture/src/fold.rs`, `captured_output`), so
  every check here is built from its argv and exit code alone. A `cargo build`
  or `cargo clippy` check needs nothing more. A test check gets no parsed
  counts.

Why canonical JSON: `serde_json`'s key order depends on whether a build unifies
its `preserve_order` feature (`Cargo.lock` lists `indexmap` under `serde_json`).
Lines written already sorted hash the same under either.

### Masking

The hook masks every string before it spools, and these lines never passed
through it. `fold` re-masks `tool_input`, `tool_response` and `error` and
debug-asserts that nothing changed (`fold.rs`, `compact`). So no payload string
carries a hex, base64 or credential-shaped run, and every string is a fixpoint
of `redact::mask_recorded`. The only hex on any line is in the hash fields above
and in the tree id.

## What each session must produce

Verdicts are the engine's words. The file names use the three cases the
operator's ruling for this commit named: supported (02 and 05), contradicted
(03) and insufficient evidence (04), with 01 the M8 scripted fake. That mapping
is this README's, not the engine's: no code maps the per-claim verdicts onto
those words. HQ
words the same verdicts through `verdictWord` in `ui/src/workspace/adapter.js`:
`supported` is `proven`, `unsupported` is `not_proven`, and `unverifiable` is
`couldnt`, or `fog` when capture is degraded.

| file | the Stop message claims | the record holds | claims: kind · verdict · verifier · binding | findings | promises | `why_it_stopped` | `claims` and `explain` exit |
|---|---|---|---|---|---|---|---|
| `01-three-asks-one-done-one-claimed-one-untouched` | the build succeeds; all tests pass | `cargo build` exit 0 | `build_ok` · `supported` · `build_ok@1` · `ran_on_final_tree`; `tests_pass` · `unverifiable` · `tests_pass@1` · none ("no test check was captured in this session") | none | none | `agent_finished` | 0 |
| `02-supported-lint-claim` | clippy is clean | `cargo clippy --all-targets` exit 0 | `lint_ok` · `supported` · `lint_ok@1` · `ran_on_final_tree` | none | none | `agent_finished` | 0 |
| `03-contradicted-tests-claim` | all tests pass | `cargo test` exit 101, then `PostToolUseFailure` | `tests_pass` · `unsupported` · `tests_pass@1` · none ("a captured test run failed: `cargo test` exited 101") | `test_run_failed` (fail) | none | `error`: a failed tool is among the last four events | 3 |
| `04-insufficient-evidence-tests-claim` | all tests pass | `pytest -q` exit 0, empty output | `tests_pass` · `unverifiable` · `tests_pass@1` · `ran_on_final_tree` ("the runner's output could not be parsed") | none | none | `agent_finished` | 0 |
| `05-open-promise-beside-supported-build` | the build succeeds; a follow-up | `cargo build` exit 0 | `build_ok` · `supported` · `build_ok@1` · `ran_on_final_tree` | none | one, `follow_up`, `open` | `agent_finished` | 0 |

01's second claim and 04's claim are both `unverifiable`, for different reasons.
In 01 nothing ran that could show the tests either way. In 04 a test run exists
and is bound to the final tree, but its result cannot be read.

## Fixture 01 and the M8 scripted fake

`docs/ADDENDUM-M7-M8.md` §8 on `codex/p3-narration-asks` asks for a session where
the user asks for three things, the agent does one, claims a second without
evidence and never mentions the third, resolving to
`proven / claimed_only / untouched`. This branch computes no ask status, so
fixture 01 pins only the claim-level record above.

The statuses it is written to produce once p3 merges are below. **They have not
been run.** They are read from p3's `crates/asks/src/resolve.rs` (`status_of`),
`crates/asks/src/anchor.rs` (`paths_match`, whole-segment suffix) and
`crates/x-cli/src/load.rs`, which takes changed paths from `fs_change` events.

The expectation for ask 1 is the weakest. The spec's rule for `proven`
(`docs/ADDENDUM-M7-M8.md:148` on that branch) needs the ask's target paths in the
final-tree diff. This fixture's work repository never changes, so that diff is
empty. p3's loader builds its changes from `fs_change` edits instead
(`crates/x-cli/src/load.rs:311-318`), and that is what makes ask 1 `proven` in the
table. If the resolver follows the spec's wording rather than the loader, ask 1
does not resolve to `proven`. Nothing here proves either reading until p3 merges.

| ask | target | expected | because |
|---|---|---|---|
| 1. fix the compile error in `src/parser.rs` | `src/parser.rs` | `proven` | "As you asked in 1, … the build succeeds." anchors a `build_ok` claim that is `supported`, and the `fs_change` for `/Users/dev/demo/src/parser.rs` covers the target |
| 2. get the test suite passing again | none | `claimed_only` | "As you asked in 2, all tests pass." anchors a `tests_pass` claim that is `unverifiable` |
| 3. add a section on `--strict` to `docs/usage.md` | `docs/usage.md` | `untouched` | never mentioned and never edited |
