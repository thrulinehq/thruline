# Change-Brief — data contract

`renderVals()` reads `const D = this.props.data || DEMO` and nothing else for
content. `DEMO` (`demo.js`) is one **snapshot** of a Change (the canvas's S1
state) plus `DEMO.states = { s1, checking, ready, moved }`: four complete
snapshots, one per state the canvas's demo switcher selects. A live Change is
one snapshot of this shape, with no `states`.

This snapshot is meant to become the core of the one shared Change model that
every Change screen reads in wave 2 (Every-Statement, What-Changed,
Done-Criteria, Repair-Loop, Approve-Sheet, Whole-Story). It is shaped as
**records** (versions, edits, runs, statements, criteria), not as Brief
sentences. Every sentence on the screen is composed in `renderVals()` from these
fields.

## Store columns

The **Store today** column was filled by a read-only survey of the worktree on
2026-10-07. It is **asserted** from code reading; no route was called.

- **yes** means a route serves the fact.
- **partial** means part of it is served, or it is stored but not served.
- **no** means nothing holds it.

The routes cited are the existing `/api/changes/*`, `/api/claims`,
`/api/sessions` and `/api/storage`. `ui/src/hq/data/api.js` also names
`/api/hq/*` routes, but no server route serves them yet. `crates/core/src/hq.rs`
and `crates/core/src/hq/*.rs` exist but are not compiled: `lib.rs` has no
`mod hq`, and `hq/view.rs` is missing. Anything found only there is marked
**unrouted**.

## What the screen derives instead of storing

These are computed in `renderVals()` from the fields below. They are not data.

- **Which of the four situations the Change is in.**
  - `ready` when `change.state === 'ready'`.
  - `moved` when `change.state === 'needs_recheck'`.
  - `checking` when a run with `status: 'running'` exists on `currentVersion`.
  - Otherwise `s1`.
  - In the gallery the switcher picks the snapshot. The screen still reads the situation from the snapshot, not from the switcher.
- **Lit or not, for every map node and edge.** The lit thread is the selected statement's proof chain:
  - the run it rests on (`proof.restsOn.runId`);
  - the edit that made the version that run ran on (`proof.ranOn.version`, via `edits[].makesVersion`);
  - the edit after it, `proof.since.editId`, drawn yellow and hatched as ◭;
  - the stop marker, drawn when `proof.stopsAt` is set.
  - For a statement with no verifier, the thread runs from the statement to the "what counts as done" node: dashed to a stop while `criteria.accepted` is false, solid once it is true.
- **The two absence markers on the map.**
  - "Before 16:01 · Not recorded" comes from `change.recordedFrom`.
  - "Nothing recorded · A run on {v} would appear here" is drawn when no `run` node sits in the current version's column.
- **Lane counts.** The number of nodes in each lane, not counting "nothing recorded". The claims lane adds 1 for the findings box.
- **Map geometry.**
  - Band x and width come from the version's index.
  - Node x, y and width come from lane, version column and order within the column.
  - Edge paths are routed by the lanes they join.
  - The canvas placed two anchors by hand for its own demo records: `HAND` in `logic.js`. Every other edge follows the rule.
- **Version relation** ("same version", "1 version older", "n versions older"): the difference between the two versions' positions in `versions`.
- **The version ruler.** For each version:
  - the edit that made it, if that edit came after the brief statement's step (◭ if it is a `since` edit);
  - its runs (● passed, ✕ failed, ⋯ no result, ◔ running);
  - ❝ for a message that holds statements, on that message's `versionBefore`;
  - ┃ for criteria acceptance;
  - ◆ now.
  - Marks are sorted by step.
- **Pager position, statement order in the message, "one message, two statements"**: all from `statements[].messageId` and the message text.
- **Every sentence of product language.** The skeletons ("Looked at: …", "Proof stops here", "claimed, and no proof found in the record for this version. The claim may be entirely true.") live in `logic.js`. Only their facts come from data.

## Snapshot fields

Types are TypeScript-style. "Canvas" gives the S1 value.

