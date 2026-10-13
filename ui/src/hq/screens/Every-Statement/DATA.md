# Every-Statement — data contract

The screen reads one object: `this.props.data || DEMO` (`demo.js`). Without
`data` it is the design gallery and also draws the canvas annotations (the
"About this board" note). With `data`, it draws only what `data` says.

The screen shows every statement in one Change exactly once: each line of what
counts as done, each checkable statement an agent made, and each sentence the
agents said that no rule applies to ("Also said"). For each one it shows where
its proof chain stops (① to ⑥).

Field names match `Change-Brief/demo.js` wherever a field means the same thing
there: `project`, `machine`, `view`, `change`, `sessions`, `messages`,
`versions`, `runs`, `edits`, `statements[].proof`, `recheck`.

Conventions:
- **step** is the record's step number inside its own session. It is an integer.
- **time** is the local clock time of the record, as `HH:MM`.
- **version** is a short id of a recorded version of the code.
- **Store today** says whether the local store serves this fact now:
  - **yes**, with the route or table;
  - **partial**, with what is missing;
  - **no**;
  - **unknown**.
  - `crates/core/src/hq/*` is the engine work in flight for `/api/hq/` (edits, commands, prompts). It is not routed yet.

## Fields

### `project`, `machine`, `view`

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `project.id` | string | project key | project record | yes — `ChangeView.project_id` (`/api/changes/{id}`) |
| `project.name` | string | the name shown in the breadcrumb | project record | yes — `/api/hq/projects` (orchestrator's `api.js`). The route is not in `uiserve.rs` yet. |
| `project.worktree` | string | the directory the check would run in ("in ~/code/checkout-web") | the project's recorded worktree | partial — `ImplementationView.workspace_identity`; display masking unknown |
| `machine.recordsBytes` | integer, bytes | the size of what ThruLine keeps on this machine, for the footer | storage measurement | yes — `GET /api/storage` |
| `view.readAt` | `HH:MM` | when this screen read the store | the adapter's clock at fetch | n/a — a client fact |
| `view.readDate` | `YYYY-MM-DD` | the date of that read | the adapter | n/a |
| `view.utcOffset` | `±HH:MM` | the local offset, for the hover title | the adapter | n/a |

### `change`, `version`, `sessions`

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `change.id` | string | Change id | Change record | yes — `/api/changes/{id}` |
| `change.title` | string | Change title | `ChangeView.request.title` | yes |
| `change.state` | `not_proven` \| `needs_recheck` \| `recorded` \| `in_progress` \| `proven` | the state pill at the top | the Change's readiness on `version.id` | partial — `/api/changes/{id}/readiness` uses its own vocabulary; a mapping is needed |
| `change.recording` | bool | whether a sensor is recording this project now ("Recording", and "· recording" in the status bar) | sensor liveness | unknown |
| `version.id` | string | the version the screen speaks for, the newest recorded one | the newest entry of `versions` | partial — `ImplementationView.snapshot_id` / `base_revision`; "recorded version" is not served as a list |
| `version.commit` | string | the git commit that version equals ("= commit 3f9a1c2") | the version record | unknown |
| `sessions[]` | array | every agent session in this Change, in order. "session N" is the 1-based position in this list. | Change associations | yes — `ChangeView.associations[].session_id`, `/api/sessions` |
| `sessions[].id` | string | session id | association | yes |
| `sessions[].agent` | string | "Claude Code", "Codex", … | the session's source | partial — `/api/sessions` has a source; the agent name mapping is unknown |

### `versions[]`, `runs[]`, `edits[]`

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `versions[]` | array, oldest first | the recorded versions; the last one is current. Gives "N version(s) older than". | version records | no — no route lists versions |
| `versions[].id` | string | version id | | no |
| `versions[].madeByStep` | int \| null | the edit step that produced it | | no |
| `versions[].recordedAtStep` | int \| null | the step where ThruLine recorded it (③ Ran on chip "Version step 38") | | no |
| `runs[]` | array, step order | every command this Change ran. Feeds ② Rests on, "Looked at: …", and the ③ version. | command records | partial — `crates/core/src/hq/commands.rs` (command, how it ended, exit code); not routed. `change_evidence.rs` `EvidenceView` serves runs per Change. |
| `runs[].id` | string | run id; proof chains point here | | partial |
| `runs[].step` | int | step of the command | | partial (hq/commands) |
| `runs[].sessionId` | string | session | | partial |
| `runs[].version` | string | the version it ran on | | no |
| `runs[].command` | string | the command text | | yes (hq/commands, masked) |
| `runs[].scope` | string[] | the paths the command named ("ran tests/test_totals.py") | the command's arguments | no |
| `runs[].status` | `finished` \| `no_result` \| `running` | whether a result is in the record | | yes — hq/commands `succeeded`/`failed`/`unknown`, mapped |
| `runs[].exitCode` | int \| null | the exit code, only when the result states one | | yes (hq/commands `exit_code`) |
| `runs[].passed`, `runs[].failed` | int \| null | the test tally ("4 passed, 0 failed") | the test runner's own summary line | **no** — the canvas note says "NEEDS ENGINE: the pytest tally" |
| `runs[].durationMs` | int \| null | how long it took; null means "duration was not recorded" | | no |
| `runs[].capturedBy` | `thruline` \| … | who captured it ("captured by ThruLine") | capture source | partial |
| `runs[].declaresPaths` | bool | whether the check says which files it reads. False means it is treated as depending on everything the Change covers. | check profile | no — "declared paths are engine work" |
| `edits[]` | array | the edits that a ④ Since link points at. Every line of every edit is on What-Changed. | edit records | partial — `crates/core/src/hq/edits.rs` `FileEdit`; not routed |
| `edits[].id`, `.step`, `.sessionId`, `.time` | | the edit's identity and when it happened | | partial |
| `edits[].file` | string | the file it changed | | yes (`FileEdit.path`) |
| `edits[].makesVersion` | string | the version the code became | | no |
| `lastEditAt` | `HH:MM` | the agent's last edit in this Change ("the agent's last edit was 16:04"; "Bar set after the work") | max of edit times | partial |

### `criteria` — what counts as done

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `criteria.accepted` | bool | whether it has been accepted | `ChangeView.accepted_scope.acceptance` | yes |
| `criteria.version` | string (`v1`) | the scope version | `ScopeView.version` | yes |
| `criteria.acceptedAt` | `HH:MM` | when it was accepted | `AcceptanceView.accepted_at` | yes |
| `criteria.acceptedBy` | string | who accepted it ("you") | `AcceptanceView.actor` | yes (a mapping to "you" is needed) |
| `criteria.lines[]` | array, accepted order | the lines; the proof strip draws them in this order | `ScopeView.criteria[]` | yes (text only) |
| `.lines[].id` | string | row key | criterion index | yes |
| `.lines[].num` | int | line number (1-based) | criterion index + 1 | yes |
| `.lines[].gate` | bool | whether the line is a gate | check profile | unknown |
| `.lines[].text` | string | the line | `ScopeView.criteria[i]` | yes |
| `.lines[].link` | `person` \| `accepted_checks` \| null | how evidence was tied to the line: "You linked this yourself", "covered by the checks you accepted", or "nothing linked to this line yet" | `change_criterion_evidence.basis` | yes — `/api/changes/{id}/coverage`, links table |
| `.lines[].proof` | Proof | see Proof below | | partial |

### `messages[]` and `statements[]` — what the agents said

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `messages[]` | array | every agent message in this Change, as recorded | the transcript events | partial — the events table holds the text; no route serves it per Change |
| `messages[].id`, `.step`, `.time`, `.sessionId`, `.agent` | | where and when it was said | | partial |
| `messages[].text` | string | the full text, masked. The inspector rebuilds "❝a; b❞" or "❝a❞ ❝b❞" from where each statement sits in this text. | | partial |
| `statements[]` | array, served order | every checkable statement. A message no statement came from is listed as "Also said · not checked" and counts toward nothing. | claim extraction | partial — `/api/claims` (claims projection), `speaker_findings` |
| `.id` | string | statement id | | yes |
| `.messageId` | string | the message it came from | | unknown |
| `.text` | string | the statement's words, verbatim | | yes |
| `.claimKind` | `tests_pass` \| `lint_clean` \| `task_complete` … | what kind of claim. It chooses which re-check applies (tests sheet or lint sheet), or none. | claim classifier | partial |
| `.verifier` | bool | whether a run can check the claim by itself | | partial |
| `.step`, `.time`, `.sessionId`, `.agent` | | where and when it was said ("Claude Code · session 1 · step 52 · 16:03") | the message | partial |
| `.versionBefore` | string | the last version recorded before it was said ("The last version ThruLine recorded before this was said is …") | versions × step order | no |
| `.proof` | Proof | see Proof below | | partial |

### Proof — one six-link chain per line or statement, and its position

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `proof.verdict` | `not_proven` \| `needs_recheck` \| `recorded` \| `in_progress` \| `proven` | the state word and glyph. A missing chain is never Proven. | partial — the verifier / readiness vocabulary differs |
| `proof.stopsAt` | 1–6 | the link where the chain stops. The rack draws "⑤ RE-CHECKED" or "② RESTS ON"; the strip counts ×N per link. | no |
| `proof.restsOn` | `{kind:'run', runId, basis}` \| `{kind:'criteria'}` \| null | ② what it rests on: a run, what counts as done (for claims like "fixed" that no run can check), or nothing | partial |
| `proof.restsOn.basis` | `linked_by_person` \| `accepted_checks` \| `cited_by_agent` | why that run counts ("You linked this yourself." / "looked at because the checks you accepted cover this kind of thing" / "cited by the agent") | partial — `LinkBasis` |
| `proof.since` | `{editId}` | ④ the edit after the run that moved the code | no |
| `proof.lookedAt` | runId[] | when nothing rests on it: the runs ThruLine examined | no |
| `proof.misses` | string[] | the paths those runs did not include ("it did not include checkout/") | no |
| `proof.lookedAtSessions` | sessionId[] | for a claim no command could back: the sessions whose every command was examined | no |
| `proof.verb` | string | for a claim decided by what counts as done, the verb the record cannot check ("fixed", "refactored") | no |

How the chain is drawn follows from these fields. If `restsOn` is a run and
`since` is set, the chain holds ①–③, is hatched at ④ and stops at ⑤. If
`restsOn` is null, it stops at ② and ③–⑤ are not reached. If `restsOn.kind` is
`criteria`, ② is decided by what counts as done. ③ "Ran on" is
`runs[restsOn].version`, and its chip is that version's `recordedAtStep`.

### `recheck`, `selected`

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `recheck.runner`, `recheck.scope` | string, string[] | the one check that would supply the missing links ("pytest -q tests/test_totals.py checkout/") | partial — `/api/changes/{id}/run` |
| `recheck.on` | string | the version it would run on | partial |
| `recheck.permission` | `ask_once_per_project` \| … | "asks once for this project" | yes — `/api/changes/{id}/run/permission` |
| `recheck.reaches.lines` | int[] | the lines that one check reaches ("reaches all 4 lines", "Reaches lines 1–4") | no |
| `recheck.reaches.statements` | statementId[] | the statements it reaches; also which rows go to "checking now" | no |
| `recheck.elapsedS` | int | seconds the running check has run ("· 3 s", "You stopped this check after 3 s") | partial — the run execution state |
| `selected` | statementId | the row open in the inspector when the screen first draws. An unknown id falls back to the first row; it never invents one. | n/a — UI |

## What `renderVals` derives, so it is never stored

- **Counts:**
  - "N things still need evidence" is the lines whose verdict is not `proven`;
  - the tallies are criteria by verdict;
  - "×N" is criteria by `stopsAt`;
  - "N statements" is lines plus statements;
  - "Claude Code 2 · Codex 2" is statements by agent.
- **Grouping:**
  - "Whose turn" puts lines first, then statements.
  - "State" orders by verdict: Not proven, Needs a re-check, Recorded, In progress, Proven.
  - Within a group the served order is kept, and empty groups are not drawn.
- **Rack sentences:**
  - "same run (step 46)" when the line above in accepted order rests on the same run;
  - "shares step 46 with lines 1, 2, 4" for a statement whose run also backs lines.
- **The headline's second sentence:** "3 rest on step 46, which ran on 9f2ad3e before checkout/api.py changed at step 56. No run has included checkout/ for line 3."
- **"Looked at: 2 finished runs (…) and step 60, which has no result yet."** This comes from every run in `runs`.

## What the store does not have today, in short

- the test tally (passed/failed);
- run duration;
- the version each run ran on, and the version list itself;
- declared check paths;
- the proof chain's stop link, `since` edit, `lookedAt` and `misses`;
- the message → statement link;
- `versionBefore`;
- which lines and statements one re-check reaches;
- the gate flag;
- whether a sensor is recording now.

## Limits of the current markup (wave 2 must change the template, not the data)

- **"WHERE THE PROOF STOPS":** the strip draws exactly two groups, "⑤ RE-CHECKED" with three bars and "② RESTS ON" with one bar. Their ×N counts come from data, but the bar count and the link labels are fixed markup.
- **"Bar set after the work":** this chip is always drawn. The times in it come from data.
- **"Also said":** the template draws one row. The count comes from data; a second message would need a loop.
- **The lint sheet:** it says "either session". That fits two sessions only.
- **The `proven` state:** it has no drawing on this artboard. `logic.js` styles it as What-Changed draws Proven, so live data never meets an undefined state. This is not founder-reviewed.

## Wave 2 — on the real record (W2-C)

**Where the data comes from.**
- `index.jsx` reads the ONE Change snapshot through the shared hook `data/use-change.js`, which calls `data/change.js`.
- `from-change.js` (`everyStatementFromChange`) selects this screen's contract from that snapshot. It renames, picks and joins; it does not re-derive anything the snapshot holds.
- Nothing else is read.

**What the selector passes or decides.**
- **`runs`:** test runs only. A plain command is kept only when a proof names it (`restsOn`, `recheck.runId`, `lookedAt`).
- **`runs[].outcome`:** passed through. The engine's `succeeded|failed|unknown` decides "held" when no tally is served.
- **`statements[].proof`:** the snapshot's proof, unchanged. Two things are lifted out:
  - `checkedAgainst.misses` becomes `misses`;
  - `recheck.runId` becomes `recheckRunId`, the later run that did not hold.
- **`proof.verb`:** for a claim decided by what counts as done, read from the claim's own words (`fixed`, `refactored`, …), falling back to the kind.
- **`recheck.reaches`:**
  - `lines` is every accepted line;
  - `statements` is every test claim that is not proven and rests on a run.
- **`change.recording`:** stays `null` when the snapshot says nothing about it. The screen then says "Recording not known", never "Not recording".
- **`view.utcOffset`:** always `null` on the live path, so no offset is ever shown (HANDOFF §4).
- **No criteria accepted:** `criteria.lines` is `[]`. The screen says "What counts as done is not accepted yet". No line count, tally or "×N" is drawn as 0.

**Fixed slots that are now loops, built from the design's own row markup:**
- "Also said" draws one row per message that no statement came from (`alsoRows`).
- "Where the proof stops" draws one group per link a line stops at, most advanced first, with one bar per line (`stops`).
- The rack was already a loop over statements; it now holds every statement served.

**States the canvas did not draw, now drawn on the live path.** Each is marked in `logic.js`.
- **Contradicted:**
  - shown as ✕ coral, per DESIGN-SPEC §1.8;
  - the rack says "step N did not hold";
  - ⑤ names the later run that failed.
- **A run with no edit after it:**
  - the chain holds up to where the snapshot or the run stops;
  - ③ "Ran on" says "same version" or "N versions older";
  - ⑤ says "Not needed: the run is on V".
- **A long agent message:** the inspector shows the statement with one on each side, and "… N more in this message" on each cut side. It never cuts silently.
- **A long command:** in the one-line captions it is cut at 90 characters with "…". The sheet shows it whole.

**Controls (C30 R-RUN).** "Run it on V" runs the tracked Change's own check, through the one run client (`data/run.js` → `Repair-Loop/actions.js`).
- When the server would start it, the press opens the check sheet already running (`#/changes/<id>/check?run=1`): the press is the consent. The sheet reads the intent once and takes it out of the address before anything starts, so a reload or Back never starts a second run.
- When the server says it will not start one, the control is off and carries the server's own reason and what would let it run (`runActOf(...).blocked`).
- Work not tracked as a Change: the press opens the check sheet's ready form, which offers "Track this work as a Change" (C26 b).
- While a check of this Change runs (the run sheet's `job`, folded into the snapshot by `data/change.js` `withRunSheet`), the screen draws it running from the record, and "Stop this check" reaches `POST …/runs/<run>/cancel`.
- This screen holds no write address of its own.
