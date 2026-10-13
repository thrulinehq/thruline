# HQ Next — the `/api/hq/` routes

Engine: `crates/core/src/hq.rs` and `crates/core/src/hq/{prompts,edits,commands,codemode,tally,view}.rs`. Routes: `crates/cli/src/uiserve/hq.rs`. The rules (what a project, prompt, turn, thread, version and "whose move" are) are written once, in the module docs of `crates/core/src/hq.rs`. This file is the wire contract.

## General rules

- **Reads write nothing.** Every read opens the ledger and derives at request time. The routes cache each project's index in memory, keyed on its members' `project_heads` and session counts, so a second read of an unchanged project skips the rebuild.
- **Masking.** Every string is masked by `redact::mask_recorded` before it is served. Each document carries `masking: {masked, families, complete}`, the merged record of the masking that produced it.
- **Host and Origin.** The same Host/Origin checks apply as on every route of `thruline ui`.
- **Queries.** A query string is refused with `400 {"error":"unknown_query_parameter"}`, except on `GET /api/hq/projects/<id>/search` (C30 ⌘K record search), which takes `q`, `limit` and `thread` and refuses any other parameter the same way.
- **Errors.** Errors are `{"error": "<code>"}`. The codes:
  - 400 `malformed_request`;
  - 403 `same_origin_required`;
  - 404 `no_such_project` / `no_such_thread` / `no_such_session` / `no_such_route`;
  - 405 `method_not_allowed`;
  - 409 (a load is already running; the body is the load status);
  - 500 `read_failed` (with `detail`);
  - 503 `store_unavailable` (with `detail`, for example a pending schema upgrade);
  - 503 `store_held` (with the fixed, path-free `message` "ThruLine is removing a project's records; try again when it finishes") when the lead route's store open is refused by a removal hold;
  - 503 `store_busy` (with `waited_ms`, `detail` and `message`, a sentence for a person) — another ThruLine process held the record's write lock for the whole 30 s wait. Returned by `POST /api/first-use/register`; the load reports the same case per file (below);
  - 507 `not_enough_disk` (with `need_bytes`, `free_bytes`).
- **Times.** Times are the recorded RFC 3339 strings, unchanged. `whose_move` is the only field computed against the server's clock (see below).
- **Ids.** Event ids are the ledger's `events.id`.
  - A command or edit inside a Codex code-mode program, or one of several files in one patch, has id `<event_id>#<n>`.
  - A claim has id `<event_id>#<n>`, its index in that message.