### `project`

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `project.id` | string | stable project id | ledger `projects` | yes: `ChangeView.project_id` (change.rs:562) |
| `project.name` | string | name in the breadcrumb ("checkout-web") | `projects.display_name` | partial: stored (ledger.rs:50), not joined into `/api/changes/<id>` |
| `project.worktree` | string | the project's recorded worktree, shown in link ③ ("~/code/checkout-web") | project's recorded worktree path | partial: stored per project; not on the Change route. Served paths must go through `redact.rs` |

### `machine`

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `machine.recordsBytes` | number (bytes) | what ThruLine keeps on this machine (footer "1.4 GB"; L1.1) | storage census | **yes**: `GET /api/storage` → `bytes` (uiserve.rs:1164) |
| `machine.networkCalls` | boolean | backs "The ThruLine app makes no network calls" | constant of the build | n/a (the footer text is literal product language) |

### `record`

The Technical drawer.

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `record.chainHead` | string | hash-chain head, as the canvas abbreviates it ("sha256:afb960f9…2861") | `events.chain_hash` / `project_heads.digest` | partial: `/api/sessions` serves `head_hash`; nothing per Change. Wave 2 should serve the full 64-hex value and let renderVals abbreviate |
| `record.tree` | string | tree hash of the current version ("c324df66…") | exec `snapshots.tree_hash` | partial: `/api/claims` serves `final_tree_hash` per session |
| `record.feeds` | string[] | what fed the record ("Claude Code hook v0.4", "the exec recorder") | capture lifecycle | partial: `capture_lifecycle_records.source_version` stored (ledger.rs:897), not served |

### `sessions[]`

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | session id | ledger sessions | yes: `ChangeView.associations` (change_session_associations) |
| `agent` | string | "Claude Code" / "Codex", used in "Change · 1 session · Claude Code" and "Claude Code said" | session harness | partial: `GET /api/sessions` → `harness` (sessions_cmd.rs:84) |
| `source` | string | the capture source and version ("Claude Code hook v0.4") | `capture_lifecycle_records.source_version` | partial: stored, not served |
| `startedAt` | string (HH:MM, local) | when the session started | `sessions.started_at` | partial: `/api/sessions` → `first_event_at` |
| `step` | number | step of the session-start record | `events.seq` | partial: not on the Change route |

### `request`

The person's prompt.

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `request.step` | number | step of the prompt | `events.seq` of `changes.request_event_id` | partial: the route serves the event id only (`RequestView.prompt_event_id`) |
| `request.time` | string | when it was sent | `events.ts` | partial |
| `request.text` | string | the prompt, verbatim | `events.text` | partial: not served by `/api/changes/<id>`; unrouted `hq Row.prompt` (hq.rs:443) |

### `messages[]`

What the agent said, one row per message.

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | message record id | events | partial |
| `step` / `time` | number / string | where and when | `events.seq` / `ts` | partial |
| `sessionId`, `agent` | string | who said it | session | partial |
| `text` | string | the whole message. The map shows its first sentence plus " …" (step 32) | `events.text` | partial: `/api/claims` serves claim `text` + `span{event_id, offset}`, not the whole message |

### `brief`

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `brief.statementId` | string | the statement the header (headline, chain strip, map default) is about: the checkable one that gates the Change | a selection rule. Canvas: the `tests_pass` statement | **no**: a wave-2 rule ("the first not-proven statement with a verifier") |

