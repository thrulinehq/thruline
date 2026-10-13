# Repair-Loop — data contract

The screen is the two sheets that open from where a statement's proof stops: **Check it again** (left) and **Tell the agent** (right), over the Change behind them. Every piece of content arrives through one object: `this.props.data` (live) or `DEMO` (`demo.js`, the design gallery). `logic.js` `renderVals()` builds every sentence, glyph and colour from it. `this.state` holds UI state only: the demo variant, copied/pasted flags and the user's edits to the draft.

Times are local wall-clock `HH:MM` strings. The data layer formats them from the record's timestamps; the screen never formats a timestamp itself.

"Store has it" means: is the fact served today, and by what. **yes** cites the route or table. **no** means a design hint says "needs engine", or nothing serves it. **unknown** means not verified in this wave.

## Fields

| Field | Type | Meaning | Must come from | Store has it |
|---|---|---|---|---|
| `project.name` | string | project shown in the crumb, behind-header and consent note | the Change's project | yes: `GET /api/changes/<id>` `project_id` → `/api/hq/projects/<id>` |
| `project.dir` | string, home-relative path | where the check runs ("runs in ~/checkout-web") | the project's recorded worktree | unknown (`implementation.workspace_identity` may carry it) |
| `change.title` | string | crumb title | the Change's request | yes: `GET /api/changes/<id>` `request.title` |
| `version.id` | string, short hash | the current version under review | the Change's current version | yes: `/api/changes/<id>` `implementation`/`code_moved.now` |
| `version.commit` | string, short hash | the commit that version equals | the same | yes: `implementation.base_revision` |
| `version.clean` | bool | true when there are no unsaved changes. The design draws only the true case ("no unsaved changes"); when false the clause is omitted, and Wave 2 owes the copy | worktree status | no: `CodeMoved` says uncommitted edits are "not compared" |
| `session.day`, `.start`, `.end` | string, string, HH:MM | when the session behind started ("today 16:01"); `.end` is not drawn (T10, t10.4: no composed clock-time range; the canvas's "today 16:01–16:09" is the one intended difference) | the session's first and last event | yes: `/api/hq/sessions/<sid>` |
| `agent.name` | string | "Claude Code". Used in the chain's ① and in "Message for Claude Code" | the session's agent | yes: session source (Claude Code / Codex) |
| `request.text` | string | the request quoted on the Change behind | the prompt the Change opened from | yes: `request.prompt_event_id` → event |
| `statement.text` | string | the statement whose proof stops ("Tests pass.") | the claim row | yes: `/api/claims` |
| `statement.said` | string | the whole sentence the agent said | the claim's source line | yes: `/api/claims` |
| `statement.step`, `.at` | int, HH:MM | where and when it was said | the claim's anchor event | yes |
| `statement.lastVersionBefore` | short hash | "The last version ThruLine recorded before this was said is …" | version history at the claim's time | unknown |
| `heldRun.step` | int | the run ② rests on until a re-check lands | the run the claim cites | yes: exec record / `/api/explain` |
| `heldRun.command`, `.version` | string, short hash | its command and the version it ran on | the run's execution record | yes |
| `heldRun.versionsBehind` | int | how many versions older than the current one ("one version older") | version history | no |
| `heldRun.exit` | int | its exit code | the execution record | yes |
| `heldRun.passed`, `.failed` | int | its test tally | the runner's tally | no for pytest: today only `cargo test` is counted ("Counting pytest results … needs engine") |
| `movedEdit.verb`, `.file`, `.step` | string, path, int | the edit after the run that moved ④ | the edit event after `heldRun` touching the claim's scope | yes: edit events; the match to the claim is `/applicability` |
| `check.scope[]` | path[] | the scope chips ("tests/test_totals.py", "checkout/") | the check's declared scope from the accepted profile | yes: `GET /api/changes/<id>/run` (run sheet) |
| `check.covers` | path | the scope entry that covers the moved file | scope × `movedEdit.file` | derivable |
| `check.command` | string \| null | the command that would run, shown and copied | the run sheet's check when it names one (what the button starts), else the command the statement rested on. Null when the record names none: the row reads "Not recorded" and offers no Copy (C30, census Repair-Loop 5) | yes: `GET /api/changes/<id>/run` |
| `check.terminalCommand` | string | the `thruline check …` line for the ready and blocked forms | `terminal.js`: `thruline check --for-change <id> -- <runner> <scope…>` (C30 P0-4); `--shell` and one quoted word when the command holds `&&`, a pipe or a redirection; no `--for-change` when the work is not tracked. `terminal-command.json` holds the examples `crates/cli/tests/repair_loop_terminal_command.rs` runs | derivable |
| `check.environment` | path | "~/checkout-web/.venv" | the run sheet's environment id | yes: `GET /api/changes/<id>/run` |
| `check.consentGranted` | bool | "allowed for checkout-web" when true. The false copy is not drawn; it renders nothing | the run permission | yes: `run_route` `permission()` |
| `concern.raisedAt` | HH:MM | when the stop at ⑤ was first read | the concern's first derivation | no: concerns are not a recorded event |
| `ladder` | `{raisedStep, later: number[], sinceStep, changed: {step, file}[]}` \| null | the repair ladder's rungs 2 and 3: "Later work seen" (the agent's own steps after the statement, or after the edit that moved its proof when that came later; the statement's own message and checks ThruLine ran are not counted) and "What changed" (the files edited after the edit where the proof stops). None is said as none; a later step is never said to answer the concern | `from-change.js`, from the snapshot's edits, runs and statements (C30, census Repair-Loop 14) | yes: the thread's own rows |
| `criteria.version`, `.lines` | string, int | "all 4 things that count as done (v1) rest on this run" | the accepted scope | yes: `accepted_scope.version`, `criteria.length` |
| `record.statements`, `.notProven` | int | status-bar counts | the Change's claims and their states | yes: `/api/claims` + `/coverage` |
| `storage.bytes` | int | "ThruLine is keeping 1.4 GB" (L1.1) | the store's size | yes: `GET /api/storage` |
| `outcomeExamples.passed/failed` | `{passed, failed}` | illustrative tallies for the Honest-outcomes rows that did not happen. The row that did happen uses the run's own tally | none: a design illustration | no. Wave 2 owes a decision: generic row words, or the last known collection count |
| `readAt` | HH:MM | "read 16:09 ↻" | the time this read was served | yes: response time |
| `now` | HH:MM | the time put on "Copied — not sent · 16:10" | the clock when the person acted | client clock |
| `steps` | int | status-bar step count | the Change's event count | yes: `/api/hq/projects/<id>/threads/<tid>` |
| `run.state` | `'running' \| 'passed' \| 'failed' \| 'stopped' \| 'unavailable'` | which of the sheet's five forms draws | the run sheet's run going now (`job`, phase starting or running), else its last end when that run started after the last edit the record holds (passed, failed, cancelled), else the snapshot's re-check; `unavailable` is the ready form when the server can start one, and the blocked form with the server's reason when it cannot | yes: `GET /api/changes/<id>/run`, read again every second while running (`data/run.js`) |
| `act.canStart`, `.blocked`, `.stopBlocked`, `.lastWords`, `.refusal`, `.busy` | bool, `{reason, enabler}`, `{reason, enabler}`, `{kind, current, sentence}`, string, bool | whether ▶ Check it again is on; the server's reason it is not (run sheet `primary.blocked`, in `HQ_COPY.run.blocked`'s words); why Stop is off while starting; the last end the run form does not draw (checked nothing, tool not found, interrupted, or from before the latest edit), in `outcomeView`'s words; the last press's refusal in `HQ_COPY.run.refusals`' words (or the cancel route's own `plain`); a press being answered | `from-change.js`, from the run sheet; `refusal` and `busy` from `index.jsx` | yes: run sheet |
| `run.version` | short hash | the version the check runs on | the execution record | yes |
| `run.elapsedS` | int seconds | "0:03", "3 seconds in", "after 3 s" | while running, the run sheet's `elapsed_s` (C30 R-RUN: whole seconds from the run's own recorded start, `data/run.js` `liveOutputOf`); when the sheet serves none (still starting, or a server from before R-RUN), now minus the job's `started_at`. An ended run: `recorded_at` minus `started_at` | yes: run sheet |
| `run.step`, `.at` | int, HH:MM | "recorded as step 62", "at 16:09" | the session step; a run started from the sheet is an execution record, not a session step, so its `step` is null and the sentences say "this check" | `at` yes; `step` only for a run in a session |
| `run.exit`, `.durationS` | int, number | "exit 1 · 0.44 s" | the execution record | yes |
| `run.passed`, `.failed` | int | the tally | the runner's tally: for a run from the sheet, the wrapper's typed count (`tests_passed`, `tests_failed`), read from libtest's `test result:` lines and, since C30 R-RUN, from pytest's own summary line (`check.rs` `parse_pytest_summary`); null when the record holds no count, and then no number is drawn | yes: run sheet `checks[].last` |
| `run.output[]` | string[] | what the check printed, line by line, without the `$ command` echo | while running: the newest `RUNNING_LINES` (12) of the lines the run sheet serves (`job.output.lines`, C30 R-RUN: masked on the server, newest last); an ended run from the sheet: its recorded failure line | yes: run sheet |
| `run.outputServed`, `.outputCut` | bool, bool | while running, the log's one plain line under the command: "ThruLine records what this check prints. It is not shown while the check runs." when the sheet serves no output (a check started from a terminal, or a server from before R-RUN); "Nothing printed yet." when it serves none yet; "Earlier lines are not shown." when the server cut its tail or more lines were served than the log draws | `from-change.js`, from the run sheet's `job.output` | yes: run sheet |
| `run.failures[]` | `{line, test, assertion}` | which output lines are a failure (highlighted, quoted in the draft) | parse of the runner output | no: needs engine |
| `run.cleanupVerified`, `.cancelledRecords` | bool, int | "cleanup verified · 1 cancelled record added" | the cancel record | yes: `POST …/runs/<run>/cancel` appends it |
| `states.{running,passed,failed,stopped,na}` | object | demo variants: `readAt`, `now`, `steps`, `run` | design gallery only | n/a |