- **Paths.** A path inside the project root (or a member's root) is served relative to it. Any other path is served as recorded (masked).
- `raw_file`/`raw_line` is the receipt: the transcript (or sealed mirror copy) and line the row came from.

## `GET /api/hq/projects`

```json
{ "schema_version": 1, "projects": [ ProjectSummary | QuickSummary, … ], "excluded_temp_projects": 2,
  "complete": false, "projects_complete": 12 }
```

**This route never builds a project index and never waits for one.**
- At `thruline ui` start, a background thread computes every project's row, newest project first, and keeps the newest project's index for its screens.
- A project whose row is ready is served whole. That is a `ProjectSummary` with `complete: true`; its `whose_move` is recomputed at the request's instant.
- A project whose row is not ready yet is served as a `QuickSummary` with `complete: false`. It carries `id, name, root, path_known, sources, members, sessions, events` (recorded rows of every kind), `first_activity` and `last_activity`. Every count only the index can decide is `null`, never 0: `prompts`, `conversation_prompts`, `edits`, `commands`, `tests`, `claims`, `threads`, `conversation_only`, `unattached_turns`, `copied_rows_skipped`, `whose_move`.
- Top-level `complete` says whether every row is whole; `projects_complete` counts the whole ones. A client re-reads until `complete` is true.

`excluded_temp_projects` is how many repository roots were left out because they are scratch: the root is inside a temporary folder (`/tmp`, `/private/tmp`, `/var/folders`, `/private/var/folders` or the OS temporary directory) and is not inside a git repository on disk. The root is the recorded `cwd` when one can be read, else the stored root. `GET /api/first-use` leaves the same folders out of `candidates` and carries the same field.

At `thruline ui` start, the most recently active project's index is built on a background thread. A request for that project that arrives meanwhile waits for that build rather than starting a second one.

## `GET /api/hq/projects/<id>` → `ProjectView`

```json
{ "schema_version": 1, "project": ProjectSummary, "threads": [ThreadSummary, …], "masking": {…} }
```

The threads are ordered by last activity, newest first.

### ProjectSummary

| field | meaning |
|---|---|
| `complete` | `true` (a `QuickSummary` says `false`; see above) |
| `id` | `sha256("hq-project-v1\0"+root)[..16]` |
| `name`, `root` | last path component; repository root. While `path_known` is `false` both are the vendor's folder name as stored (for example `-Users-me-my-app`), **not a path** |
| `path_known` | `false` when no record states where the folder is: its newest recordings state no `cwd`, it is not a root another recording names, and it was not picked by path on the First run screen. `root` and `name` are then the vendor's folder name as stored, never a decoded guess, and the screens say "path not known until the first recording". `id` is unchanged by this (it is still hashed from the name decoded, which is internal and never served as a path). Read `true` when absent. |
| `sources` | `claude_code` and/or `codex` |
| `members[]` | `{ledger_project_id, source, root, root_basis}`: the ledger projects folded into this one. `root_basis` is one of:<br>• `recorded_cwd` — the recordings state it;<br>• `known_root` — a Claude Code folder with no `cwd` whose name IS a root another recording states, or is a worktree folder (`--claude-worktrees-`) inside one;<br>• `folder_name` — nothing states the path: `root` is the folder name as stored, not a path (`path_known: false`). A name merely beginning with another project's name (`proj-two` after `proj`) is not inside it;<br>• `stored_root` — Codex, the real path; or a Claude Code folder picked on the First run screen (its stored root is the path that was picked: it differs from what its name decodes to, or nothing is recorded in the folder yet). |
| `sessions` | recordings, both vendors, sub-agent sessions included |
| `prompts` | the person's work prompts (their turn edited or ran something) |
| `conversation_prompts` | the person's prompts that started no work |
| `edits` | file edits: one per file per edit call (a patch touching 3 files is 3) |
| `commands`, `tests` | `{total, succeeded, failed, unknown}` (rule: `hq/commands.rs`) |
| `claims` | claims of finished work (`MarkerExtractor`, kinds `work`/`marker`) |
| `first_activity`, `last_activity` | first and last recorded instant |
| `threads` | number of threads |
| `conversation_only` | the person's prompts with no thread open and no work |
| `unattached_turns` | turns that did work with no thread to join: work before any person's prompt, and orphan sub-agent sessions |
| `copied_rows_skipped` | rows of resumed or forked transcripts that copy an earlier session (counted once) |
| `whose_move` | `{you, agent, done}`: thread counts |

### ThreadSummary

| field | meaning |
|---|---|
| `id` | `sha256(first work prompt event id)[..16]`, stable |
| `title` | a sentence the person wrote, verbatim, masked, ≤ 90 characters with `…`, chosen by the C26 rule below |
| `title_basis` | `work_sentence` \| `work_sentence_later_prompt` \| `pasted_work_sentence` \| `fallback_instruction` \| `fallback_first_sentence` \| `pasted_heading` \| `pasted_first_sentence` \| `none` |
| `title_event_id` | the prompt the title came from. If the first prompt has no sentence (an image only), this is the thread's next prompt that has one. |
| `started_at`, `last_activity_at` | |
| `sessions`, `prompts`, `conversation` | counts |
| `edits`, `files`, `top_files[] {path, edits}` | up to 5 files, most-edited first |
| `commands`, `tests` | `{total, succeeded, failed, unknown}` |
| `commits` | `git commit` calls that did not fail |
| `claims` | |
| `whose_move` | `you` \| `agent` \| `done` |
| `whose_move_basis` | `recent_activity` \| `claim_without_passing_test` \| `last_test_failed` \| `agent_asked` \| `nothing_open` |
| `proof.last_test` | `{event_id, command_id, at, outcome, command}`, or `null` |
| `proof.last_passing_test_at` | |
| `proof.edited_after_last_pass[]` | files edited after the last passing test run (the canvas's "changed after the run") |
| `current_version_id` | the version of the thread's last row |
| `told_agent` | **additive (C30 R-LEDGER), absent when the thread has no act:** `{last_act, at, version_id, act_id, later_work}` — the last time a person copied a message for the agent on this thread (`copied`), said they pasted it (`marked_pasted`) or took that back (`unmarked_pasted`); when; the version the thread was at; the row; and whether any row of the thread was recorded after the act (`later_work`, derived at read, never stored, judged to the whole second). No text. Moves no `whose_move`, no count, no order. Only the **project** view carries it; see "`POST …/told-agent`" below |

### The title rule (C26)

The title is a sentence the person wrote, never rewritten. It is the first that holds of:

1. **`work_sentence`** — the first sentence the person typed, outside pasted blocks, that is not skippable and names work, taken from the thread's first work prompt.
   - **Names work.** After fillers ("please", "can you", "I want you to", "let's"), the sentence opens with an instruction verb that is not a go-on verb, and an object follows: a word that is not a pronoun, particle or adverb, or a path / `` `code` `` token. The go-on verbs are continue, start, resume, proceed, go, carry, let, help and try.
   - **Skippable**, any of:
     - (a) three words or fewer that name no work ("Start now.", "Go ahead.");
     - (b) a question that names no work and asks the agent to go on, continue or take over ("Can you continue this in …?");
     - (c) a role or instruction preamble: "You are …", "You're …", "Your role …", "Context:", "Background:", "Note:", "Rulings:", "Read this first", "Read … first", an instruction to read a brief ("Read the rules.", "Read `X.md` and do it."), any line ending in `:`, or a lane or role heading that names no work.
   - A question that names a change is work: "Can you fix the login redirect?" stays eligible.
2. **`pasted_work_sentence`** — a work sentence among the first six sentences of a pasted block. A sentence deeper in a pasted brief never titles.
3. **`work_sentence_later_prompt`** — the first later prompt of the thread with a work sentence. Decided because a thread that opens with "Start now." and then asks for a change is about that change.
4. **The pre-C26 rule on the first work prompt.** The first instruction sentence (`fallback_instruction`), else the first sentence (`fallback_first_sentence`), else a pasted heading (`pasted_heading`) or the pasted first sentence (`pasted_first_sentence`). If even that finds nothing (an image-only prompt), the first later prompt with any title.

A prompt made only of attachments, such as images, has no sentence. If no prompt in the thread has one, the title is empty with `none`.

`title_event_id` names the prompt the title came from.

Whose move is decided by the first of these that holds:

1. `agent`: the thread's last row (a prompt, tool call, tool result or assistant row) is within 10 minutes of now.
2. `you`: any of:
   - **an open claim.** A claim is open unless the latest test run before it passed with no edit in between, or a passing test run follows it.
   - **the last test run failed.**
   - **the agent asked.** Its last words end in `?`, or its last tool call was `AskUserQuestion` or `request_user_input_async`.
3. `done`: otherwise.

The brief's literal "claim with no later passing test run" was refined to this rule; see DECISIONS.md.

For a thread with `whose_move_basis: "agent_asked"`, `waiting_event_id` is the opaque ID of the newest question row after the latest user row: an `AskUserQuestion` or `request_user_input_async` tool call, or assistant words ending in `?`. It is absent when an older question was followed by a user answer, even if the older `whose_move_basis` has not changed. The open-page Agent waiting notification requires this ID and deduplicates by it; it never treats the thread's activity time as a question identity.

Example from the scratch store (2026-10-07: 539 sessions, 144 threads):

```json
{
 "schema_version": 1,
 "project": {
  "id": "dfbfd6089e4bb388", "name": "thruline", "root": "/Users/…/thruline",
  "sources": ["claude_code", "codex"],
  "members": [{"ledger_project_id": "-Users-…-thruline", "source": "claude_code", "root": "/Users/…/thruline", "root_basis": "recorded_cwd"}, "…"],
  "sessions": 539, "prompts": 505, "conversation_prompts": 116, "edits": 6361,
  "commands": {"total": 65608, "succeeded": 16021, "failed": 435, "unknown": 49152},
  "tests": {"total": 2645, "succeeded": 1937, "failed": 85, "unknown": 623},
  "claims": 7142, "first_activity": "2026-07-28T20:37:59.553Z", "last_activity": "2026-10-06T21:49:04.483Z",
  "threads": 144, "conversation_only": 13, "unattached_turns": 159, "copied_rows_skipped": 3080,
  "whose_move": {"agent": 1, "done": 56, "you": 87}
 },
 "threads": [{
   "id": "2922e7da1306e3f6", "title": "Can you continue this in cloud session?", "title_basis": "instruction",
   "title_event_id": "01M49HRTQSD14ECKKZ1XTKR6WX",
   "started_at": "2026-10-06T21:13:33.011Z", "last_activity_at": "2026-10-06T21:21:04.323Z",
   "sessions": 1, "prompts": 2, "conversation": 0, "edits": 1, "files": 1,
   "top_files": [{"path": "/Users/…/thruline-hq/docs/hq-next/HANDOFF.md", "edits": 1}],
   "commands": {"total": 17, "succeeded": 16, "failed": 1, "unknown": 0},
   "tests": {"total": 0, "succeeded": 0, "failed": 0, "unknown": 0},
   "commits": 2, "claims": 5, "whose_move": "you", "whose_move_basis": "claim_without_passing_test",
   "proof": {"last_test": null, "last_passing_test_at": null, "edited_after_last_pass": []},
   "current_version_id": "wt-00bb958a3c8b2ae7"
 }, "…"],
 "masking": {"masked": 31, "families": ["email", "high_entropy", "openai"], "complete": true}
}
```

## `GET` / `POST /api/hq/projects/<id>/inbox-hides` (C30 Inbox)

`GET` returns `{schema_version:1, project, rows:[{row_kind, thread_id, thread_event_id, row_sha256, hidden, act_id?, actor?, recorded_at?}]}`. There is one current address per project thread: `prompt` while its `whose_move` is `you` because the agent asked, otherwise `thread`. The digest includes that kind and every ordered event ID in the thread. A newly recorded event changes the address, so an earlier hide no longer applies. A repeat GET appends nothing.

`POST` takes exactly `{"act":"hidden"|"shown","row_kind":"thread"|"prompt","thread_id":"…","expected_sha256":"…64 lowercase hex…"}` and requires this server's `Origin`, JSON framing, and a body of at most 512 bytes. It checks the current project record fingerprint and row address under the ledger write lock, then appends one `inbox_hides` act and returns `{schema_version:1, project, row:{…}}`. A client rereads `GET` after a successful act. The name on the act is the account ThruLine runs as. A show act refers to the standing hide; no source recording or `whose_move` value is changed.

Malformed input is 400; an unknown current row is `404 no_such_row`. A changed record or digest, an already hidden row, or a row that is not hidden to show is 409 (`row_changed`, `already_hidden`, `not_hidden`); these refusals append nothing. Missing or foreign Origin is `403 same_origin_required`. The route uses the existing append-only `inbox_hides` table and adds no schema.

## `GET /api/hq/projects/<id>/threads/<tid>` → `ThreadView`

| field | meaning |
|---|---|
| `project` | `{id, name, root}` |
| `thread` | ThreadSummary |
| `prompts[]` | `{event_id, session_id, seq, ts, text, segments, work, version_id, raw_file, raw_line}`. `segments: [{kind: typed\|pasted, text}]` splits the prompt into what was typed and what was pasted, wrapper tags removed, each part masked and trimmed of blank lines. A prompt with no paste is one `typed` segment. A paste with no closing tag runs to the end. `text` is the whole prompt, masked and never truncated. `work: false` marks conversation. A slash command with arguments is `/name args`. |
| `edits[]` | see below, in thread order |
| `commands[]` | see below, in thread order |
| `claims[]` | see below |
| `messages[]` | Recorded assistant text, including messages with no classified claim: `{event_id, session_id, ts, text, version_id, raw_file, raw_line, recorded_while_paused?}`. Text is masked and carries the event identity and raw receipt for a readable step; it has no verdict. Absent when empty. |
| `versions[]` | see below, in thread order |
| `current_version_id` | same as `thread.current_version_id` |
| `sessions[]` | `{id, ledger_project_id, source, origin: person\|subagent, started_at, last_at, file}` |
| `files[]` | `{path, edits, lines_added, lines_removed}` |
| `masking` | |

### Edit

```
{id, event_id, session_id, ts, file, op: edit|write|multiedit|notebook|patch,
 action: create|update (write) | add|update|delete (patch) | null, moved_to,
 hunks: [{old_start, old_lines, new_start, new_lines, header, lines: [{kind: context|added|removed, text}]}],
 lines_added, lines_removed, ok: true|false|null,
 source: structured_patch|apply_patch|request, via: tool|codex_patch|codex_exec,
 version_id, raw_file, raw_line}
```

The hunks come from one of three sources, in this order:
1. **`structured_patch`**: Claude Code's `toolUseResult.structuredPatch`. It carries the real line numbers and the context lines.
2. **`apply_patch`**: the Codex patch text. `header` is the text after `@@`. `old_start`/`new_start` are `null` because Codex states no line numbers; an added file has `old_start: 0, new_start: 1`.
3. **`request`**: only when neither exists. The hunk is computed from `old_string`/`new_string`, `content` (Write) or `new_source` (NotebookEdit). It has `old_start: null` and no context lines.

Every line's text is masked.

`ok` is `null` when:
- the record has no result; or
- the edit ran inside a Codex program that reported an error, since which of its calls failed is not stated.

### Command

```
{id, event_id, part, session_id, ts, command, readable, outcome: succeeded|failed|unknown, exit_code,
 output_excerpt: [≤ 40 trailing lines], output_lines_total, output_scope: command|program,
 is_test, is_commit, tally: {passed, failed, skipped, errors, unit: tests|packages, runner, lines} | null,
 tally_line, via: bash|codex_exec, version_id, raw_file, raw_line}
```

- **`outcome`**:
  - Claude Code and classic Codex calls take theirs from the result's `is_error`.
  - A command inside a Codex code-mode program (`via: codex_exec`) is `unknown` unless the program ran exactly that one command and its output states the exit code, or (for a test run) a runner summary.
- **`exit_code`** appears only when the result text states one.
- **`output_scope: program`** means the excerpt is the whole program's output.
- **`is_test`** follows the rule in `hq/commands.rs`: `identity::is_test_invocation`'s runners plus `EXTRA_TEST_RUNNERS`, read through `cd`, `A=b`, `timeout N` and the like.
- **`tally`** is parsed from the full result text of a test run, never from the excerpt. Summaries are summed across lines, and `tally_line` is the last one, masked. The shapes read:

  | runner | line read |
  |---|---|
  | cargo | `test result: ok. 12 passed; 0 failed; 1 ignored` |
  | pytest | `4 passed, 1 failed in 0.3s` |
  | jest | `Tests: 3 passed, 1 failed, 4 total` |
  | vitest | `Tests  3 passed (3)` |
  | node --test / TAP | `# pass N`, `# fail N` |
  | go | `ok`/`FAIL`/`?` per package; `unit: packages` |

  `null` when no summary is stated.

### Claim

```
{id, event_id, session_id, ts, text, kind: tests_pass|builds|fixed|done|other, has_verifier,
 marker, rests_on_command_id, followed_by_passing_test, verdict, verdict_basis, version_id, raw_file, raw_line,
 objections?: [{id, actor, at, reason, matches_sentence}]}   // additive (C30 R-LEDGER), absent when nobody objected
```

`verdict`/`verdict_basis` follow the rule under the statements route below.

- **`text`** is the claim's sentence, masked.
- **`kind`** is decided by phrase; the first match wins:
  1. `tests_pass`: a ratio marker, or the sentence names tests and passing — and does not report a failure;
  2. `builds`: it names a build, compile, clippy or lint and clean or success;
  3. `fixed`: it says fixed or resolved;
  4. `done`: a completion marker, or done, complete, implemented, added, shipped, merged or deployed;
  5. `other`.
- **`has_verifier`** is true for `tests_pass` and `builds`.
- **`rests_on_command_id`** is the latest test command before the claim in the thread.

### Version

```
{id, commit, short, session_id, from_seq, to_seq, committed, message, boundary_event_id, committed_at}
```

- A version is the code between two commit boundaries in one session. A boundary is a `git commit` whose result shows a hash or that succeeded.
- **`id`** is the 7-character hash when the result shows one (`[main 3f9a1c2] msg`). Otherwise it is `wt-<16 hex of sha256(session_id:from_seq)>`.
- **`committed: false`** marks the work after a session's last commit.
- Every edit, command, prompt and claim carries the `version_id` current at its row, in its own session's sequence.

Example (shortened; a Codex thread from the scratch store):

```json
{
 "thread": {"id": "1825e12fe11838fd", "title": "Can you take over claude code from here?", "whose_move": "you", "…": "…"},
 "prompts": [{"event_id": "event_5f95…", "session_id": "session_55b1…", "seq": 17, "ts": "2026-10-06T20:04:53.850Z",
   "text": "Can you take over claude code from here? \n", "work": true, "version_id": "wt-3cbba2230b08a2bb",
   "raw_file": "/…/mirrors/…/payload.part", "raw_line": 12}],
 "edits": [{"id": "event_0432…#0", "event_id": "event_0432…", "ts": "2026-10-06T20:08:48.089Z",
   "file": "/Users/…/thruline-v011/crates/cli/tests/licence_as_shipped_cli.rs", "op": "patch", "action": "update",
   "hunks": [{"old_start": null, "old_lines": 2, "new_start": null, "new_lines": 34, "header": null,
     "lines": [{"kind": "context", "text": "    assert_eq!(v[\"technical\"][\"keys_built_in\"], 1, \"{v}\");"}, {"kind": "context", "text": "}"}, "…"]}],
   "lines_added": 32, "lines_removed": 0, "ok": true, "source": "apply_patch", "via": "codex_exec", "version_id": "wt-3cbba2230b08a2bb"}],
 "commands": [{"id": "event_a00f…#0", "part": 0, "command": "node --test src/workspace/hq-copy.test.js 2>&1 | rg '^not ok|^# (tests|pass|fail)'",
   "readable": true, "outcome": "succeeded", "exit_code": null, "output_excerpt": ["Script completed", "…"], "output_lines_total": 6,
   "output_scope": "command", "is_test": true, "is_commit": false,
   "tally": {"passed": 49, "failed": 0, "skipped": 0, "errors": 0, "unit": "tests", "runner": "tap", "lines": 2},
   "tally_line": "# fail 0", "via": "codex_exec", "version_id": "wt-8e444bf764262345"}],
 "claims": [{"id": "event_98da…#0", "text": "The focused C20 run is red as expected: **31 tests, 24 passed, 7 failed**.",
   "kind": "done", "has_verifier": false, "marker": "completion",
   "rests_on_command_id": "event_a00f…#0", "followed_by_passing_test": true, "version_id": "wt-8e444bf764262345"}],
 "versions": [{"id": "wt-3cbba2230b08a2bb", "commit": null, "short": null, "session_id": "session_55b1…", "from_seq": 10, "to_seq": 679,
   "committed": true, "message": "test: pin no-trial Pro-only approval for v0.1.1", "boundary_event_id": "event_65a2…", "committed_at": "2026-10-06T20:13:03.953Z"}],
 "current_version_id": "wt-17173bf4c130df1c",
 "sessions": [{"id": "session_55b1…", "ledger_project_id": "project_d807…", "source": "codex", "origin": "person", "…": "…"}],
 "files": [{"path": "/Users/…/thruline-v011/.github/workflows/release.yml", "edits": 2, "lines_added": 9, "lines_removed": 11}],
 "masking": {"masked": 50, "families": ["email", "high_entropy"], "complete": true}
}
```

## `GET /api/hq/projects/<id>/statements` → `Statements`

One row per claim and no text, for screens that count across threads (Record-Over-Time). It is derived from the same cached index as the project view, so a warm read does not touch the database.

```
{schema_version: 1, project: {id, name},
 claims: [{id, thread_id, ts, agent: claude_code|codex, session_id, kind, has_verifier,
           verdict: proven|not_proven|contradicted|recorded, verdict_basis,
           proven_at, run_before, proof_moved}],
 sessions: [{id, source, started_at, last_at}],
 masking}
```

- **Order.** Claims are in recorded order.
- **`proven_at`** (C30 Record over time) is the recorded instant of the test run that proves the claim: the run it rests on for `run_holds`, the later passing run for `settled_by_later_run`. `null` for every other verdict, and for a proving run whose row has no instant. It is the instant of the row that started the run, so it can be before `ts` (a claim backed by an earlier run).
- **`run_before`** is true when a test run precedes the claim in its thread. Only a claim with a verifier is asked; it is `false` for any other claim and for a claim in no thread.
- **`proof_moved`** is true when the run the claim rests on passed and the code has moved since: an edit came after that run, or the thread now stands on a later version (④ SINCE of the proof chain). Every `edited_since_run` and `run_on_older_version` claim has it, and so does a claim a later run then proved again or failed; no other not-proven claim does. Added to the existing schema 1 as three additive fields; they read the same steps as the verdict, with no new rule.
- **`thread_id`** is `null` for a claim in no thread. Such a claim is `recorded` without a verifier, else `not_proven` (`not_in_a_thread`).
- **`sessions`** holds every recording, so a day with no session for an agent can be told apart from a day with no claim.

### The verdict rule

The rule is the one `ui/src/hq/data/change.js` `proofOf` applies, restated in `crates/core/src/hq/statements.rs`. The same `verdict`/`verdict_basis` are also served on every claim of the thread view.

Every edit, command and claim of the thread has a step: its place when sorted by recorded instant, then edit < command < claim, then its order in the thread view. A test run passes when it succeeded and its tally names no failure; it fails when it failed or its tally names one. The current version is the version of the thread's last row.

| verdict_basis | verdict | when |
|---|---|---|
| `no_verifier` | recorded | the kind is not `tests_pass`/`builds` |
| `later_run_failed` | contradicted | the last test run after the claim that has a result failed |
| `settled_by_later_run` | proven | a passing test run on the current version after the last edit, other than the run the claim rests on |
| `no_run_before` | not_proven | no test run before the claim |
| `run_failed` | contradicted | the run it rests on failed |
| `run_has_no_result` | not_proven | the run it rests on has no result |
| `run_holds` | proven | the run passed, no edit since, on the current version |
| `edited_since_run` | not_proven | the run passed and an edit came after it |
| `run_on_older_version` | not_proven | the run passed on an earlier version |
| `not_in_a_thread` | not_proven | a verifiable claim outside every thread |

The rows are tested first-match top to bottom, in that order.

Example (scratch store): 7,142 claims, 539 sessions, 2.2 MB.
- **Verdicts:** recorded 5,531, not_proven 1,300, contradicted 173, proven 138.
- **Timing:** 0.04–0.08 s warm.

```json
{"id": "01M49HR997YBJD21H4N8HNDH71#0", "thread_id": "23e8932119bc05b6", "ts": "2026-09-29T16:42:20.130Z",
 "agent": "claude_code", "session_id": "…", "kind": "tests_pass", "has_verifier": true,
 "verdict": "contradicted", "verdict_basis": "later_run_failed"}
```

## `GET /api/hq/sessions/<sid>` → `SessionTimeline`

```
{schema_version, session_id, ledger_project_id, hq_project_id, source, origin: person|subagent, file,
 items: [{event_id, seq, ts, kind, text, segments?, tool, sidechain, raw_file, raw_line}], masking}
```

- **Rows.** Every row of the session except `meta`, in order.
- **`kind`**:
  - `prompt` — the person's;
  - `agent_prompt` — a sub-agent's or sidechain "user";
  - `injected` — a vendor envelope;
  - `assistant`, `thinking`, `summary`;
  - `tool` — the call, with its result attached;
  - `tool_result` — only for a result whose call is not in the session.
- **`segments`** is present on `prompt` and `agent_prompt` items, with the same shape as on the thread view.
- **`tool`** is `{name, source_name, summary, outcome, exit_code, output_excerpt, output_lines_total}`. `summary` holds, by tool:
  - the command(s) for `Bash` and Codex programs;
  - `file (+a −r)` for edits;
  - the masked input, ≤ 2,000 characters, for any other tool.

Example item (shortened):

```json
{"event_id": "event_870f…", "seq": 23, "ts": "2026-10-06T20:04:58.845Z", "kind": "tool", "text": null,
 "tool": {"name": "spawn_agent", "source_name": null, "summary": ["{\"fork_turns\":\"all\",\"message\":\"[masked: high_entropy]\",\"task_name\":\"repo_audit\"}"],
          "outcome": "succeeded", "exit_code": null, "output_excerpt": ["{\"task_name\":\"/root/repo_audit\"}"], "output_lines_total": 1},
 "sidechain": false, "raw_file": "/…/payload.part", "raw_line": 18}
```

**Caching (C27 (c)).** The server keeps each thread view it serves as serialized bytes, keyed by project, thread, the project's index fingerprint (its members' heads and session counts) and `Index::thread_active_at`. That last value is the only part of a view that depends on the clock. A warm read is therefore a copy of the same bytes a fresh build would produce. Anything recorded in the project changes the fingerprint, so the next read is rebuilt. The cache holds up to 256 MiB and drops the oldest entry first. Tests: `crates/cli/tests/hq_routes.rs::a_cached_thread_view_is_the_fresh_one_and_never_outlives_a_change` and `crates/core/tests/hq_thread_cache_key.rs`. The project view (`GET /api/hq/projects/<id>`) is cached the same way, keyed by project, index fingerprint and `Index::active_threads_at` (the ids of the threads active at that instant, the only part of a project view that depends on the clock), in a separate cache of up to 64 MiB; tests: `crates/cli/tests/hq_routes.rs::a_cached_project_view_is_the_fresh_one_and_never_outlives_a_change`, `crates/cli/src/uiserve/hq.rs` `tests`, and `crates/core/tests/hq_project_cache_key.rs`.

## `POST /api/hq/projects/<id>/load` → 202 `LoadStatus`

- **What it reads.** Every recording on this machine that belongs to the project, through the existing ingest (`ingest::ingest`, `ingest::ingest_codex`), one file at a time, in the background. The files are found by `hq::load_plan`:
  - Claude Code: folders named for the root, or for the root plus `-…`, whose transcripts state a `cwd` inside it. This takes in worktrees and subdirectories. Only top-level `*.jsonl` files are read.
  - Codex: every `sessions/**` and `archived_sessions/**` file whose `session_meta.cwd` is inside the root.
- **What it accepts.** Same-origin is required. The request takes no body (any body → 400), and no path or parameter comes from the browser.
- **Idempotent.** Ingest appends only lines it has not seen. A second run adds only what was recorded since: on the scratch store it added 141 rows, all appended to two live transcripts.
- **When it refuses.** 409 while a load runs. 507 when the files the ledger has never seen would need more than the free space minus 2 GiB, counting 1.5× their size (A131).
- **Mirror.** Sealed copies go to `$THRULINE_HOME/mirrors` when that is set, else `~/.thruline/mirrors` (the ingest default).
- **Projections.** No projection is rebuilt.
- **What it skips (C27).** Before reading anything, each file is checked by `stat` alone against the seal of its session's accepted generation (size, modification time to the nanosecond, device, inode) and, for Claude Code, the session row's line count and line digest. A Codex file is also required to resolve to the owner its session was recorded under, from its path alone. A file that matches is not opened, sealed or parsed. A file whose `stat` moved but whose size did not (macOS renumbers a volume's device at boot; a tool touches a file's time) is read once and compared, and is skipped only when its bytes are exactly the accepted generation's and a full read would add nothing (`LoadMirror::unchanged_how`, `crates/core/tests/hq_load_content_skip.rs`). Both count in `files_unchanged`, which therefore means "not read into the ledger again", not always "not opened". A file that is gone counts in `files_gone`. Both are also counted in `files_done`. A file that grew is sealed in full and read as before (C26). The rule and its tests: `crates/core/src/ingest.rs` (`LoadMirror::unchanged`), `crates/core/tests/hq_load_speed.rs`.
- **Order (C27).** Newest modification first, so the newest threads appear first. Each file commits on its own, so a project's heads move, and the index the routes cache is rebuilt, file by file during the load.
- **Threads (C27).** The reading (capture, framing, parse, Codex mapping) runs on up to four worker threads; every ledger write runs on one, in the order above. The rows written are the rows a one-file-at-a-time load writes (`a_parallel_load_writes_the_same_rows_as_a_sequential_one`).

## `GET /api/hq/load` → `LoadStatus`

```json
{"schema_version": 1, "running": false, "project": "dfbfd6089e4bb388", "files_total": 540, "files_done": 540,
 "claude_code_files": 114, "codex_files": 426, "bytes_total": 2983597506, "events_added": 668444, "sessions_read": 540,
 "store_busy": 0, "errors": ["rollout-2026-09-27T00-12-17-…jsonl: 1 recording generation(s) not accepted (source_diverged)"],
 "started_at": "2026-10-06T21:26:06Z", "finished_at": "2026-10-06T21:36:38Z"}
```

C27 adds `files_unchanged` and `files_gone` (additive; see "What it skips").

C47 adds `store_held_refusals`: a saturating in-memory count of hold refusals in this HQ process. It remains visible after the hold is released and resets when HQ restarts. This field does not enable removal.

`errors` holds what ingest refused, by file name, masked:
- lines that changed after they were read;
- generations that were not accepted, such as a live Codex file that changed while it was being sealed (`source_diverged`).

Every refused file is still counted in `files_done`.

The load waits up to 30 s for another writer, such as a Claude Code hook. A file still blocked after that wait counts in `store_busy`, and its `errors` entry is `<file>: Another ThruLine process was writing to the record (a Claude Code hook or a terminal command), so nothing was changed. Try again in a moment.`

## `GET /api/hq/projects/<id>/capture` → `CaptureHealth` (C30 R-ROUTES "Capture health")

Is this project being recorded, from facts on this machine only. It feeds First run step 3, the left-nav Capture card, "Try again", "Look again" and the Settings system bar. Route: `crates/cli/src/uiserve/capture_health.rs`. **Read-only**: it writes nothing, appends nothing and adds no DDL; reading it again is how "Look again" and "Try again" re-check (`crates/cli/tests/hq_capture_health.rs` holds the ledger's bytes identical across requests). Only `GET` (405 otherwise); a query is 400 `unknown_query_parameter`; an unknown project is 404 `no_such_project`; a ledger that cannot be opened is 503 `store_unavailable`.

```json
{ "schema_version": 1, "checked_at": "2026-10-07T16:44:30Z",
  "project": {"id": "dfbfd6089e4bb388", "root": "/Users/me/src/app"},
  "claude_code_hooks": {
    "installed": true, "installed_reason": null,
    "alive": true, "alive_reason": null,
    "events": ["PostToolUse", "PreCompact", "PreToolUse", "SessionEnd", "SessionStart", "Stop", "StopFailure", "UserPromptSubmit"],
    "missing_programs": 0,
    "version": "0.1.0", "version_reason": null,
    "levels": [
      {"scope": "user", "path": "/Users/me/.claude/settings.json", "state": "read", "reason": null,
       "commands": [{"event": "Stop", "command": "/opt/homebrew/bin/thruline sensor append Stop", "kind": "recorder",
                     "program_exists": true, "missing_program": null, "this_program": true}, …]},
      {"scope": "project", "path": "/Users/me/src/app/.claude/settings.json", "state": "absent", "reason": null, "commands": []},
      {"scope": "project-local", …}, {"scope": "managed", …} ] },
  "codex": {"configured": false, "state": null, "paused": null, "watches_this_project": null,
            "reason": "HQ was not started with an observation source"},
  "git": {"repository": true, "toplevel": "/Users/me/src/app", "reason": null},
  "record": {
    "sessions": {"claude_code": 12, "codex": 0},
    "first_step": {"at": "2026-09-30T08:01:12.000Z", "agent": "claude_code"},
    "latest_step": {"at": "2026-10-07T16:40:03.000Z", "agent": "claude_code"},
    "through_hooks": {"deliveries": 4120, "first_at": "2026-09-30T08:01:12Z", "latest_at": "2026-10-07T16:40:04Z", "latest_event": "PostToolUse"},
    "reason": null },
  "masking": {"masked": 0, "families": [], "complete": true} }
```

The values above are illustrative; `events` is the `sensor install` set (`sensor::install_set()`), and `version` is the binary's `CARGO_PKG_VERSION` (the workspace version, `0.1.0` at this commit).

**The rule for every field: a fact that could not be read is `null`, with a sentence in the `…reason` beside it. Never a guess** (C30 G3: no "Installed" or "Found" without the file or the record behind it).

**`claude_code_hooks`** — the settings files Claude Code merges for this project, in its order: `user` (`$HOME/.claude/settings.json`), `project` and `project-local` (`<root>/.claude/settings.json`, `settings.local.json`), `managed` and each `managed drop-in`. They are read by the same function as `thruline doctor`'s hooks check (`doctor::read_hook_levels`, over `doctor::merged_settings_paths_for(<root>)`), and each command's program is resolved the way `doctor` resolves it (`doctor::resolve_program`). So this route and `thruline doctor` run in the project folder cannot disagree about what is installed or whether it can run.
- A **ThruLine command** is any hook command containing `thruline`. A **recorder** is one that appends capture rows (`sensor append`, `hook claude-code`, `hook codex`, `hook gemini`); speakers, the Stop gate and anything else are `kind: "other"`.
- `installed`: `true` when a recorder is on at least one event; `false` when every level was read (or is absent) and none holds a recorder; `null` when none was found and a level could not be read (`unreadable`, with the OS's reason, or `not_json`).
- `alive`: `false` as soon as any ThruLine command names a program that is not a file on this machine (`missing_programs` counts them; the reason says whether a recorder is among them). `true` when every one resolves and every level was read. `null` when nothing is installed or a level could not be read.
- `events`: the events that carry a recorder, sorted, each once.
- `version`: there is no version in a hook command, so this is this build's version **only when every recorder names the very binary serving the page** (`this_program`, by canonical path). A recorder that names another ThruLine program gives `null`, with the reason that its version is not read without running it. The route never runs a program named in a settings file.
- `levels[].state`: `read` | `absent` | `unreadable` | `not_json`.

**`codex`** — what this server's observer says, and nothing more: `configured` (HQ started with `--observe-codex`), the observer's own `state` word (`starting`, `watching`, `paused`, `disconnected`, `stopped`), `paused` (`state == "paused"`), and `watches_this_project` (the observer's folder and this root resolve to the same path). With no observer, the last three are `null` and `reason` says why. Codex's own hooks file is not read here.

**`git`** — one `git -C <root> rev-parse --show-toplevel`, through `bounded_child::output_until` with a 5 s deadline, system and user git config refused (`git_isolation::isolate_config`), `GIT_DIR` and `GIT_WORK_TREE` removed. `repository: true` with git's `toplevel` when git answers; `false` when git says "not a git repository" (so a folder with an empty `.git` reads `false`); `null` with a reason when the folder is not on this machine, git is not installed, git did not answer in time, or git failed for another reason. A root inside a larger repository reads `true`, and `toplevel` names that repository.

**`record`** — from the ledger, read through the same open as every other HQ read:
- `sessions`: the project's sessions, by agent (each member's `sessions` rows).
- `first_step` / `latest_step`: the earliest and latest recorded step (`MIN`/`MAX(events.ts)` over the members) and its agent; `null` when nothing is recorded.
- `through_hooks`: the deliveries Claude Code's hooks put in the ledger (`sensor_events`, the `thruline init` / `sensor install` pack) for this project — whose `cwd` is the root of the project or a member, or a folder inside one, or whose session the ledger files under a member. `deliveries: 0` means every step here came from reading the recordings (a load or `ingest`), not through the hooks. The `hook-install` capture pack writes to its own spool and store (`exec.db`), which this route does not read.
- `reason`: set, with every other field `null`, when the ledger could not be read.

**`holds`** (C30 R-LEDGER capture holds, additive; this document's `schema_version` stays 1) — this folder's pause and path exclusions as the record holds them, with the built-in defaults laid in: `{paused, paused_at, paused_by, exclusions[], honoured, exclusions_honoured, note}` and `holds_reason` beside it. Each exclusion is `{key, covers, note, built_in, on, source: "default"|"recorded", basis, since, by}`; `env` and `secrets` are `on` by default and `generated` is not, and a default no row names reads `source: "default"` with `basis: "on by default (not recorded)"` (`"off by default (not recorded)"` for `generated`) and no `since` or `by`. A typed path in force is listed after the three built-ins. **`honoured` and `exclusions_honoured` are both `true` (C34 Q4–Q6): a pause is honoured by the six capture callers, and a path exclusion replaces matching entries when a view is served or exported; `note` says both.** `paused_skips` (`{hook_deliveries, audits, last_at}`) sits beside `holds`, and is absent while no hook delivery or Stop audit has been left out for this folder (see the capture-holds section below). `holds` is `null`, with the reason in `holds_reason`, when the record could not be read. Masked; no folder is served (the project's root is `project.root`, once). Read-only like the rest of the document.

**Cost.** Each request re-reads the settings files and runs git once. The hook deliveries are tallied in one aggregate pass the first time a project is asked about, split by `rowid` across up to 16 read-only connections (`sensor_events` has no index on `cwd`, and none is added), and the tally is kept in server memory per store and project. A later request reads only the rows appended since, through the `seq` index, and starts over when a root, a member, or a member's session count changes. Measured on an APFS clone of the founder's store, served by `thruline ui`, GET only, at load 4–6: about 1.5 s for the first read after a server start with none of the ledger in memory, and 50–70 ms warm. The answer is byte-identical to a full recount (`hq_capture_health.rs` `the_delivery_tally_equals_the_exact_computation_after_every_change`).

## `GET /api/hq/projects/<id>/search?q=<text>[&limit=<n>][&thread=<tid>]` → `RecordSearch` (C30 R-ROUTES "Record search")

What ⌘K's Find rows find, across every Change (thread) of one project, the `Read` and `Version` kinds included (census §3.08 row 4). Engine: `crates/core/src/hq/search.rs` (`Corpus`). Route: `crates/cli/src/uiserve/hq/search.rs`. Reader: `ui/src/hq/data/search.js`. **Read-only**: it writes nothing, appends nothing and adds no DDL (`crates/cli/tests/hq_search_route.rs` holds the ledger's bytes, and its `-wal`, identical across searches). Only `GET` (405 otherwise).

**The one address under `/api/hq/` that takes a query string.** Every other one still refuses a query with 400 `unknown_query_parameter`, as before.

- `q` (required): what the person typed, percent-encoded; `+` is a space. 1–200 characters, not only spaces. 400 `missing_query` when absent or blank; 400 `query_too_long` over 200.
- `limit`: how many hits to serve, 1–100, default 20. 400 `bad_limit` otherwise.
- `thread`: the Change open in the palette, by its thread id (ASCII letters and digits). Of that thread, only `Read` and `Version` hits are served: its prompts, edits, commands and statements are drawn by the palette from that thread's own view (`/threads/<tid>`), where each has the step every Change screen names. 404 `no_such_thread` when the project holds no such thread.
- A parameter given twice, an empty pair, or one that does not decode: 400 `malformed_request`. Any other parameter: 400 `unknown_query_parameter`. An unknown project: 404 `no_such_project`.

**What is searched**, per thread, from the same index the other routes read:

| `kind` | what | grouped by | `count` |
|---|---|---|---|
| `Request` | a person's prompt (the index's text: the first 8,000 characters) | one per prompt | 1 |
| `Said` | a statement the agent made (a claim) | one per claim | 1 |
| `File` | a file the agent edited | thread and path | edits |
| `Read` | a file the agent read: a Claude Code `Read` call's `file_path`, read from `events.tool_input` by primary key | thread and path | reads |
| `Run` | a test command | thread and command text | runs |
| `Command` | any other command | thread and command text | runs |
| `Version` | a version of the code (its id, commit and commit message are searched) | one per version, in the thread holding its commit, else the thread holding its latest row | 1 |

A copied row (a resumed transcript's history) is not a thread member, so nothing is found twice. A `Read` call whose input is not JSON or names no `file_path` is counted in `searched.reads_without_a_path`, never searched.

**The match rule** is the palette's own (`Command-Ask/logic.js`): the text is trimmed and lower-cased; its **words** are the runs of `[a-z0-9/._-]` of at least four characters that are not stop words (`what which when where were there that this with from have does about should would could anything something after before into your they them then than been being will just some ever`). A thing matches when its searched text, lower-cased, contains the whole text or any one word. **The searched text is the masked text**, so a secret the masker removed is never found by typing it.

Active path exclusions are applied to the corpus before matching. A matching title or entry is shown only as the fixed exclusion marker; typing its former path cannot find it. `left_out_by_exclusion` counts those entries, and is absent when none matched. The `searched` counts describe the original recorded corpus, including entries left out of presentation.

**Order:** the open thread's hits first, then newest first (the latest occurrence's instant), then by kind (`Request`, `Said`, `File`, `Read`, `Run`, `Command`, `Version`), then text.

```
{schema_version: 1, project_id, thread: <tid>|null, limit,
 total,                       // everything that matched
 by_kind: {Request, Said, File, Read, Run, Command, Version},   // total, by kind; every kind present
 results: [Hit],              // the first `limit` of it
 searched: {requests, statements, files, reads, commands, versions, reads_without_a_path},
 masking: {masked, families, complete},   // every string served
 left_out_by_exclusion?: N}               // present only when N > 0

Hit {kind, thread_id, thread_title, open_thread, text, at, event_id, count, source,
     work,                    // Request: the prompt's turn did work
     outcome, exit_code, tally,   // Run / Command: how the latest ended; tally {passed, failed, skipped, errors} for a Run
     verdict, claim_kind,     // Said: the statements route's verdict and kind
     version}                 // Version: {id, short, commit, committed, message, current}
```

- Every field is present on every hit; a field that does not belong to its kind is `null`. `exit_code` is set only when the output states one; `tally` only when the runner printed its summary.
- `thread_title` is the Inbox's own title for that thread (the C26 rule, `Index::title_of`), masked; `""` when its prompts hold no sentence.
- `text` is masked; a text longer than 160 characters is cut to a window around the first match, never through a `[masked: …]` marker, with `…` where it was cut.
- `event_id` is the latest occurrence's record (`events.id`; `<event_id>#<n>` for a statement). A version's `id`, `short` and `commit` are identifiers, served as the thread view serves them.
- `current`: the version of its thread's last row.

**Where each hit opens** (the palette, `Command-Ask/live.js` `searchHitRow`): `Request`, `Read`, `Run`, `Command` and `Version` on the thread's Brief (`#/changes/<tid>`); `File` on What changed (`/changed`); `Said` on Every statement (`/statements`) when its verdict is `proven`, `not_proven` or `contradicted`, else the Brief. Never the Inbox in its place.

**Cost.** The route builds a search corpus from the cached index the first time a project is searched and keeps the latest one in server memory, keyed on the index fingerprint and active-exclusion fingerprint; a later search is one linear pass of `contains` over it, a sort of what matched, and a window for each hit served. The corpus is rebuilt when the record or active exclusions change. With `generated` enabled, each search rebuilds the corpus so a `.gitattributes` edit cannot leave stale visible results. Measured before exclusions on a synthetic store (never a real one) of 2,000 Claude Code sessions, 244,000 events and 2,000 threads in one project, release build, core only (`crates/core/tests/hq_search_probe.rs`, `--ignored`): index 1.03 s (the existing cost); corpus build 0.57–0.59 s, holding 135,465 things and 26.8 MB of text; a search 3–11 ms for words that match up to 17,000 things, 17–25 ms for a single letter that matches 87,000–118,000, with JSON under 11 KB at the default limit.

## `GET` / `POST /api/hq/projects/<id>/install` → `InstallPlan` / `InstallAnswer` (C30 R-ROUTES "Install")

First run's step 2: hooks, MCP, speaker and shim, exactly as the person ticked them. The click is the consent, and it runs only on that project. Route: `crates/cli/src/uiserve/install_route.rs`. Client: `ui/src/hq/screens/Onboarding/install.js`. Tests: `crates/cli/tests/hq_install_route.rs` (process level, on temporary HOMEs and a temporary git repository), `ui/src/hq/screens/Onboarding/install.test.js` and `install-live.test.js`.

**Which file each part writes, and nothing else.** Each part is written through the existing installer, never a second implementation:

| Part (body key) | File | Level | Entries, through |
|---|---|---|---|
| Claude Code hooks (`hooks`, always `true`) | `<project>/.claude/settings.local.json` | project-local | the `thruline sensor install` pack, one entry per `sensor::install_set()` event (`sensor_install_into`, the merge `thruline init` uses) |
| Speaker hooks (`speaker`) | the same file | project-local | `SessionStart brief`, `Stop audit`, `SessionEnd digest` (`init_speaker_install_into`) |
| MCP server (`mcp`) | `<project>/.mcp.json` | project | `mcpServers.thruline` → `<this binary> mcp --agent claude --db <this HQ's store>` (`init_mcp_install_into`) |
| Run capture (`run_capture`) | the shim folder in ThruLine's home (`$THRULINE_HOME/shim`, else `~/.thruline/shim`) | thruline-home | a wrapper for each `shim::WRAPPED` program on the search path, and its manifest, as `thruline shim install` run in the project's folder writes them (`shim_cmd::install_options`, `shim::install`) |

`.claude/settings.local.json`, not `.claude/settings.json`: every hook names this machine's ThruLine program by its absolute path, so it belongs in the personal, unshared level. **Never written:** `~/.claude/settings.json`, `~/.claude.json`, a project's shared `.claude/settings.json`, a shell profile, the ledger. The route appends nothing to the record and adds no DDL; the installers it calls append nothing either. Run capture records nothing until the person adds the line `path_line` names to their shell profile, which this route never writes. Claude Code asks once before it uses a server from `.mcp.json`.

### `GET` → `InstallPlan`

Writes nothing. Only `GET` and `POST` (405 otherwise); a query is 400 `unknown_query_parameter`; an unknown project is 404 `no_such_project`; a ledger that cannot be opened is 503 `store_unavailable`.

```json
{ "schema_version": 1,
  "project": {"id": "dfbfd6089e4bb388", "root": "/Users/me/src/app"},
  "plan": "<64 hex>",
  "parts": {
    "hooks":   {"state": "will_add", "file": ".claude/settings.local.json", "level": "project-local",
                "adds": ["SessionStart", "UserPromptSubmit", …], "where": [], "refusal": null},
    "speaker": {"state": "installed", "file": ".claude/settings.local.json", "level": "project-local",
                "adds": [], "where": ["project-local"], "refusal": null},
    "mcp":     {"state": "refused", "file": ".mcp.json", "level": "project", "adds": [], "where": [],
                "refusal": {"code": "name_taken", "plain": "A server named thruline is already in .mcp.json, and ThruLine did not write it, so it was left alone."}},
    "run_capture": {"state": "will_add", "file": "<ThruLine home>/shim", "level": "thruline-home",
                "adds": ["pytest", "cargo", …], "where": [], "refusal": null,
                "wrappers": [], "candidates": ["pytest", …], "path_line": "export PATH=\"$HOME/.thruline/shim:$PATH\"", "on_path": null} },
  "undo": {"project": "thruline init --undo --settings .claude/settings.local.json --claude-json .mcp.json",
           "user": null, "shim": null},
  "masking": {"masked": 0, "families": [], "complete": true} }
```

- `parts.*.state`: `will_add` (the click would add `adds`), `installed` (every entry is already in a file Claude Code reads for this folder; `where` names the levels: `user`, `project`, `project-local`, `managed`, `managed drop-in`, `local` for `~/.claude.json`'s per-project servers, or `thruline-home`), or `refused` (`refusal.code` and `refusal.plain`). A part already installed anywhere Claude Code reads for this folder is left as it is.
- `run_capture`: `adds` is the candidate list; a wrapper is written only for a program on the search path, as `shim install` does. `wrappers` names the wrappers of an intact shim; `on_path` is whether the shim folder is on the search path **this HQ was started with** (`null` until the shim is installed), which is not necessarily the agent's. `path_line` writes the home folder as `$HOME`.
- `undo.project` is set when the project's two files hold an entry `thruline init` writes; `undo.user` when the user's own settings or `~/.claude.json` do; `undo.shim` when the shim is installed. The screen's "Changed your mind?" copies them. The uninstall route below takes back only the project's speaker hooks; the terminal undo handles the other parts.
- `plan` digests what each part would do and the bytes of the two project files. The POST must carry it.

**Refusal codes.** `folder_missing` (the project's folder is not on this machine); `other_store` (hooks and speakers: they name no store and write to the default one, `~/.thruline/ledger.db`, when they fire, and this HQ reads another, so nothing they recorded would show here); `symlink` (the file, or `.claude`, or the shim folder, is a link: refused rather than written through or replaced); `not_a_file` / `not_a_folder`; `unreadable`; `not_json` (the file is not a JSON object); `unexpected_shape` (an installer refused its shape, e.g. `hooks` not an object); `unreadable_level` (hooks: another settings level Claude Code merges for this folder could not be read, so a second recorder cannot be ruled out); `another_recorder` (hooks: `hook_install::refuse_double_recorder_across`, over this project's levels, refuses the write because another ThruLine recorder is already on an event Claude Code merges here); `split` (speakers: some are in another level, and the rest here would split them across two files); `name_taken` (an `mcpServers.thruline` ThruLine did not write); `occupied` (the shim folder holds files ThruLine did not write); `no_home` (neither `THRULINE_HOME` nor `HOME` is set).

### `POST` → `InstallAnswer`

Body, `application/json`, at most 512 bytes, unknown keys refused: `{"hooks": true, "mcp": bool, "speaker": bool, "run_capture": bool, "plan": "<the plan>"}`. Every key is required.

Guards, before the body is read and before the store is opened, by the run route's own two functions (`run_route::origin_is_ours`, `run_route::token_is_ours`): exactly one `Origin`, this server's (403 `same_origin_required`); exactly one `X-ThruLine-Run-Token`, this launch's (403 `run_token_required`). A foreign `Origin` never reaches the route (the server-wide check answers 403).

- 400 `malformed_request` (framing), `invalid_install_request` (unknown key, missing key, wrong type), `hooks_required` (`hooks: false`: the design locks them on). 413 `request_too_large`.
- 409 `plan_changed`, with `plan` (the fresh `InstallPlan`): what the click would do is no longer what the page showed. Nothing is written.
- 409 `install_in_progress`: one install at a time in this HQ; nothing waits.
- 200 otherwise, with each part's outcome:

```json
{ "schema_version": 1, "project": {…}, "changed": true,
  "parts": {
    "hooks":   {"outcome": "wrote", "file": ".claude/settings.local.json", "entries": ["SessionStart", …], "where": ["project-local"], "refusal": null},
    "speaker": {"outcome": "not_chosen", …},
    "mcp":     {"outcome": "already", "file": ".mcp.json", "entries": [], "where": ["user"], "refusal": null},
    "run_capture": {"outcome": "refused", …, "refusal": {"code": "occupied", "plain": "…"}} },
  "plain": "Wrote .claude/settings.local.json.",
  "after": { …the InstallPlan read after writing… } }
```

`outcome`: `wrote`, `already` (nothing written for it), `refused`, `failed` (the write did not happen; `refusal.code` `write_failed` and the reason), or `not_chosen` (unticked: nothing written, and nothing that is there is removed). `changed` is whether any file was written. A second click with every chosen part in place writes nothing, answers `changed: false`, every chosen part `already`, and `plain` "Nothing changed: every part you chose was already there."

**How each file is written.** Hooks and speakers go to one file in one write. The JSON is the installers' own merge of what was read, re-checked against the plan's bytes just before the write, written to a new file beside it (created exclusively), flushed, given the old file's permissions, and renamed over it; `.claude` is created when absent and refused when it is a link. Pretty JSON and a final newline, as `thruline init` writes. The shim is written by `shim::install` itself.

**The terminal doors.** The same files from a terminal in the project's folder: `thruline init --settings .claude/settings.local.json --claude-json .mcp.json` (hooks, MCP and, asked, the speakers) and `thruline shim install`. The reversal: `thruline init --undo --settings .claude/settings.local.json --claude-json .mcp.json` and `thruline shim uninstall` (`hq_install_route.rs` `the_terminal_undo_it_names_removes_exactly_what_it_wrote` runs the first).

### `POST /api/hq/projects/<id>/uninstall` → `UninstallAnswer` (C46 §1.2(2), write address 30)

Settings' speaker switch, Off, takes ThruLine's three speaker hooks out of `<project>/.claude/settings.local.json`, the one file the install writes them to, through `thruline init`'s own remover (`init_speaker_uninstall_from`), and changes nothing else. On is the install POST above with only `speaker` ticked. Same route file, same guards (one same-origin `Origin`, this launch's `X-ThruLine-Run-Token`, both before the body is read), the same one-at-a-time lock (`install_in_progress`) and the same `plan` (the install GET's digest; `plan_changed` with the fresh plan when it no longer is). Only `POST` (405 otherwise); a query is 400.

Body, `application/json`, at most 512 bytes, unknown keys refused: `{"part": "speaker", "plan": "<the plan>"}`. `part` other than `speaker` is 400 `unknown_part`; an unknown or missing key 400 `invalid_uninstall_request`.

- 409 `elsewhere`, `{error, where: ["user", …], plain}`: a speaker hook is in another file Claude Code reads for this folder (the user's `~/.claude/settings.json`, a shared `.claude/settings.json`, managed settings). This route never changes those, so the lines stay on; `plain` says where and to take them off in a terminal. Nothing is written, not even the project's own file.
- 409 `not_installed`, `{error, plain}`: none of ThruLine's speaker hooks is in the project's file. Nothing changed.
- 409 `refused`, `{error, refusal: {code, plain}, plain}`: the project's file cannot be used (a link, not JSON, unreadable), or another Claude Code settings level cannot be read, so Off cannot promise the lines will stay off. Nothing changed.
- 500 `write_failed`, `{error, plain}`: the file was not written, so the lines stay on.
- 200: `{"schema_version": 1, "project": {…}, "changed": true, "part": "speaker", "file": ".claude/settings.local.json", "removed": ["SessionStart brief --hook session-start", "Stop audit --hook stop", "SessionEnd digest precompute --hook session-end"], "plain": "Took ThruLine’s lines out of .claude/settings.local.json. Sessions in this project start without them from their next start.", "after": { …the InstallPlan read after writing… }}`. `removed` names the entries that were there.

The file is written as the install writes it (re-checked against the plan's bytes, a new file renamed over it, never through a link). Appends nothing to the record and adds no DDL. Free. Tests: `hq_install_route.rs` `speaker_off_takes_out_exactly_the_three_speaker_hooks_and_nothing_else`, `speaker_off_needs_this_page_its_token_the_plan_and_exactly_its_body`, `speaker_hooks_turned_on_at_another_level_are_never_touched`; client `Settings/settings-speaker.test.js`.

### `GET /api/hq/projects/<id>/run-permissions` (C46 Settings)

Read-only, schema version 1: `{schema_version, project, folders}`. Each folder row is `{id, folder, state, actor, at, change_id, latest_row, action, historical}`. The `id` is a server-derived opaque identity for an exact recorded worktree folder; `folder` is a project-relative display line, `Project folder`, an opaque external-worktree label, or the active exclusion marker. The raw path is never served. Rows are ordered by exact recorded folder. When several Changes name one folder, the first Change by id is the representative. An old permission folder with no usable current Change remains visible.

`state` is `given`, `revoked`, `never`, `file_missing` or `not_in_record`, determined from the latest permission row and the sealed file together. `actor` and `at` are non-null only for an effective `given`; `latest_row` remains available to guard a later act in every recorded state. `action` is `revoke`, `grant` or null. A historical folder can offer `revoke` for a standing grant but never `grant`; the first check in a never-asked folder still asks through the Check sheet. The read selects only this HQ project's ledger members and their Changes; it does not build the HQ index or run a check. Active path exclusions are applied to the raw folder before making any display line. An unreadable exclusion policy refuses the read with a fixed 503. Other methods are 405, a query is 400, and an unknown project is 404.

### Settings permission writes (C46)

The existing Change addresses `POST /api/changes/<id>/run/permission` and `…/revoke` accept the Check sheet's `{}` unchanged. Settings sends `{expected_folder: "<opaque id>", expected_row: "<latest row id>" | null}`. Under the permission-act lock, the server resolves the Change's current recorded worktree again. A moved folder or permission row answers 409 `{error: "permission_target_changed", plain: "This permission changed in the record. Read it again."}` before appending a row or touching a file. The latest-row comparison is repeated inside the append transaction. A current folder or any ancestor replaced by a symlink refuses before the append.

`POST /api/hq/projects/<id>/run-permissions/<folder>/revoke` is write address 31 for a standing historical grant only. The browser sends `{expected_row: "<latest row id>"}` and the opaque folder id in the URL, never a filesystem path. The server resolves both against that HQ project's ledger members, refuses a current Change binding and a moved row, and appends one `revoked` row before removing the sealed cache through pinned directory handles opened without following symlinks. A removed historical worktree has no cache and can still have its standing grant taken back. There is no historical grant route. The existing same-origin and per-launch run-token guards apply before the body is read; unknown targets are 404, stale or linked targets 409, a query 400, another method 405, and a malformed body 400. No check is started, earlier runs are untouched, and no DDL or paid surface is added.

## `GET /api/first-use` (additive fields, C21-d)

- `excluded_temp_projects`: as above.
- `diagnostics: [{code, count, sentence}]`: the same facts as `notes[]`, typed, each with a sentence for a person, for example:
  - `{"code":"codex_not_scanned","count":533,"sentence":"533 Codex recordings were not opened: this screen reads the newest 120."}`
  - `{"code":"temp_projects_excluded","count":1,"sentence":"1 folder recorded in a temporary folder is not listed: scratch work is not a project."}`

  A code this server does not know has `sentence: null`; no sentence is invented for it.
- `notes[]` keeps its machine form (`code=count`) and gains `temp_projects_excluded=N`.

## `POST /api/first-use/change` with a thread (C26 ruling (b))

The plain form `{project}` is unchanged: `project` is a ledger project id, and the Change confirms the project's newest recorded work.

With a thread, same-origin as every write:

```json
{ "project": "<HQ project id (a ledger project id is accepted too)>", "thread": "<HQ thread id>" }
```

The thread is found through the project's HQ index. The project-wide proposal derivation is not run. The Change:

- lives in the ledger project of the thread's first work prompt;
- has `request.prompt_event_id` = that prompt, which is the thread's identity;
- has `request.title` = the thread's served `title` (the C26 rule, the same text every screen shows);
- has every session of the thread in that ledger project confirmed (`basis: user`, actor `you`, reason "tracked from this thread").

The store allows a Change only the sessions of its own ledger project. So the sessions of a thread that spans two recording projects (Claude Code and Codex on one repository) cannot all be confirmed. The ones in the other project are listed in `not_confirmed` and not written.

No suggested links are recorded and nothing else is appended: no scope, no acceptance, no approval.

**200, written:**

```json
{"schema_version": 3, "change": "chg_01…", "already_tracked": false, "thread": "1825e12fe11838fd",
 "title": "…the thread's title…", "confirmed": 2, "confirmed_sessions": ["s1", "s2"],
 "not_confirmed": [{"session": "session_…", "ledger_project_id": "project_…", "reason": "other_recording_project"}],
 "suggested": 0, "basis": "user", "opened_at": "2026-10-07T…Z"}
```

**200, already tracked:** a Change in that ledger project already points at the thread's first work prompt. Nothing is written.

```json
{"schema_version": 3, "change": "chg_01…", "already_tracked": true, "thread": "1825e12fe11838fd"}
```

**Refusals:**

| Status | Body | When |
|---|---|---|
| 404 | `{"error":"no_such_project"}` | the project id matches no HQ project and no ledger project |
| 404 | `{"error":"no_such_thread","detail":"no thread with this id in this project"}` | the thread is unknown, or belongs to another project |
| 400 | `{"error":"refused","detail":…}` | the store refused a write |
| 503 | `store_busy` | another process held the record for 30 s |

## `GET` / `POST /api/preferences` (C30 R-PREFS)

A person's preferences. **They are not record (C30 R-LEDGER):** they live in one file, `preferences.json`, in the folder that holds the ledger this server opened. That is the folder `/api/storage` names, `~/.thruline/preferences.json` for the product. Neither method opens a ledger row or adds DDL. Route module: `crates/cli/src/uiserve/preferences.rs`. Write-address table row 15 in CLAUDE.md.

### The keys

The set is closed. The values and defaults are the Settings design's (`DESIGN-SPEC.md` §1.11, §2.9 and the §3.10 state table).

| Key | Values (default **bold**) | Settings draws it as |
|---|---|---|
| `face` | **`plain`** · `technical` | Plain / Technical (also the top-bar toggle and `t`) |
| `density` | **`comfortable`** · `compact` | Comfortable / Compact |
| `motion` | **`system`** · `reduce` | Follow this Mac / Always reduce |
| `colour_vision` | **`standard`** · `proven_teal` | "Proven is teal" off / on (§1.11) |
| `keys` | **`on`** · `mod` · `off` | Single-key shortcuts: On / Only with ⌥ / Off |
| `masked_copy` | **`true`** · `false` | When you export, also make a masked copy |
| `notifications` | `{ready, not_backed, agent_waiting, recording_stopped}`, each **`false`** · `true` | Four browser events from the open HQ page only (C51). The first switch-on asks browser permission. No permission prompt is itself an event. |
| `weekly_summary` | **`false`** · `true` | On the first HQ open of a local calendar week, show a dismissible card computed from the last seven days; Save as Markdown uses the existing browser export. |

### `GET /api/preferences` → 200

```json
{"schema_version": 1,
 "preferences": {"face": "technical", "density": "comfortable", "motion": "system", "colour_vision": "standard",
                 "keys": "on", "masked_copy": true,
                 "notifications": {"ready": false, "not_backed": true, "agent_waiting": false, "recording_stopped": false}, "weekly_summary": false},
 "defaults": {"face": "plain", "...": "every key at its default"},
 "path": "/Users/…/.thruline/preferences.json",
 "file": "read",
 "from_file": ["face", "notifications"],
 "unknown_keys": [],
 "problems": []}
```

- **Every key is always present** in `preferences`: the file's value where it was usable, otherwise the default. `from_file` names the keys whose value came from the file.
- **`file`** is one of:
  - `absent`: no file; everything is the default;
  - `read`;
  - `not_json`: not a JSON object;
  - `unrecognised`: no `schema_version` this version reads;
  - `newer_schema`: a newer ThruLine wrote it; everything is served as the default and a POST is refused, so the file is left as it is;
  - `not_a_regular_file`: a symlink or anything else. It is not followed, and everything is served as the default;
  - `unreadable`.
- **`problems`** lists every reason something in the file was not used, each `{problem, key?, detail?}`. `problem` is one of `not_json`, `no_schema_version`, `newer_schema`, `not_a_regular_file`, `unreadable`, `no_preferences_object`, `invalid_value` or `unknown_key` (an unknown notification switch). One bad value costs only its own key (or its own switch).
- **`unknown_keys`** names the keys the file holds that this version does not know. They are not served. They are kept on the next write, so a newer version's choices are not lost.
- **A query string** gets 400 `unknown_query_parameter`. A body gets 400 `malformed_request`.

### `POST /api/preferences` → 200, the document above, read back after the write

```json
{"face": "technical", "notifications": {"not_backed": true}}
```

- **The body** sets one or more keys. The rest keep their current value. `notifications` may name any subset of its four switches.
- **Same-origin.** Exactly one `Origin`, this server's own, is required. No `Origin` gets 403 `{"error":"same_origin_required"}`. A foreign `Origin` is refused by the server before the route sees it (403).
- **Framing.** `Content-Type: application/json` and one `Content-Length`, with a body of at most 1024 bytes. A larger body gets 413 `request_too_large`. Bad framing, or a body that is not a JSON object, gets 400 `malformed_request`. An empty object gets 400 `no_preferences`.
- **Refusals name the key**, and nothing is written:
  - 400 `{"error":"unknown_preference","key":"theme"}`, also used for an unknown switch: `"key":"notifications.perm"`;
  - 400 `{"error":"invalid_value","key":"face","allowed":["plain","technical"]}`.
- **The write.** The whole document, `{"schema_version":1,"preferences":{…}}`, goes to a new temporary file in the same folder. The temporary file is created exclusively, flushed, and renamed over `preferences.json`. Writes are serialised in the process.
- **Write refusals:**
  - 409 `preferences_file_not_regular`: `preferences.json` is a symlink or not a regular file. It is left as it is, and nothing is written through it or in its place;
  - 409 `preferences_file_newer`: a newer ThruLine wrote it;
  - 500 `write_failed` (with `detail` and `path`).
- **A corrupt file** (`not_json`, `unrecognised`) is replaced by the next successful POST.
- **No ledger.** Neither method opens the ledger. `crates/cli/tests/hq_preferences.rs` holds the ledger's bytes (`ledger.db`, `ledger.db-wal`) identical across every request it makes.

## `GET /api/hq/not-backed` and `GET /api/hq/weekly-summary` (C51)

Both are read-only and use an existing-schema, hold-checked read-only ledger connection. A held or unavailable store answers 503. Other methods answer 405. No claim text, project path, or session identifier is returned.

`GET /api/hq/not-backed` baselines the current maximum `speaker_findings.seq` (`-1` when empty) and returns `{schema_version:1, latest_seq, next_seq, finding_items:[]}`. `GET /api/hq/not-backed?after=<seq>` returns at most 256 later findings as `{id, recorded_at}` in sequence order, with `next_seq` equal to the last returned sequence (or latest when none). The open HQ page polls this indexed sequence every five seconds only while the Not backed switch is On. A malformed or other query is 400. The first read on each page open baselines existing findings; later items are delivered once per opaque ID. These are Stop-audit completion claims for which the record held no matching proof, not allegations that the claim was false. The other three notification event reads follow only the project open in HQ, so a five-second tick never sweeps every project on the machine.

`GET /api/hq/weekly-summary` has no query and returns `{schema_version:1, since, through, approvals, not_backed, rechecks, projects_recorded, finding_items}`. The UTC window is the preceding seven days at read time, inclusive. `approvals` counts `change_decisions` rows with `act=approved`; `not_backed` counts `speaker_findings`; `rechecks` counts distinct Change-associated run keys with a `running` execution row; `projects_recorded` counts distinct recorded root paths with an event in the window. `finding_items` holds only opaque IDs and times. The weekly card reads this aggregate once on its first eligible HQ open of the person's local Monday-to-Sunday week, and the existing browser export saves its Markdown. No background job, record row, or weekly digest file is written.

## `GET /api/changes/<id>/run` (additive fields, C30 R-RUN)

The run sheet (`crates/cli/src/uiserve/run_route.rs` `view`) is the existing P6-5 route, and every field it served before is unchanged. `schema_version` moves from 2 to 3 (`RUN_VIEW_SCHEMA_VERSION`) for four additive fields. HQ reads them through `ui/src/hq/data/run.js` `liveOutputOf`, every second while a check of this Change runs (`POLL_MS`).

On `job`, the check this HQ started and has not yet seen end:

| Field | What it is |
|---|---|
| `recorded_started_at` | The run's own recorded start: the earliest `started_at` the wrapper wrote for it. A check is usually two phases, ThruLine's dependency fetch and then the check, and each phase's `running` row carries its own start. So this is the earliest phase's `COMMAND_STARTED` `observed_at` (the same string as that phase's `running` row), or the latest row's `started_at` when it is earlier. `null` while the job is `starting` (nothing recorded yet) and for another Change's job. |
| `elapsed_s` | Whole seconds from `recorded_started_at` to now while the record says the run is going, and to the record's own end (`recorded_at` of its terminal row) once it has ended. `null` when `recorded_started_at` is. |
| `output` | `{lines, cut, masking}` — what the run has printed so far — for this Change's job only. Another Change's job (`for_this_change: false`) serves `null`, so its output is never drawn as this one's. |

`output`:

- **`lines`** are the newest finished lines of both streams together, oldest first. A line still being printed is not served until it ends (it may end in half a token, which a masker cannot recognise). A run that has printed nothing yet serves `[]`.
- **Bounded.** At most 100 lines (`check::LIVE_LINES`) and 16 KiB (`check::LIVE_BYTES`), the newest kept. No served line is longer than 1 KiB (`check::LIVE_LINE_BYTES`); a longer line is cut at a character, **after** the masking (the tail keeps up to 8 KiB of a line, `check::LIVE_TAIL_LINE_BYTES`, so a secret is whole when it is masked).
- **`cut`** is `true` when anything printed is not among `lines` as it was printed: an older line was dropped, or a line was cut.
- **As a terminal would leave each line.** Colour and other escape sequences are removed, along with control characters other than a tab. A line redrawn with `\r` shows its last drawing.
- **Masked.** The home folder is `~`, as the sheet's folders are. The lines then go through `redact::mask_recorded` in one pass, so a key printed across lines (an armoured block) is masked whole. `masking: {masked, families, complete}` is that pass's record, as on every HQ document.
- **Not the record.** The tail is kept in memory, beside the job, by the wrapper's own tee (`check::LiveOutput`). The tee hashes and counts exactly the bytes it did before. A detached run's streams used to go nowhere and now go to this tail. The record still holds each stream as a digest and a length, and `crates/cli/src/check.rs` `a_run_with_the_tail_listening_records_exactly_what_it_records_without` holds both identical with the tail and without it. Nothing writes the tail anywhere, and it goes when the job goes: once the run ends and the job is removed, the sheet serves no output. A check started from a terminal prints to that terminal and has no job here, so the sheet serves no output for it.

On `checks[].last`:

| Field | What it is |
|---|---|
| `elapsed_s` | As on `job`, for the run the record names: whole seconds from its recorded start to now while it runs, and to its recorded end once it has ended. A check started from a terminal has one too. |

**Writes nothing.** Reading the sheet while a check runs appends no row (`run_route_guards.rs` `reading_the_sheet_while_a_check_runs_writes_nothing`). No DDL.

**pytest's count.** `checks[].last.tests_passed` / `tests_failed` / `suites` are the typed count the wrapper records (`check_run_tallies`). Since C30 R-RUN the wrapper also reads pytest's own session summary line (`check::parse_pytest_summary`), not only libtest's `test result:` lines. The last summary in a stream is the run's count:

- `passed` → passed.
- `failed`, `error` and `errors` → failed.
- `skipped` → ignored.
- `deselected`, `xfailed`, `xpassed`, `warning(s)`, `rerun` and `subtests …` are counted nowhere.
- `no tests ran` is zero and zero.
- A stream with no summary stores no count, never zero.

The sheet can start only `cargo-test` (`check::RUNNABLE_CHECKS`), and `checks[].last` is the latest run of a check the sheet can start. So a pytest count appears here only for such a run. A pytest run from a terminal (`thruline check --for-change <id> -- pytest`) is counted the same way, and it reaches the Change's coverage rather than this field.

## `GET /api/changes/<id>/pr` → `PrPreview` (C30 R-PR)

What the pull-request preview (PR-Face, `#/changes/<id>/pr`) needs from this machine that no other route serves, for one **tracked** Change: the branch and head of the folder ThruLine recorded for it, the receipts the ThruLine Action would verify there, and the CLI's own check-run name and badge words. Route: `crates/cli/src/uiserve/pr_preview.rs`; test `crates/cli/tests/hq_pr_preview.rs`. **Read-only**: it writes nothing, appends nothing and adds no DDL; across repeated requests every table's row count, and the repository's index, refs, object count and status, are unchanged. Only `GET` (405 `method_not_allowed` otherwise); a query is 400 `unknown_query_parameter`; a Change the store does not hold is 404 `no_such_change`; a ledger that cannot be opened is 503 `store_unavailable` (with `detail`, e.g. a pending schema upgrade). Matched before the one-Change route, which would otherwise answer it 404.

```json
{ "schema_version": 1, "read_at": "2026-10-08T09:12:03Z",
  "change_id": "cccccccc-0000-4000-8000-0000000000a1",
  "folder": {"recorded": true, "present": true, "name": "shop", "reason": null},
  "git": {"branch": "fix-total", "head": "abcdef0123456789abcdef0123456789abcdef01", "detached": false, "reason": null},
  "receipts": {"dir": ".thruline/receipts", "count": 0, "newest": null, "reason": null},
  "check_run": {"name": "thruline/receipt"},
  "badge": {"states": [
    {"conclusion": "success", "label": "verified", "colour": "#2ea043"},
    {"conclusion": "neutral", "label": "partial", "colour": "#bf8700"},
    {"conclusion": "failure", "label": "not verified", "colour": "#cf222e"}]},
  "masking": {"masked": 0, "families": [], "complete": true} }
```

**The rule for every field: a fact that could not be read is `null`, with a reason beside it. Never a guess.**

- **`folder`** — `change_decision::worktree_root`: the folder the decision file and the code-moved line use. `recorded: false` (with `present` and `name` null) when ThruLine recorded none; `present: false` when it is not a directory on this machine now. `name` is the folder's last part only: the full path is never served.
- **`git`** — `git -C <folder> rev-parse --verify --quiet HEAD^{commit}` and `git -C <folder> symbolic-ref --quiet --short HEAD`, both through `bounded_child::output_until` inside one 5 s deadline, system and user git config refused (`git_isolation::isolate_config`), `GIT_DIR` and `GIT_WORK_TREE` removed, `GIT_OPTIONAL_LOCKS=0`. Read-only verbs only. `branch` is the short branch name; `detached: true` with `branch: null` when HEAD names a commit and no branch; `head: null` on an unborn branch. A folder that is not a repository, a missing or recorded-elsewhere folder, git not installed, or git not answering in time: every field `null` and `reason` says which.
- **`receipts`** — the regular `*.json` files directly in `<folder>/.thruline/receipts/` (links and folders are not counted), and the newest by modified time (ties: the greater name): the file the ThruLine Action verifies when given none (`packaging/action/action.yml`, `ls -t .thruline/receipts/*.json | head -1`). `count: 0` when the folder has none or no such directory; `null` with `reason` when it could not be read. **Nothing is opened, parsed or verified**: `thruline verify-receipt` snapshots the working tree with `git add -A` into a temporary index and `git write-tree`, which writes objects into the repository, so the page names the file and the command and does not run it.
- **`check_run.name`** and **`badge.states`** — `thruline_attest::report::CHECK_NAME` and `badge_label` for each `Conclusion`: the name the CLI's `--format check-run` posts and the words and colours its `--format badge` draws. Served so the page cannot draw a name or a word the generator does not produce.
- **`masking`** — the merged record of `redact::mask_recorded` over every string read from the folder (its name, the branch, the receipt's file name, a reason).

**Cost.** Two git reads and one directory listing per request. The page reads it once on opening and every 20 s while it is open.

## `POST /api/changes/<id>/scope/accept` (additive fields, C30 R-ROUTES "Done-Criteria")

The T-2-3 route (`crates/cli/src/uiserve.rs` `change_scope_accept`, write-address table row 11) is unchanged for every body it took before. It gains two optional fields, both written by this one act through `change_propose::accept_with` (`accept` is `accept_with` with nothing added), and nothing else:

```json
{"criteria": [1], "edits": {"1": "…"}, "lines": ["The parser names the line it stopped at."],
 "exclusions": ["error messages in other languages"], "allowed_scope": ["src/**"], "actor": "you", "replacing": false}
```

- **`lines`** — accepted exactly as written, after the chosen proposals, in order: a line the person wrote, or a line of the standing version they keep when they change it. Whose words each one is in is derived as for every accepted line (`attribution`: `as_proposed` when a current proposal says exactly it, otherwise `your_words`). `lines` alone is enough: with no `criteria` and no `all`, the selection is "no proposal", and the version holds the person's lines.
- **`exclusions`** — out of scope, added to the exclusions the standing version already holds, each once, trimmed. It never removes one.
- **Nothing is kept unless it is accepted.** There is no proposals table: a line the person added and did not accept is not stored anywhere, and neither is a drop (no row says a proposal was dropped).
- **Refusals before the store is opened (409, nothing written),** each `{"error", "part": "lines"|"exclusions", "index", "limit", "plain"}`:
  - `too_many`: more than 40 lines (`MAX_ACCEPT_LINES`) or 25 out-of-scope items (`MAX_SCOPE_PATHS`);
  - `empty`: a part that is empty once trimmed;
  - `too_long`: a part over 240 characters (`change_propose::MAX_CRITERION_CHARS`).
- **The core's refusals** stay 400 `{"error":"refused","detail":<sentence>}`, with two more: a line already in the list being accepted ("… is in this list twice, so nothing was saved. Keep it once.") and an empty part.
- **An unknown field is refused** (400 `malformed_request`), never dropped: a misspelt `exclusions` would otherwise lose what a person marked out of scope without a word. (Before C30 an unknown field was ignored.)
- **Body cap.** `SCOPE_ACCEPT_BODY_MAX` = (2 × 40 + 25) × 240 characters × 12 bytes + 2 × 25 × 512 + 4096 = 332,096 bytes, sized from the parts a person can type, instead of the 512 bytes the other action routes keep. A larger declared body is 413 `request_too_large` before it is read. (Before C30 any body over 512 bytes was 400 `malformed_request`.)

Tests: `crates/core/tests/change_propose_additions.rs`, `crates/cli/tests/hq_done_criteria.rs`.

## `GET /api/hq/projects/<id>/done-templates` (C30 R-ROUTES "Done-Criteria: templates from policy.yml")

What the project's `.thruline/policy.yml` holds for templates of what counts as done. Route: `crates/cli/src/uiserve/done_templates.rs`. **Read-only**: it writes, creates and runs nothing (`hq_done_criteria.rs` holds the ledger's bytes identical across its requests). Only `GET` (405 otherwise); a query is 400 `unknown_query_parameter`; an unknown project is 404 `no_such_project`; a ledger that cannot be opened is 503 `store_unavailable`.

```json
{"schema_version": 1, "project": {"id": "dfbfd6089e4bb388"},
 "policy": {"path": ".thruline/policy.yml", "state": "read", "detail": null},
 "templates": [], "reason": "no_templates_section",
 "masking": {"masked": 0, "families": [], "complete": true}}
```

- The file is read from the project's **base branch**, never its working tree. The symbolic `origin/HEAD` default branch is used when present; otherwise exactly one local `main` or `master` must exist. Git supplies the committed blob without following links or fetching missing objects. If the base cannot be identified or read, `policy.state` is `unreadable`, `reason` is `policy_not_read`, and no template is served. No working-tree fallback occurs.
- The closed policy schema accepts this subset: `templates:` is a map of plain names containing ASCII letters, digits, `_` or `-`; each name has exactly one `lines:` inline list of 1–40 nonempty strings, each at most 240 characters. Example: `templates:\n  bug_fix:\n    lines: ["Handle empty input, with an error.", "Keep the input unchanged."]`. These lines are proposals; a person still accepts them. Other policy fields and the gate's verdict retain their meaning.
- `policy.state`: `absent` | `read` | `unreadable` (with a masked reason in `detail`) | `not_a_regular_file` (a Git symlink is not followed) | `too_large` (over 256 KiB, not read). `reason` is `no_policy_file`, `no_templates_section`, or `policy_not_read` when nothing is served; it is `null` when templates are served. Each served template is `{name, lines[]}`, sorted by name; HQ (`ui/src/hq/screens/Done-Criteria/templates.js`) adds a picked template's lines as proposals.

## `POST /api/changes/<id>/notes`, and the `note` on the decision routes (C30 R-LEDGER)

Write-address table row 21, and the row-1 correction. Route: `crates/cli/src/uiserve/person_acts.rs` (`add_note`); core: `crates/core/src/change_note.rs`; tests: `crates/core/tests/change_notes.rs`, `crates/cli/tests/change_notes_route.rs`. **A note is words, never a result:** nothing that decides a verdict, a coverage count, readiness or which approval stands reads it, and it is free (the one paid act is approving a version).

```json
POST /api/changes/<id>/notes        {"body": "Read it twice; fine by me."}
200 {"schema_version": 7, "note": {"id": "cn_01J…", "actor": "alex", "body": "Read it twice; fine by me.",
                                    "recorded_at": "2026-10-08T09:12:44Z", "decision_id": null}}
```

- **Body** is `{body}` and nothing else; an unknown field is `400 malformed_request`. The name on the note is the account ThruLine is running as (`check::account_name()`); the route takes no name from the browser.
- **Refusals, nothing written:** `400 body_required` (nothing but whitespace), `409 {"error":"too_long","field":"body","limit":2000}` (over 2,000 characters), `413 too_long` (a body larger than a note within the limit could make: `MAX_NOTE_CHARS * 12 + 4096` bytes, a note's characters each written the longest way JSON can), `404 no_such_change`, `403 same_origin_required`, `503 upgrade_pending`.
- **Stored as typed, served masked.** The `note` in the answer, `notes[]` and a decision's `note` all pass through `change::mask_text`.

**`GET /api/changes/<id>` (served Change shape, `CHANGE_VIEW_SCHEMA_VERSION` 6 → 7)** gains `notes[]`, oldest first (append order): `{id, actor, body, recorded_at, decision_id}`; `decision_id` is `null` for a note added from the Brief and names the approval or withdrawal row for one kept with it. A row of `decisions.history` (and `decisions.standing`) gains a **derived** `note`, present only when the act carried one: an approval recorded without a note serves exactly the keys it always did.

**The two decision routes** (`POST …/decisions`, `POST …/decisions/withdraw`) take an optional `note` (at most 2,000 characters; nothing but whitespace is no note). When given, the act appends one `change_notes` row naming the new decision row **in the same transaction** (`change_decision::approve` / `withdraw`), so the decision and its note are recorded together or not at all. The answer's `acted.decision` carries the `note`. `409 too_long` names `note` (or `actor`, which the old 512-byte body cap used to bound and is now bounded on its own at 200 characters) before anything is recorded. **Changed with this route, recorded in CLAUDE.md:** these two requests now refuse a field they do not name (`400 malformed_request`; before, it was dropped), and keep the larger body cap above instead of 512 bytes. **The decision file does not carry the note** and stays schema 3 (C34 ruling Q11).

**Terminal door:** `thruline change note <change> --body <text> [--actor <name>]`, the same `change_note::add`. No approve verb exists, so the Approve note has no door. Exit 1 on a refusal.

## `POST /api/changes/<id>/seconds` and `…/seconds/withdraw` (C30 R-LEDGER)

Write-address table rows 22 and 23. Routes: `crates/cli/src/uiserve/person_acts.rs` (`add_second`, `withdraw_second`); core: `crates/core/src/change_decision_second.rs` (`second`, `withdraw`); tests: `crates/core/tests/change_seconds.rs`, `crates/cli/tests/change_seconds_route.rs`, and `seconds_through_the_real_functions_move_no_result` in `crates/core/tests/r_ledger_migration.rs`. **A second is a name, never a result:** it makes no approval stand, rewrites no decision file, moves no verdict, count, readiness or approval availability, and is free (C34 ruling Q2: the one paid act is approving a version).

```json
POST /api/changes/<id>/seconds   {"actor": "Alex", "seen_version": "snap_…"}
200 {"schema_version": 8,
     "acted": {"outcome": "recorded",
               "second": {"id": "cds_01J…", "act": "approved", "decision_id": "cd_01J…",
                          "target_revision": "snap_…", "scope_version": 1, "actor": "Alex",
                          "recorded_at": "2026-10-08T09:12:44Z", "withdraws": null, "counting": true}},
     "decisions": {"standing": {"…": "…", "seconds_standing": 1}, "history": ["…"], "target_revision": "snap_…", "scope_version": 1}}
200 {"schema_version": 8, "acted": {"outcome": "moved", "seen": "snap_a", "now": "snap_b", "changed": ["src/cart.py"], "message": "The approved version is not the one this screen was showing, so nothing was recorded. …"}, "decisions": {"…": "…"}}
409 {"error": "nothing_to_second", "message": "No version is approved on this Change right now, so there is nothing to second. Nothing was saved."}
409 {"error": "same_person", "with": "Sam", "message": "A second approval has to come from a different name than Sam, who is already on this version. ThruLine records the name you type; it does not check who is at the keyboard."}

POST /api/changes/<id>/seconds/withdraw   {"actor": "Alex"}
200 {"schema_version": 8, "acted": {"outcome": "recorded", "withdrawal": {"act": "withdrawn", "withdraws": "cds_01J…", "…": "…", "counting": false}}, "decisions": {"…": "…"}}
409 {"error": "no_second_to_withdraw", "message": "Alex has no second approval standing on the approved version, so there is nothing to take back. Nothing was saved."}
```

- **Bodies** are exactly `{actor, seen_version}` and `{actor}`; an unknown or missing field is `400 malformed_request`, a blank name `400 actor_required`. The name is **typed** (as on the approval routes) and compared trimmed and case-folded (ruling Q3): ThruLine records the name and does not check who is at the keyboard, and the served `same_person` message ends with that sentence. A name or version over 200 characters is `409 {"error":"too_long","field":…,"limit":200}`; a body larger than two such parts could make (`person_acts::SECONDS_BODY_MAX`) is `413 too_long`.
- **Nothing is written** on any refusal (`nothing_to_second`, `same_person`, `moved`, `no_second_to_withdraw`, and every `4xx`). `404 no_such_change`, `403 same_origin_required`, `503 upgrade_pending` as on the other action routes.
- **A second belongs to one approval.** It copies the approval's version and criteria version; it stops counting when that approval is withdrawn or replaced (derived at read, never stored) and is not carried onto the next. A person who took theirs back may second again: a new row, the earlier ones stay.

**`GET /api/changes/<id>` (served Change shape, `CHANGE_VIEW_SCHEMA_VERSION` 7 → 8)** gains `seconds[]`, oldest first (append order), masked: `{id, act, decision_id, target_revision, scope_version, actor, recorded_at, withdraws, counting}`, where `counting` is **derived**: true only for an `approved` second that no later withdrawal took back and whose approval is the one that stands now. `decisions.standing` gains a **derived** `seconds_standing: n`, absent when n is 0 (so an approval nobody seconded serves exactly the keys it always did); `decisions.history` rows do not carry it.

**Terminal door:** `thruline change second <change> [--actor <name>] [--seen-version <v>] [--withdraw]`, the same two functions. Exit 1 on a refusal, with the same sentences.

## `POST /api/hq/projects/<id>/threads/<tid>/told-agent`, and `told_agent` on a thread (C30 R-LEDGER)

Write-address table row 24. Route: `crates/cli/src/uiserve/person_acts.rs` (`told_agent`, dispatched from `hq::handle`); core: `crates/core/src/change_agent_message.rs` (`copied`, `marked_pasted`, `unmarked_pasted`, `last_acts`, `tracking_change`) and `crates/core/src/hq.rs` (`Index::view_told`, `Index::told_agent`); tests: `crates/core/tests/change_agent_messages.rs`, `crates/cli/tests/hq_told_agent_route.rs`.

```json
POST /api/hq/projects/<id>/threads/<tid>/told-agent   {"act": "copied", "message": "Please re-run the totals test."}
200 {"schema_version": 1,
     "act": {"id": "cam_01J…", "act": "copied", "thread_id": "…", "thread_event_id": "01J…", "change_id": null,
             "version_id": "wt-…", "actor": "alex", "recorded_at": "2026-10-08T16:02:11Z", "refers_to": null,
             "message": "Please re-run the totals test.", "message_sha256": "…64 hex…"},
     "told_agent": {"last_act": "copied", "at": "2026-10-08T16:02:11Z", "version_id": "wt-…", "act_id": "cam_01J…", "later_work": false},
     "nothing_sent": true,
     "message": "Nothing was sent. ThruLine only keeps that you copied this and, if you say so, that you pasted it."}

POST …/told-agent   {"act": "marked_pasted"}                      -> refers_to: the latest copy (or the copy named in "refers_to")
POST …/told-agent   {"act": "unmarked_pasted", "refers_to": "cam_…"} -> the standing mark it takes back (latest when absent)
409 {"error": "no_copy_to_mark" | "no_mark_to_undo" | "already_marked", "message": "… Nothing was saved.", "mark": "cam_…" | null, "nothing_sent": true}
```

- **Body** is exactly `{act, message?, refers_to?}` (`deny_unknown_fields`): a digest, an anchor, a version, a Change or a name in it is `400 malformed_request`. `copied` needs `message` and takes no `refers_to`; the other two take no `message`. `400 unknown_act` for any other word (there is no act that could say a message was sent); `400 message_required` for nothing but whitespace.
- **The server reads** the thread's anchor (its first work prompt, `thread_event_id`), the version it is at, and the Change that tracks it (the one whose request is that prompt, the match "Track this work as a Change" uses); the **client sends none of them**, and the digest is the SHA-256 of the exact bytes stored.
- **A message** is kept exactly as written, at most 8,000 characters (`409 too_long` naming `message`; a body larger than that could make is `413 too_long`), and served masked. `403 same_origin_required`, `404 no_such_project` / `no_such_thread`, `503 upgrade_pending` / `store_busy` as on the other action routes. The name on the act is the account ThruLine is running as.
- **A refusal appends nothing.** A mark needs a copy on the same thread; a copy carries at most one standing mark; an undo needs a standing mark.
- **Free.** It asks no licence (`PAID_SURFACES` stays one row). **Nothing is sent anywhere**; the table has three acts and none that could say otherwise. No terminal door.

**`told_agent` on the project view.** `GET /api/hq/projects/<id>` serves each thread's last act (one scan of a small table), judged against the index: `later_work` is true when some row of the thread was recorded after the whole second the act is stamped to. The served project view is cached per (record fingerprint, active threads, the id of each thread's last act), because an act is not an event and the fingerprint does not move when one is kept. A project with no act serves the bytes it served before. The HQ routes' `schema_version` stays 1.

## `POST /api/hq/projects/<id>/objections`, and `objections[]` on a claim (C30 R-LEDGER)

Write-address table row 25. Route: `crates/cli/src/uiserve/person_acts.rs` (`objection`, dispatched from `hq::handle`); core: `crates/core/src/statement_objection.rs` (`object`, `standing`, `last_by_event`, `check_reason`) and `crates/core/src/hq/statements.rs` (`Index::claim_sentence`, `Index::thread_claim_events`) and `crates/core/src/hq/view.rs` (the claim's `objections`); tests: `crates/core/tests/statement_objections.rs`, `crates/cli/tests/hq_objection_route.rs`.

```json
POST /api/hq/projects/<id>/objections   {"statement": "01J…#0", "reason": "The test never ran."}
200 {"schema_version": 1,
     "statement": "01J…#0",
     "objection": {"id": "obj_01J…", "actor": "alex", "at": "2026-10-08T16:02:11Z",
                   "reason": "The test never ran.", "matches_sentence": true},
     "message": "Your objection is kept beside this statement. It does not change what ThruLine says about it."}
```

- **Body** is exactly `{statement, reason}` (`deny_unknown_fields`): a digest, a sentence, a verdict, a name or a withdrawal in it is `400 malformed_request`. A reason with nothing but whitespace is `400 reason_required`.
- **The server reads** the sentence the project's own index gives for the statement id (`<events.id>#<n>`, the ids the thread view and the statements route serve) and keeps its SHA-256; the **client sends neither**. An id this project does not serve — made up, malformed, past the last statement of its event, or an event that holds no statement — is `404 no_such_statement`; an unknown project `404 no_such_project`.
- **A reason** is kept exactly as typed, at most 2,000 characters (`409 too_long` naming `reason`; a body larger than that could make is `413 too_long`), and served masked. `403 same_origin_required`, `503 upgrade_pending` / `store_busy`.
- **It changes no verdict.** The table has no verdict column; `judge_thread`, every count, `whose_move`, readiness, the speaker and the stop gate do not read it. A refusal appends nothing.
- **Free.** It asks no licence (`PAID_SURFACES` stays one row). **There is no way to take one back** (C34 ruling Q16: the table keeps `withdrawn`; no route, control or terminal door offers it). No terminal door.

**`objections[]` on a claim.** `GET /api/hq/projects/<id>/threads/<tid>` serves, on each claim somebody objected to, `objections: [{id, actor, at, reason, matches_sentence}]`, oldest first, from one scan of a small table (an `objected` row no `withdrawn` row points at). `matches_sentence` is false when the stored digest is no longer the digest of the sentence the extractor gives now: the screen says "the sentence changed since" and does not carry the objection onto the new wording. Absent — not empty — when nobody objected, so a thread with none serves the bytes it served before. The served thread view is cached per (record fingerprint, whether the thread is active, the id of the last objection row kept against each of the thread's claim events), because an objection is not an event and the fingerprint does not move when one is kept. The project view, the statements route and the sessions route do not carry it. The HQ routes' `schema_version` stays 1.

## `POST /api/hq/projects/<id>/capture/pause` and `…/capture/exclusions`, and `holds` on the capture document (C30 R-LEDGER)

Write-address table rows 26 and 27. Route: `crates/cli/src/uiserve/capture_holds.rs` (`pause`, `exclusion`, dispatched from `hq::handle`); core: `crates/core/src/capture_hold.rs` (`record_for_person`, `holds_view`, `pattern_problem`). The DDL (`capture_holds`) landed in `c8dc8743`; no DDL here.

**A PAUSE AND AN EXCLUSION ARE HONOURED AT THEIR SEPARATE BOUNDARIES** (C34 rulings Q4, Q5, Q6). Every answer below carries `holds.honoured: true`, `holds.exclusions_honoured: true` and the sentence that says both. A path exclusion replaces matching served and exported entries with the fixed words “left out by your exclusion (`<key>`)” and a `left_out_by_exclusion` count; it does not remove anything from the sealed copy of a transcript (Q4).

The exclusion policy uses the HQ repository root that the hold route records, including when a Change or export names a ledger member whose stored root is a Claude Code transcript directory. Active built-ins and typed paths are matched by path components; `generated` uses the repository's `linguist-generated` Git attribute. An unreadable policy or failed generated-file lookup refuses the presentation instead of serving the unfiltered view. With no matching entry, the original JSON bytes are returned. The policy is applied to HQ project, thread, session and search reads, Change index/detail/coverage/readiness/export reads, the staged UI fixtures, local-model input, and both CLI export modes. The private UI export stage retains the derived source so an `included` act restores its presentation on the next read; a new standalone export writes the filtered file.

The legacy session projections (`/api/explain`, `/api/claims`, `/api/phases`, `/api/read`) and integration presentation/action JSON also apply the selected session's project policy. Run sheets, capture health, PR previews, install plans and done templates filter their path-bearing display fields. Decisions GET JSON passes through the same final presentation boundary without changing the decisions engine's stored judgments. For the separate `/api/reviews/export` preserved-evidence ZIP, the founder chose refusal when its authenticated manifest names a path covered by the current exclusion. The download is also refused while any exclusion is active and the authenticated manifest lists opaque attachments: their content cannot be proven free of excluded file material from the manifest's digest-only paths. Both refusals use fixed words and echo no attachment path. An attachment-free, nonmatching archive is served as the original hash-verified ZIP bytes, unchanged. An unreadable policy or manifest lookup refuses the download rather than silently serving the archive.

```json
POST /api/hq/projects/<id>/capture/pause   {"act": "paused"}      // or "resumed"
200 {"schema_version": 1, "act": "paused",
     "hold": {"id": "chd_01J…", "act": "paused", "pattern": null, "at": "2026-10-08T09:00:00Z", "by": "alex"},
     "holds": {"paused": true, "paused_at": "2026-10-08T09:00:00Z", "paused_by": "alex",
               "exclusions": [ {"key": "env", "covers": ".env and .env.* files", "note": "environment files",
                                "built_in": true, "on": true, "source": "default",
                                "basis": "on by default (not recorded)", "since": null, "by": null}, … ],
               "honoured": true, "exclusions_honoured": true,
               "note": "While paused, ThruLine reads nothing from this folder, and a session started while it was paused is never read. A path exclusion leaves matching entries out when ThruLine shows or exports a view. It does not stop recording, and ThruLine's sealed copy of each transcript still holds what Claude Code wrote."},
     "codex_observer": "not_watching",
     "message": "Pause recorded for this project."}

POST /api/hq/projects/<id>/capture/exclusions   {"act": "excluded", "pattern": "vendor/legacy"}   // or "included"
200 {"schema_version": 1, "act": "excluded", "pattern": "vendor/legacy", "hold": {…}, "holds": {…},
     "message": "Recorded: this path is excluded."}
```

- **Body** is exactly `{act}` (pause) or `{act, pattern}` (exclusions), a JSON object (`deny_unknown_fields`): a folder, a project, a name, a time, or an `ends` in it is `400 malformed_request`, and so is an array. Any other act word — the exclusion words on the pause route and the pause words on the exclusions route included — is `400 unknown_act`. The folder is the project's own root as HQ derives it; the browser names none. The name on the row is the account ThruLine is running as.
- **One row each** on the `capture_hold` hash chain that `thruline verify` re-folds. A pause the folder already has is `409 already_paused`; resuming one that is not paused is `409 not_paused`; excluding what is already excluded — **a built-in default that is on counts** — is `409 already_excluded`; including what is not excluded is `409 not_excluded`. Each carries a `message` and appends nothing.
- **Defaults are code, not rows** (C34 ruling Q7). The first change to one appends. Turning a default **off** appends **two** rows in one transaction: the default written down as an `excluded` row under the actor "ThruLine default" (so no person is said to have excluded it), then the person's `included` row ending it. The answer's `hold` is the person's row. Once any row names a key in a folder, the record decides it.
- **A pattern** is a built-in key (`env`, `secrets`, `generated`) or one relative path pattern, trimmed, at most 256 characters, not absolute, no `..` component, no control character, `*` the only wildcard. A folder called `env` is written `env/`. It is checked for its shape and never opened. `400 bad_pattern` with a `message` that says nothing was saved; `409 too_long` `{field: "pattern", limit: 256}`; a body larger than a pattern within the limit could make is `413 too_long` (the pause route's cap is 512 bytes).
- **The Codex observer.** `codex_observer` is `paused` / `resumed` when the route also set the observer's in-memory flag (`Observer::pause`, as `POST /api/observation` does), which it does only after the row is appended and only for the folder the observer watches; `not_watching` otherwise, including when HQ was started with no observation source. The exclusions route has no such field.
- **Errors** as the other HQ writes: `403 same_origin_required`, `404 no_such_project`, `503 upgrade_pending` / `store_busy`.
- **Free.** Neither route asks a licence (`PAID_SURFACES` stays one row). Terminal doors `thruline capture pause|resume|exclude|include|holds` (each takes `--project <id or folder>`) call the same core function; `holds` is the design's `capture status`, whose name Assured Capture already holds.

**`holds` on `GET /api/hq/projects/<id>/capture`** is described in that section above.

### The pause gates (C34 R-LEDGER (7), rulings Q5 and Q6)

No new address. A recorded pause is read by the six capture callers, each in its own caller and through one module (`crates/core/src/capture_pause.rs`); `ingest.rs`, `sensor::append`, `mirror.rs` and every `change_*` writer are byte-untouched.

- **The rule (the session-start rule).** While a folder is paused, nothing under it is read. After the pause is lifted, a recording whose **session started inside the pause** (its first timestamped transcript line, or its Codex `session_meta`, falls in `[paused.recorded_at, resumed.recorded_at)`) is never read, and is counted. A session that started outside any pause is read in full. "Inside a folder" is by whole path components (`/a/b` is not inside `/a/bc`), as written or as the disk spells it.
- **The six callers.** (1) Hook deliveries, `thruline sensor append`: a delivery whose `cwd` is under a pause in force, or whose session started inside a lifted one, is not appended, not sealed on PreCompact and given no liveness marker; it exits 0, prints nothing and spills nothing. The read fails open: any error is "not paused". (2) The Stop audit: no `speaker_findings` row is appended. (3) The HQ load, `POST /api/hq/projects/<id>/load`: recordings are left out of the plan before `files_total` is counted. (4) The first-use register, `POST /api/first-use/register`. (5) The Codex observer: it reads the record on its first cycle and every cycle after, and `POST /api/observation` `resume` is `409 paused_in_record` while the record holds the watched folder paused. (6) `thruline ingest`: one printed line per paused folder, and a summary line.
- **Every skip is counted.** `GET /api/hq/load` and the `202` of the load route gain `paused_by_you` and `started_while_paused` (both always present; `files_total` counts what is left). `POST /api/first-use/register` answers the same two keys. `thruline ingest` ends with `paused  N recording(s) not read: A because the folder is paused, B because the session started while recording was paused`. The hook and the audit, which have no one to tell, leave one line each in `capture-pause-skips.jsonl` beside the ledger (`{at, gate, folder}`: the folder and the time, never a session or a word of the work), and the capture document serves the count as `paused_skips`; `thruline capture holds` prints it.
- **`recorded_while_paused` on a thread row.** `GET /api/hq/projects/<id>/threads/<tid>` marks each prompt, edit, command and claim whose `ts` falls inside a pause recorded over the project's folder with `recorded_while_paused: true`. **Absent, never `false`**, so a project nobody paused serves the bytes it served before; `schema_version` stays 1. A join made when the view is read; nothing is written. The thread cache key carries the last pause or resume act, so a pause moves the served bytes.
- **Zero holds is a no-op.** With no pause or resume act in the record every gate returns before it opens a recording (`PauseGate::is_empty`); `crates/cli/tests/capture_gates_noop.rs` runs one fixed sequence on the base code and on this code and compares the world they leave, byte for byte (events, chain heads, `sensor_events`, mirror files).

## Behaviour changes to routes that existed before C30 (collected, C38 §3)

Three routes answered some requests differently after C30. Each change is made on purpose and is also written in the section of the route it came with; this is the one list, so nobody has to find them by diffing. Every request that used to succeed with a body the route took before still does. What changed is how the route treats a request it used to treat loosely.

| Route | Before C30 | Now | Why |
|---|---|---|---|
| `POST /api/changes/<id>/decisions`, `POST /api/changes/<id>/decisions/withdraw` | A field the route did not name was dropped without a word (a `note` included). A body over 512 bytes was `400 malformed_request`. A name was bounded only by that 512-byte cap. | **An unknown field is `400 malformed_request`**. **The body cap is `person_acts::BODY_MAX`** = `MAX_NOTE_CHARS * 12 + 4096` = 28,096 bytes, sized from the largest note a person can write; a larger declared body is `413 too_long`. **A name over 200 characters is `409 too_long` naming `actor`** (`DECISION_ACTOR_MAX_CHARS`), answered before anything is recorded. The routes also take an optional `note` of at most 2,000 characters. | A misspelt field must not be lost silently. The cap had to grow for a note, so the name needed a limit of its own. |
| `POST /api/changes/<id>/scope/accept` | An unknown field was ignored. A body over 512 bytes was `400 malformed_request`. | **An unknown field is `400 malformed_request`**. **The body cap is `SCOPE_ACCEPT_BODY_MAX`** = 332,096 bytes; a larger declared body is `413 request_too_large` before it is read. `lines` and `exclusions` are accepted; their refusals are `409` naming the part. | A misspelt `exclusions` would otherwise lose what a person marked out of scope. |
| every path under `/api/changes/` and `/api/sessions/` | A path whose prefix and suffix overlapped (`/api/changes/decisions`, `/api/changes/notes`, `/api/sessions/changes`, …) made the handler **panic**: the router sliced the path by the suffix length, `[13..12]`. | **Refused with a 4xx; no request can panic a handler.** The id between prefix and suffix is taken with `strip_prefix` / `strip_suffix`, which answer none instead of panicking. | `crates/cli/tests/router_short_paths.rs` sends every degenerate shape of every route the router's source names, over four methods, and requires an answer every time. |

A client that sent an extra field to one of the first two routes now gets a 400 where it used to get a 200. The HQ screens send exactly the fields in this document, and `crates/cli/tests/change_notes_route.rs`, `change_seconds_route.rs` and `hq_done_criteria.rs` pin the refusals.

## Per-Change recorded-chain check (C30 R-ROUTES)

`GET /api/changes/<id>/integrity` checks the event hash links and session heads for sessions **currently confirmed** for this Change. It does not re-read the agent's source files, verify a downloaded Whole Story file, run `thruline verify`, or update verification metadata. The route is read-only; repeated GETs leave the ledger unchanged. A request for another method is `405 method_not_allowed`, a query is `400 unknown_query_parameter`, and an unknown Change is `404 no_such_change`.

```json
{"schema_version":1,"change_id":"chg_…","checked_at":"2026-10-09T07:00:00Z","state":"INTACT","sessions_checked":2,"links_checked":11,"reason":null,"scope":"confirmed_change_sessions","source_files_checked":false,"saved_export_checked":false}
```

`state` is `INTACT` only when every selected link and stored session head adds up. A broken link, sequence or head is `MISMATCH`; absent confirmed sessions, an incomplete head or a read failure is `UNVERIFIABLE`. Whole Story names that limited scope beside the result and keeps its separate warning that a saved export does not prove it stayed unchanged.