### `change`

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | Change id | `changes` | yes: `GET /api/changes/<id>` |
| `title` | string | breadcrumb title | `RequestView.title` | yes (change.rs:546) |
| `state` | `'not_proven' \| 'ready' \| 'needs_recheck'` | the L1 word: pill, status bar, situation | readiness + applicability | partial: `Readiness.ready` (change_readiness.rs:371) via `/coverage`/`/readiness`; "needs a re-check" from `/applicability` (voice.rs:1289). No single field |
| `stages[]` | `{ id: 'define'\|'prepare'\|'build'\|'verify'\|'review'\|'approve', status: 'done'\|'current'\|'not_started'\|'not_reached' }` | the six-stage strip | derivation over scope, runs, review, decisions | **no**: the route has only `examination`/`decision = "none_yet"` |
| `recordedFrom` | string (HH:MM) | recording of this project began here; anything before is "Not recorded" | capture `installed` time | partial: `capture_lifecycle_records.accepted_at_unix_ms` stored, not served; unrouted `ProjectSummary.first_activity` (hq.rs:204) |
| `stepCount` | number | steps in the Change ("19 steps") | count of events in the Change's sessions | partial (derivable from events; not served) |
| `recording` | boolean | capture is live ("· recording"; the Recording pill) | capture state | partial: `/api/state`. The pill markup is literal, so only the status-bar word reads this field |

### `view`

The person's read of the screen.

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `readAt` | string | "read 16:04 ↻": when this snapshot was read | client clock at fetch | n/a (client) |
| `readDate` | string (YYYY-MM-DD) | tooltip date | client clock | n/a |
| `readVersion` | string | the version on screen when it loaded. In `moved` this differs from `currentVersion` ("Approve e71c5b0" disabled) | the version at first load | n/a (client keeps it) |
| `previousLookAt` | string \| null | when the person last looked ("Since you looked at 16:04") | per-person last-seen time | **no** |
| `sinceLook` | `{ criteriaAccepted: boolean, runs: number, filesChanged: number }` \| null | what happened since then | diff of two snapshots | **no** |
| `changedSinceLoad` | string[] | files changed since this screen loaded (moved) | diff of `readVersion` → `currentVersion` | **no** (needs the diff route) |

### `versions[]`

In order; the last is usually current.

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string (7 hex) | version id ("e71c5b0") | worktree tree identity | partial: `ExaminedInputs.source_identity` via `/api/changes/<id>/inputs`; exec `snapshots(tree_hash, head_commit, dirty)` unrouted; unrouted `hq Version` uses commit or `wt-sha(session:seq)` |
| `madeByStep` | number | the edit step that produced it | edit → snapshot link | **no** |
| `recordedAtStep` | number \| null | the step at which ThruLine recorded it (receipt chip "Version · step 58") | snapshot record seq | **no** |
| `commit` | string \| null | the commit it equals ("= commit 3f9a1c2") | `head_commit` | partial (`base_revision`) |
| `unsavedChanges` | boolean \| null | includes uncommitted changes | `dirty` / `uncommitted` | partial (`ExaminedInputs.uncommitted`) |
| `illustrative` | boolean? | demo-only marker; adds "(illustrative)" in the gallery only | n/a | n/a |

`currentVersion` (string) is the version the Change is about now. Store today: partial (`Readiness.target_version`).

### `edits[]`

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | record id | tool_use event | partial |
| `step` | number | step | `events.seq` | partial |
| `time` | string \| null | when (the canvas shows it for 56 and 64 only) | `events.ts` | partial |
| `file` | string | the path edited | edit tool input | partial: raw events only; unrouted `hq/edits.rs FileEdit` |
| `makesVersion` | string | the version this edit produced | edit → snapshot | **no** |

### `runs[]`