## Controls that write, and the route each maps to (wave 2: wired)

Every write lives in `actions.js`, as a named constant beside the one function that uses it. `repair-loop-live.test.js` checks that no other file in this folder posts, and that `actions.js` reaches exactly the three addresses below.

| Control | Address | Live behaviour |
|---|---|---|
| **Check it again** | `POST /api/changes/<id>/run/permission` (the first time in a project), then `POST /api/changes/<id>/run` with `{check}`. Both carry `X-ThruLine-Run-Token`, read from `GET /api/run/token`. | On only for a tracked Change whose run sheet offers a check (`runSheetModel` primary act `run` or `allow-run`). Otherwise off, with the design's own sentence: "Running the check again is not something this version of ThruLine can start for you." The terminal command is offered instead. |
| **Stop this check** | `POST /api/changes/<id>/runs/<run>/cancel` (write address 3), `{actor}` | On only when the run sheet names the running run. Otherwise off: "It was not started here, so it can only be stopped where it runs." |
| **Copy** / **Copy command** | none: clipboard only | |
| **Copy message** | the clipboard, and `POST /api/hq/projects/<id>/threads/<tid>/told-agent {act: copied, message}` (write address 24): the words the person left in the draft, kept as written (C30 R-LEDGER); nothing is sent | reads the record's last act |
| **Mark as pasted** · undo | `…/told-agent {act: marked_pasted}`, and `{act: unmarked_pasted}` for the undo, each naming the row the screen saw (`refers_to`); nothing is sent | reads the record's last act |
| **Approve <version>** (link) | opens Approve-Sheet | |