Every command execution.

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | record id | | partial |
| `step` | number \| null | step (null while running) | seq | partial |
| `time` | string \| null | when it finished (step 62 at 16:09) | `bound_at` / `started_at` | partial |
| `kind` | `'test' \| 'command'` | a test run or another command (step 60 is a command) | test-runner heuristic | partial: unrouted `hq/commands.rs`; `identity::is_test_invocation` |
| `command` | string | the command, verbatim | argv | partial: `EvidenceBinding.argv` (evidence_binding.rs:386) |
| `version` | string | version it ran on | binding → snapshot | partial |
| `status` | `'finished' \| 'no_result' \| 'running'` | whether a result is recorded | result presence / execution state | partial: `ExecutionRecord` state `running`/`not_run`/`interrupted` (run_execution.rs:259) |
| `exitCode` | number \| null | exit status, only when the record states one | result text | partial: `evidence_bindings.exit_code`; run sheet `outcome_of` |
| `passed` / `failed` | number \| null | **the per-runner tally**: never from the agent's prose | runner output | yes for a run the wrapper records: `check_run_tallies` (test_tally.rs) holds libtest's count and, since C30 R-RUN, pytest's own summary (`check.rs` `parse_pytest_summary`); a transcript run's tally is `hq/tally.rs`. Null when the record holds no count |
| `durationMs` | number \| null | duration ("0.42 s"; null → "duration not recorded") | start/end | partial (derivable, not stored) |
| `capturedBy` | `'thruline' \| 'transcript'` | who captured the run | binding source | partial: `CoverageView.execution_sessions` |
| `scope` | string[] | paths the run covered | argv scope | partial: `scope_key` |
| `elapsedS` | number? | seconds so far, while running | live job | yes (C30 R-RUN): the run sheet's `job.elapsed_s`, whole seconds from the run's own recorded start, stands in for the snapshot's on the same run (`data/run.js` `liveOutputOf`, read by `logic.js`); the snapshot's own is now minus the job's `started_at` |
| `output` | string[]? | live output lines, while running | live job | yes (C30 R-RUN): the run sheet's `job.output.lines`, masked on the server; the live box draws the command and the newest two, "Nothing printed yet." when it serves none yet, and "ThruLine records what this check prints. It is not shown while the check runs." when it serves no output (a check started from a terminal, or a server from before R-RUN). The snapshot row's own `output` stays `[]`; the gallery's is illustrative |

### `statements[]`

The checkable statements, in pager order.

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | statement id | claim id | partial: `/api/claims` `id` |
| `messageId` | string | the message it was cut from | claim span event | partial: `span.event_id` |
| `text` | string | the fragment ("Tests pass.") | claim text | yes: `/api/claims` `text` |
| `claimKind` | `'tests_pass' \| 'task_complete' \| …` | what kind of claim it is | `ClaimKind` | yes (claims/src/claim.rs:30) |
| `verifier` | boolean | whether any rule can check this kind (false → "Recorded", decided by what counts as done) | rule registry | partial (implicit: `task_complete` has none) |
| `step` / `time` | number / string | where and when it was said | span event | partial |
| `sessionId` / `agent` | string | who said it | session | partial |
| `versionBefore` | string | "the last version ThruLine recorded before this was said" | snapshot before the span event | **no**: only a per-session `final_tree_hash` |
| `source` | `{ file, line }` \| null | the raw JSONL receipt. null → "the transcript line" | `events.raw_file` / `raw_line` | partial: stored; `LogReceipt` served only for each session's final claim |
| `proof` | see below | the ①–⑥ chain | | |

### `statements[].proof`

The six links. Every link is a typed fact, never a sentence.

| Link | Field | Type | Meaning | Store today |
|---|---|---|---|---|
| ⑥ | `verdict` | `'not_proven' \| 'proven' \| 'recorded'` | the verdict, on `version` | partial: `/api/claims` `verdict` = supported/unsupported/unverifiable. No "recorded"; not version-bound per statement |
| ⑥ | `version` | string | the version the verdict is for | partial |
| ⑥ | `scope` | string[] | what a proven verdict covers ("tests/test_totals.py, checkout/") | partial |
| ⑥ | `rule` | string \| null | the deciding rule (`tests_ran_before_final_edit`) | yes: `findings[].detector` (verify/src/tests_pass.rs:207) |
| — | `stopsAt` | 2 \| 5 \| null | where the proof stops (5 = re-check missing) | **no** (derivable) |
| ② | `restsOn` | `{ kind: 'run', runId, basis: 'cited_by_agent' \| 'matched', match?: { sameCommandAs: step, widerScope: boolean }, checkedAgainst?: { covers: string[], misses: string[] } }` or `{ kind: 'criteria' }` | the evidence it rests on, how it was linked, and what the agent's cited run did not cover | partial: `/api/claims` `evidence`/`binding`; `LinkBasis` Declared/Named/Manual (change_evidence.rs:143). No "matched by ThruLine" (a command match; the canvas says "needs engine") |
| ③ | `ranOn.version` | string | the version that run ran on | partial |
| ④ | `since` | `{ editId, dependsOnWholeChange: boolean }` \| null | the edit after the run that moved the code. `dependsOnWholeChange`: the check declared no file dependencies | partial: the `tests_ran_before_final_edit` finding; unrouted `Proof.edited_after_last_pass` (hq.rs:228) |
| ⑤ | `recheck` | `{ status: 'none' \| 'running' \| 'not_needed', runId }` | the re-check on the current version | partial: `/coverage` `applies`/`fresh` |
| ① | (the statement's own `step`, `time`, `agent`, `text`) | | said | partial |
| — | `lookedAt` | string[] (run ids) | the runs the verdict considered ("Looked at: …") | **no**: only `rows_scanned` |
| — | `earlier` | `{ at, verdict, version, scope }` \| null | what this read at an earlier look ("At 16:04 this read: ○ Not proven…") | **no** (needs verdict history) |
| — | `thenAndNow` | `{ from, to, noLongerCounts: [{ runId, editId }], kept: [], added: [{ runId, newScope }] }` | "Proof, then and now" | **no** |
| — | `followUp` | `{ toldAgent, laterWork: { editId } \| null, laterWorkSince, checkedAgain: { runId } \| null }` | "After it was raised" | **no** |

### `criteria`

What counts as done.

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `accepted` | boolean | whether the person accepted it | **yes**: `ChangeView.accepted_scope` / `ScopeView` (change.rs:441), `change_scope_acceptances` |
| `acceptedAt` | string \| null | when ("16:06") | yes: `acceptance.accepted_at` |
| `acceptedOnVersion` | string \| null | the version current when it was accepted (ruler ┃ mark) | partial (derivable) |
| `acceptedAfterStep` | number \| null | the last step before acceptance (orders the ┃ mark among runs) | **no** |
| `version` | string \| null | the criteria version ("v1") | yes: `ScopeView.version` |
| `lineCount` | number \| null | lines accepted ("4 lines") | yes (derived from `criteria`) |
| `proposedCount` / `proposedFromStep` | number | "4 statements are proposed from your request (step 6)" | yes: `GET /api/changes/<id>/proposals` (`ProposedCriterion`) |
| `provenOn` | `{ version, count, runId }` \| null | how many lines are proven on a version, and the run they rest on | yes/partial: `/coverage` `CriterionCoverage` (change_evidence.rs:1348), `Obligation.outcome` |

The individual criteria lines are not drawn on the Brief; Done-Criteria owns them.

### `recheck`

The check that would settle the brief's statement.

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `runner` | string | "pytest -q" | partial: `GET /api/changes/<id>/run` `checks[].command`, cargo-test only (check.rs:231) |
| `scope` | string[] | ["tests/test_totals.py", "checkout/"]. The command shown is `runner + scope` | **no**: there is no `--scope` on `thruline check` (main.rs:1426). The technical line "thruline check --scope … -- …" is the canvas's, and no CLI accepts it today |
| `on` | string | the version it would run on | partial |
| `permission` | `'ask_once_per_project' \| …` | "asks once for this project" | yes: `run_permissions` + `POST …/run/permission` |

### `findings[]`

What ThruLine noticed. Typed; the text is composed.

| `kind` | Fields | Shown as | Store today |
|---|---|---|---|
| `edit_after_run` | `editStep` | "Edit after the run · step 56" | partial (verify finding) |
| `no_check` | `file` | "checkout/api.py: no check" | **no** |
| `agent_reported` | `claimStep`, `runStep` | "Agent-reported · 52 vs 46" | partial (`EvidenceKind` + `execution_sessions`) |
| `run_outcomes` | `failedSteps[]`, `passedSteps[]` | "Step 26 failed, 46 passed" | partial |
| `no_result` | `step` | "Step 60: no result yet" | partial (`ExecutionState`) |
| `edit_covered` | `editStep`, `runStep` | "Edit at 56 · covered by 62" | **no** |
| `in_scope` | `path`, `runStep` | "checkout/ in scope at 62" | **no** |

Any finding may carry `resolved: true`, which renders it muted.

### `graph`

The lane graph.

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `nodes[]` | `{ id, lane: 'request'\|'work'\|'execution'\|'claims', kind: 'session'\|'request'\|'edit'\|'agent_said'\|'run'\|'command'\|'criteria'\|'statement', step: number\|null, version: string\|null, label: string\|null, time?: string, ref: string }` | a record placed on a lane and, for work and execution, on its version's column. `ref` names the record in `sessions`/`edits`/`runs`/`statements`; the tally and "makes {version}" are read from it. Order within a lane and column is display order | partial: `nodes` table (derived graph) exists; no route serves lanes or columns |
| `edges[]` | `{ from, to, kind: 'recorded_order' \| 'documented_reference' \| 'inferred' }` | the record's links. recorded order = the record's own sequence; documented reference = one record names another (the agent cites step 46); inferred = ThruLine's reading | partial: `edges(kind IN ('seq','inferred'))` (ledger.rs:286). There is **no documented-reference kind**, and no route serves edges |

Lit is not stored; see the derived facts at the top. The canvas draws the same six base edges in all four states. It never draws `edit-56 → edit-64` (recorded order, moved state) or `stmt-52-tests → run-62` (matched, ready and moved) as grey base edges, only as thread segments. DEMO keeps those two out of `edges` so the render stays pixel-identical. A live store should list them, and the thread will draw over them.

### `primaryAction`

`'check_again' | 'stop_check' | 'approve' | 'check_what_changed'`: the Change's one primary action. It picks the proof panel and the button label "Check it again" or "Check what changed". The inspector footer's action is per selected statement and is derived.

Store today: **no** single field. It is derivable from readiness and run state.

## Not yet in the store, in one list

- The six-stage statuses.
- Which edit made each version, and the step each version was recorded at.
- The last version recorded before a statement was said.
- "Matched by ThruLine": linking a statement to a later run by command match.
- A pytest tally for a run the run sheet starts: the wrapper counts pytest since C30 R-RUN, but the sheet can start only `cargo-test` (`check::RUNNABLE_CHECKS`).
- The "looked at" set of runs.
- Verdict history ("At 16:04 this read …").
- Proof then-and-now, and the after-it-was-raised follow-up.
- The documented-reference edge kind, and any route serving graph lanes and edges.
- The "no check" and "covered by" findings.
- The re-check suggestion for non-cargo runners, and `thruline check --scope`.
- Output for a check started from a terminal while it runs: it prints to that terminal, and the run sheet serves only what a check HQ started has printed (C30 R-RUN).
- The per-record `file:line` receipt (only the final claim per session has one).
- The person's previous-look time, and what changed since.

## Canvas inconsistencies kept on purpose

These are kept so the gallery stays pixel-identical.

- Link ② reads differently in two states. Ready says "matched by ThruLine ⓘ (same command as step 46, wider scope, current version) · 0.42 s". Moved says "matched by ThruLine ⓘ · captured by ThruLine · 0.42 s". The Brief formats ② by situation.
- "Evidence by kind" lists runs with their versions in S1 and checking, and as "steps 26, 46 and 62" in ready and moved.
- "Looked at" adds "Nothing has run on {v}." only in the moved state.
- `run-62`'s statement edge lands 124 px into the node; every other one lands at 112 px. The inferred edge `run-26 → said-32` sits at x+120 rather than the centre. Both are in `HAND`.

## One deliberate change from the canvas

The version ruler's `aria-label` is now derived. In the moved state it reads "4b19a0c, 9f2ad3e, e71c5b0, then the current version" where the canvas's literal left out e71c5b0. Nothing visible changes.

## Wave 2 (W2-B): the live snapshot, `ui/src/hq/data/change.js`

`changeSnapshot(threadView, tracked, now, options)` builds this shape from one served ThreadView (`GET /api/hq/projects/<p>/threads/<t>`, docs/hq-next/API.md) and, when the work is tracked as a Change, from that Change's `deriveChangeModel` result. `use-change.js` reads it for an address. Nothing below was renamed; every new field is an **addition**, marked here.

### Where each field comes from now

| Field | From |
|---|---|
| `project.{id,name}`, `project.worktree` | `view.project.{id,name,root}` |
| `machine.recordsBytes` | `GET /api/storage` → `bytes`; null when it does not answer |
| `record.chainHead`, `record.tree` | **null** — not served on the HQ routes; the Technical line is omitted |
| `record.feeds` | session sources: "Claude Code transcripts" / "Codex transcripts" |
| `sessions[]` | `view.sessions` (`agent` from `source`; `source` capture version **null**, not served) |
| `request` | the first `prompts[]` row with `work: true`; plus `id`, `sessionId`, `source {file,line}` (added) |
| `messages[]` | claims grouped by `event_id`; `text` = their sentences joined (the whole message is not served) |
| `versions[]` | `view.versions`, served order, the current one moved last. `id` = short form a person reads (7-char commit, else `wt-` + 7). Added: `fullId` (the engine id), `message`, `sessionId`. `madeByStep`, `recordedAtStep` **null** |
| `currentVersion` | `view.current_version_id`, short form |
| step numbers | a row's 1-based place in the thread's served record (prompts, edits, commands, claims) ordered by time; `change.stepCount` = that record's length |
| `edits[]` | `view.edits`. Added: `ts`, `worktree`, `hunks` and `source` **as served** (API.md §Edit), `op`, `receipt {file,line}`, `linesAdded`, `linesRemoved`, `ok`, `sessionId`. `file` is relative to the project root, or to the member root (another worktree) it sits under with `worktree` naming it; `.claude/worktrees/<name>/` likewise; any other path as served |
| `runs[]` | every `view.commands` row. `kind` test/command from `is_test`; `status` finished unless `outcome: unknown` → `no_result`; `passed`/`failed` from `tally` — **null when no runner summary**, never 0; `exitCode` only when stated; `capturedBy: 'transcript'`; `durationMs` **null**. Added: `outcome`, `ts`, `sessionId`. `scope` = the paths the test command names, **null when it names none** |
| `statements[]` | `view.claims`; `claimKind` = engine `kind` (`tests_pass`, `builds`, `fixed`, `done`, `other`); `verifier` = `has_verifier`; `versionBefore` = the claim's `version_id`; `source` = `raw_file:raw_line`. Added: `plain` — `text` with `**`, `__` and single backticks removed, for display inside ❝ ❞ (`text` stays verbatim) |
| `statements[].proof` | DESIGN-SPEC §4.0, below |
| `brief.statementId` | the latest statement with a verifier whose verdict is not proven; else the latest with a verifier; else the latest |
| `criteria` | the tracked Change only: `accepted_scope` (`version` → `v1`, `acceptance.accepted_at`), `lineCount` = the derived model's criteria, `provenOn.count` = its `counts.proven`, `proposedCount` = the proposals. Untracked: `accepted: false`, every count **null** |
| `change.state` | `needs_recheck` when the current version differs from the one first shown (`options.readVersion`); `ready` when the tracked model's readiness says ready; else the brief statement's verdict. **New values (added): `proven`, `contradicted`, `recorded`** |
| `change.stages` | from criteria acceptance, readiness and the tracked decision; untracked = Define ○ Prepare ✓ Build ● Verify ○ Review — Approve — |
| `change.recording` | **true** only when the thread's last row is within 10 minutes (`whose_move: agent`); else **null** (not known). The Brief's literal "Recording" pill in the top bar is canvas markup and does not read it yet |
| `change.recordedFrom` | **null** — not served; the "Before … Not recorded" marker is not drawn |
| `change.threadId`, `change.whoseMove` | added |
| `view.readVersion`, `view.changedSinceLoad` | the hook keeps the first version shown; files edited after the first read when the version moved |
| `view.previousLookAt`, `view.sinceLook` | **null** — not served |
| `recheck` | the brief statement's run command; `scope: []`, `permission: null`. Since C30 R-RUN the Brief runs a tracked Change's own check in place (`check.js`, through `data/run.js` `useCheckRun`); untracked work opens the check sheet; where the server will not start one, the control is off with the server's own reason |
| `runSheet` | added C30 R-RUN: the tracked Change's served run sheet (`GET /api/changes/<id>/run`), or null. `withRunSheet` folds its typed fields into `runs[]` (a `running` row, or a finished `passed`/`failed` test row with the server's counts, `step: null`, `capturedBy: 'thruline'`) and into each checkable statement's proof, so every screen reads one result |
| `statements[].ts` | added C30 R-RUN: when it was said, to order a check ThruLine ran (no step) against it |
| `statements[].objections` | added C30 R-LEDGER: `[{id, who, at, when, reason, matches, lead, meta}]` (`data/objections.js` `objectionsOf`), from the claim's served `objections[]` (`crates/core/src/statement_objection.rs`); **set only when someone objected**, so a statement nobody objected to is the object it was. Words beside the statement: the inspector says "You said this is wrong: …" under it, with who and when, and the verdict it already has is untouched; `matches` false means the sentence changed since. Written by the ⋯ menu's "This is wrong…" (`objection-actions.js`, `POST /api/hq/projects/<id>/objections`); no way to take one back |
| `findings[]` | `edit_after_run` (the brief's ④), `no_check` (the thread's `edited_after_last_pass`, first two), `run_outcomes` (latest failed and latest passed test), `no_result` (latest test with no result) |
| `graph.nodes` | session (one, labelled with the session count), request, every edit, **every test run** (the execution lane holds what a statement can rest on; every command stays in `runs`), the criteria node, every statement |
| `graph.edges` | each statement → the run it rests on, `inferred` (the engine's latest-test-before link is not the agent's citation). The Brief adds `recorded_order` edges among what it draws (run → the edit before it in its column; request → the first edit drawn) |
| `primaryAction` | `check_what_changed` (moved) · `approve` (ready) · `stop_check` (a run on the current version is running; never served today) · `check_again` |
| `masking` | added: `view.masking` as served |