## Wave 2: where the live data comes from

`from-change.js` `selectRepairLoop(snapshot, tracked)` builds this screen's data:
- **snapshot** is the ONE Change snapshot from `data/change.js`, read by `data/use-change.js`;
- **tracked** is the tracked Change's run sheet, read through `live.js` → `readChangeDocuments` + `GET …/run`. It is read through `runSheetModel` and never re-derived.

The statement repaired is the Brief's (`snapshot.brief`) when its proof rests on a run, else the first statement whose proof stops. ② is the run that `thenAndNow.noLongerCounts` names, else `restsOn`. ④ is `since`. ⑤ is a running run on the current version, else the run the proof now rests on.

Live only:
- **Honest outcomes.** A tally row ("N passed, M failed") is drawn only when the record holds a run of that kind on the current version. The four kinds with no number always stay.
- **No canvas examples.** `outcomeExamples` is the record's own, never the canvas's.
- **The statement in the stop strip and the sheet header** is cut to 18 characters with "…". It is quoted whole in the concern and the draft.
- **Absences** read in the design's language: "nothing yet", "No result yet ⋯", "Not recorded", "From the record" (for the live pill when the snapshot cannot say capture is live).

## Design annotations (gallery only, `v.isDemo`)

The `Demo state` switcher, the per-state "Design note · …" under Honest outcomes, and the "Design note · “Told the agent” …" paragraph render only when `props.data` is absent.