### The proof chain (`statements[].proof`), from the record

- **No verifier** (`has_verifier: false`): `verdict: 'recorded'`, `restsOn: {kind: 'criteria'}`, `stopsAt: 2` until what counts as done is accepted.
- **Later failing test run** after the claim: `verdict: 'contradicted'` (added value), `recheck: {status: 'failed', runId}` (added value) — ✕ coral at ⑥.
- **A passing test run on the current version after the last edit**, other than the one named: `verdict: 'proven'`, the chain rests on that run, `basis: 'matched'` (same command) or `'later_run'` (added value).
- **No run named** (`rests_on_command_id` null): `verdict: 'not_proven'`, `stopsAt: 2`, `restsOn.runId: null`.
- **The named run has no result**: `not_proven`, `stopsAt: 2` — "No result yet ⋯" never counts as a link.
- **The named run passed**: ④ = the first edit after it. Its command states no file dependencies, so any edit counts and `since.dependsOnWholeChange: true` says so. No edit after it and the same version → `proven`; otherwise `not_proven`, `stopsAt: 5`.
- `restsOn.basis` added value **`latest_before`**: the engine's "latest test command before the claim", worded "the last test run before it was said", never "cited by the agent".
- `earlier`, `followUp.laterWork` are **null** (no verdict history is served).

### What the Brief draws from a live snapshot, and how it caps

At most 3 version columns: the current one and the ones the selected statement's thread passes through, filled with the latest (the first band reads "+N more · {version}"). Per column and lane, at most 2 nodes: the thread's nodes first, then the latest, with the rest folded into one dashed "+N more" node. The claims lane keeps the brief's statement and the selected one. Long record text in a node is cut with "…" (paths from the left); the whole text is in the inspector. The lane counts are the record's (edits, test runs, statements), not what is drawn.
