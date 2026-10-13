# Inbox: the data contract

`renderVals()` reads every piece of content from `const D = this.props.data || DEMO`
(`demo.js`). `text.js` formats it into sentences; `template.jsx` draws them. This
file lists every field of that object: its type and meaning, the real fact it
must come from, and whether the store serves that fact today.

**What the founder asked for, 2026-10-07:** the Inbox lists **every** Change (thread of
work) in the project, grouped by whose move it is.

That makes the following facts the heart of this contract:
- the projects list;
- one row per Change;
- the counts;
- the whose-move groups;
- the capture and recording status;
- the storage line.

The rest is detail for the selected row.

"Store today" was read from the code on 2026-10-07; no route was called.
- **yes**: a route serves the fact.
- **partial**: a route serves part of it, or serves it in a different shape.
- **no**: nothing serves it.

Routes are in `crates/cli/src/uiserve.rs`. `/api/hq/*` is the read layer that
`ui/src/hq/data/api.js` names. The server has served it since `5a01a45b` (see the Wave 2 section).

## Wave 2: where the live data comes from

`ui/src/hq/data/project.js` builds this object. `inboxData()` takes:
- `GET /api/hq/projects`;
- `GET /api/hq/projects/<id>`;
- `GET /api/storage`.

`index.jsx` reads the three through `useInbox()` in `data/use-project.js`. The current project is the one `?project=` names, else the most recently active one (`currentProjectId`).

**Every thread of the project is one row.** There is no cap other than the design's own collapse of "Nothing needed". Each group in `template.jsx` is a loop built from the canvas's own row markup for that kind.
- The canvas's first row of each kind is the loop's template.
- "Waiting on you" switches between the approve row and the prompt row.
- "Needs a check" switches between the canvas's ◭ Needs a re-check pill and proof bar, and its ○ Not proven pill and proof bar.
- The sidebar's projects are one loop over three dot variants: live, hollow and idle.
- The "Nothing needed" items are one loop over approved, dismissed and done rows, plus the destructive-command rows.

The gallery renders the same markup as before, and pixdiff stays at 0.00%.

| Thread (`ThreadSummary`) | Filed as | Group |
|---|---|---|
| `whose_move: you`, `whose_move_basis: agent_asked` | `changes[id]` (`move: you`, `state: asked`) + `prompts[]` (`asked: true`) | Waiting on you (prompt row) |
| `whose_move: you`, `edited_after_last_pass` non-empty and `last_passing_test_at` set | `changes[id]` (`state: recheck`, `changedFile`, `changedFiles`, stops at ⑤ RE-CHECKED) | Needs a check: ◭ Needs a re-check |
| `whose_move: you`, any other basis | `changes[id]` (`state: notProven`, `lastTestFailed`, stops at ② RESTS ON) | Needs a check: ○ Not proven |
| `whose_move: agent` | `changes[id]` (`move: agent`, `state: working`) + `sessions[]` | Recording now |
| `whose_move: done` | `nothingNeeded.items[]` (`kind: done`) | Nothing needed: "– Nothing open", collapsed |

The counts follow from that filing.

| What the screen shows | Equals |
|---|---|
| Your move: headline, filter, sidebar badge, footer | `project.whose_move.you` |
| The agent's | `project.whose_move.agent` |
| Nothing needed | `project.whose_move.done` |

`data/project.test.js` checks those equalities. On the shared scratch store, 2026-10-07: 144 threads gave 87 rows plus 57 in Nothing needed. Every title is in the markup.

Row text and links:
- **Title.** The thread's `title`, verbatim. It is masked by the engine.
- **Version chip.** `shortVersion(current_version_id)`: a 7-character commit, or `wt-` plus 7 hex for work after the last commit. A `wt-` id is never shown as a commit.
- **Time.** `last_activity_at` in the machine's local time: "HH:MM" today, "yesterday", or "5 Sep".
- **Changed file.** The first of `proof.edited_after_last_pass`, relative to the project root, with "and N more".
- **Proof bar.** The canvas's own bar for that state.
- **Links.** `#/changes/<thread id>?project=<project id>` (routes `brief`), and `…/check` for "Check it again", "Tell the agent" and "Copy it again". Every link on a row names the thread, whether or not a tracked Change has been read for it; the one address that names a tracked Change is the approve row's (below).

The live record does not hold these, so the screen leaves them out. It shows no zero and no placeholder.
- the criteria lines;
- the evidence count;
- the claim quote;
- the version thread graph;
- the check command;
- capture health;
- dismissed items;
- destructive commands.

The Inbox preview draws only what the selected row has.

## The approve row, and re-reading while an agent is recorded

**A Change that is ready for a decision is the approve row.** A thread has no readiness; a tracked Change does. The Inbox reads the project's open Changes (`GET /api/changes`, those whose `project_id` is one of the project's `members[].ledger_project_id`; the 20 most recently active), each one's `GET /api/changes/<id>/readiness`, and for each that answers `readiness_ready` with a `target_version`, the thread it stands for through the Change screen's own join (`resolveChange`, `data/use-change.js`). All of it is in `data/inbox-ready.js`; `inboxData({…, ready})` files it.
- The row is the thread's own row replaced: `state: 'ready'`, `move: 'you'`, the readiness's version, `href`/`approveHref` at the tracked Change (`#/changes/<chg>` and `…/approve`). A thread the engine calls finished becomes the person's move, so the Inbox's headline, chip, footer **and its sidebar badge** count it (the badge is the engine's `whose_move.you` plus those; other screens' badges read the engine's number alone).
- Two threads keep what they are: one the agent is working on now, and a question only the person can answer.
- `readiness_ready_with_exceptions` is not filed as plain-ready; a Change already approved for exactly that version is not waiting; a Change whose readiness read fails gets no row, and the header says `could not check N Changes for approval` beside the read time. Never a ready the record did not state.
- **Approved** (Nothing needed): a finished thread whose tracked Change has a standing approval for the version that Change is on now, that is, the approval's `target_revision` equals the Change's readiness `target_version`. A Change's target is a snapshot id and a thread's `current_version_id` is a commit or `wt-…`: two id spaces, never compared when the Change's own answer is known. The item names the approved `target_revision`, the same name its Approve row used.
- The record holds when the work was last active, not when it became ready, so the row says "last active"; it draws the canvas's lines only when the readiness names them, and never the six-link chain (not served).

**While something is being recorded the Inbox re-reads** its project view every 5 s (`RECORDING_REREAD_MS`): when a thread's `whose_move` is `agent`, or the project's `whose_move.agent > 0`, or `/api/lanes` says an agent session on this machine was written to (how a recording that starts after the page opened is noticed). It stops when nothing is, pauses while the page is hidden (one read on return), never starts a read while the last is running, and a failed re-read keeps the rows and the time of the last good read and says `could not refresh just now` beside it. (`data/use-polled.js`, `useInbox` in `data/use-project.js`.)

## The groups (the "whose move" rule)

The rule is the engine's, in API.md under "Whose move". This screen only files each thread into its group, as above.

The design's `DEMO` groups use the same contract:
- `changes` with `move` and `state`;
- `prompts[]`;
- `sessions[]`;
- `nothingNeeded`.

"Waiting on the agent" holds Changes with `noteCopiedAt`: a note was copied for the agent. On the live path the record supplies it (C30 R-LEDGER): a thread whose `told_agent` (`GET /api/hq/projects/<id>`, API.md) has `last_act` `copied` or `marked_pasted` and `later_work: false` is FILED here, at the act's own time (`noteCopiedAt`, with `toldAct` saying which act). Only the filing moves: the row keeps the engine's `move` (`you`), so the headline, the filter chips and the sidebar badge are still the engine's counts, and a filter shows the waiting rows of its own move only ("Your move" shows them; "The agent's" does not, because the engine does not say they are the agent's). A thread that is asking the person a question, or is being recorded now, is never filed here. Nothing was sent, and every sentence says so.

## Fields

### Top level

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `readAt` | string `HH:MM` | When this screen last read the record. Also "now" for retries and re-copies. | the time of the read, taken by the data layer | yes: the client clock at read; `/api/state.generated_at` for the export |
| `projects[]` | list | **Every project on this machine**, in sidebar order. Header: "3 projects · counted by Change". | `GET /api/hq/projects` | **yes** (wave 2): `{id, name, recording: whose_move.agent > 0, lastRecordedOn: last_activity, changeCount: threads}` |
| `projects[].id` | string | project id | project key | partial (same) |
| `projects[].name` | string | display name, e.g. `checkout-web` | folder label | yes: `candidates[].label` |
| `projects[].recording` | bool | Is a session recording in this project now? It drives "recording · …" and the header "Recording · N projects". | liveness of the project's sessions | **no** per project: `/api/lanes` gives only a machine-wide count of live files (5-minute window); `session_status::derive_all` is CLI-only. P6-IR-2. |
| `projects[].lastRecordedOn` | string, e.g. `5 Sep` | last recorded day, shown when not recording | the last event time of the project's sessions | yes: `candidates[].last_recorded`, or `sessions[].last_event_at` |
| `projects[].changeCount` | int | **how many Changes the project holds**: "recording · 4 Changes" | Changes per project | partial: `candidates[].changes`; also `/api/changes?project=` length |
| `projects[].dismissedCount` | int | items the person dismissed: "1 dismissed" | `attention_acknowledgments` | partial: the export index only (`attention.dismissed_hidden`); no route |
| `capture.claudeCode.on` / `.hookVersion` | bool / string | "Claude Code · hook v0.4" in Capture | the installed hook pack for this machine | **no** over HTTP: `thruline doctor --json` (CLI) checks the hook packs; nothing serves a hook version |
| `capture.runCapture.on` | bool | "Run capture on/off" | the run-capture shim is installed | **no** over HTTP: `doctor.rs` shim check (CLI only) |
| `capture.codex.answering` | bool | the Codex app server answers. The canvas draws the not-answering card only. | a Codex App Server client | **no**: no client exists ("Needs engine", a canvas note) |
| `storage.bytes` | int (bytes) | "ThruLine is keeping 1.4 GB of records on this machine" | what ThruLine keeps | **yes**: `GET /api/storage` → `bytes` |
| `countedBy` | `{change, needs[]}` | the Change named in the sub-headline, and why it has more than one row: `needs` from `check`, `prompt` | the your-move Change with more than one your-move row | derived (client), from `changes` and `prompts`; not served |

| `projectId`, `projectName` | string | the current project (live) | `ProjectView.project.id/name` | yes |
| `threadCount` | int | "every thread of work in thruline, 144 in all" | `ProjectView.project.threads` | yes |
| `whoseMove` | `{you, agent, done}` | the engine's counts; the screen's counts equal them | `ProjectView.project.whose_move` | yes |

Live-only per-row fields, all from `ThreadSummary`:
- `lastActive`
- `href`, `checkHref`
- `changedFiles`
- `lastTestFailed`
- `prompts[].asked`
- `nothingNeeded.items[].kind = 'done'`

### `changes`: every Change in view, keyed by id (`{[id]: Change}`)

The common fields of every Change:

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | Change id | `ChangeView.id` | yes: `GET /api/changes` |
| `title` | string | "Fix double-applied discount on checkout total" | `ChangeView.request.title` | yes |
| `shortName` | string? | "the discount fix", used in the sub-headline. Falls back to `title`. | none: a person's or the agent's nickname | **no** |
| `project` | string | project display name | `ChangeView.project_id`, joined to the project's label | yes (id); the join is the client's |
| `state` | `'ready' \| 'recheck' \| 'notProven' \| 'proven' …` | the Change's state word | readiness headline + coverage | partial: `/api/changes/{id}/readiness` → `headline_slug` (per Change, one call each). It is **not in the list response**. |
| `move` | `'you' \| 'agent' \| 'none'` | **whose move it is** (see the rule above) | derived from state + decision + told-agent | **no** |
| `version` | string (7 chars) | the version the state is about | the Change's current version | partial: `/readiness` → `target_version`; the list has only `implementation.base_revision` |
| `since` | `HH:MM` | when it entered this state | time of the state change | **no** as a field; derivable from the last event that moved the state |
| `agent` | string | the agent that worked on it | session harness | yes: `sessions[].harness` |

**The ready Change** (approve slot, `post`). It also has:

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `sessionEndedAt` | `HH:MM` | "session ended 15:48" | last event of its last session | yes: `sessions[].last_event_at` |
| `criteria.version` | string? | accepted-criteria version ("v1"); `null` hides it | accepted scope version | yes: `ChangeView.accepted_scope` / `/readiness.scope_version` |
| `criteria.lines[]` | `{n, state, gate}` | each line of "What counts as done", with its state (`proven`, `recheck`, `progress`, `notProven`) and whether it is the gate. Every "N of M" is counted from these. | coverage per criterion | yes: `/api/changes/{id}/coverage` → `criteria[]{index,state_slug}`; `/readiness.obligations[]` |
| `chain.links` | int | "6 links hold" | the proof chain per line | **no**: the six chain-link names exist only in the design |

**The re-check Change** (check slot, `disc`). It also has:

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `sessions` | int | sessions in this Change | `ChangeView.associations` | yes |
| `needEvidence` | int | "4 things still need evidence" | `/readiness.missing[]` length | yes |
| `changedFile` | string | the file whose change after the last run stopped the proof | a code-moved edit after the last passing run | partial: `ChangeView.code_moved` (detail route only) |
| `claim` | `{agent, quotes[], at, step}` | "Claude Code said ❝Checkout total fixed.❞ ❝Tests pass.❞ 16:03 · step 52" | the agent's completion claims | partial: `/api/claims?session=` → `claims[]{text,span.event_id}`. **No step number**: the step index must be computed from the event's position. |
| `restsOn` | `{lines, step, version}` | "3 rest on step 46, which ran on 9f2ad3e" | the run the lines rest on | partial: `/readiness.rests_on[]`; step number as above |
| `changedAtStep` | int | the step of the edit | the edit event's step | partial (as `claim.step`) |
| `uncovered` | `{scope, line}` | "No run has included checkout/ for line 3" | the line whose scope no run covered | partial: `/coverage` items |
| `criteria` | as above | 4 lines: re-check, re-check, in progress, gate re-check | as above | as above |
| `stops[]` | `{link, name, lines}` | where the proof stops: "⑤ RE-CHECKED ×3 · ② RESTS ON ×1" | the chain link each line stops at | **no**: there is no chain-link model in the engine |
| `thread[]` | `{version, now, ran?, events[]{step, kind: run\|claim\|edit\|command, result?, file?}}` | the lane graph "The thread, version by version", and its screen-reader sentence | the Change's versions and the steps on each | partial: versions from the record; steps need the step index (the `/api/hq/projects/{p}/threads/{t}` route is planned) |
| `check` | `{command, reachesLines, asks}` | the check command that would re-check every line, and how often it asks permission | the run plan + run permission | partial: `POST /api/changes/{id}/run` plan and `/run/permission` |

**The Change waiting on the agent** (agent slot, `stripe`). It also has:

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `noteCopiedAt` | `HH:MM` (or `HH:MM on 7 Oct`) | when the person copied a note for the agent, or said they pasted it | the thread's `told_agent.at` (`change_agent_messages`) | **yes** (C30 R-LEDGER) |
| `toldAct` | `copied` \| `marked_pasted` | which act it was; absent on the canvas's row | `told_agent.last_act` | **yes** |
| `laterWork` | bool | was later work seen after the note | `told_agent.later_work`; a row with later work is not filed here | **yes**, derived by the server at read |
| `stops[]` | as above | "② RESTS ON" | as above | no |

### `prompts[]`: commands waiting for the person's permission

These are "your turn" rows.

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `change` | Change id | the Change it belongs to | **no**: permission prompts are not captured. The PermissionRequest and Notification hooks are unsubscribed (canvas note; `guardian.rs:14-25`). |
| `agent` | string | "Your turn in Claude Code" | no |
| `command` | string | the command asked about | no |
| `effect` | `'removes-directory' \| 'removes-files'` | what it does. `text.js` turns it into "asks to remove a directory" and "It removes a directory." | **no** destructive-command classification exists |
| `step` | int | its step ("step 60 · no result yet") | no |
| `requestedAt` | `HH:MM` | when it was asked | no |
| `waitingMinutes` | int | minutes waiting ("6 min") | no (`readAt − requestedAt`) |
| `scope` | string | the scope that would settle the check if the command runs | no |

### `sessions[]`: sessions recording now

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `id`, `agent`, `project` | string | the session | yes: `/api/sessions` → `id, harness, repo_key` |
| `change` | Change id | "part of Fix double-applied discount…" | yes: `ChangeView.associations` |
| `startedAt` | `HH:MM` | "16:01 to now" | yes: `first_event_at` |
| `hookVersion` / `hookInstalledAt` | string / `HH:MM` | "hook v0.4", "the hook was installed at 16:01" | **no** |
| `steps` | int | steps recorded ("19") | yes: `event_count` (events, not steps; the definition of a step must be fixed) |
| `lastStepAt` | `HH:MM` | "Last step recorded at 16:04" | yes: `last_event_at` |
| `waitingAtStep` | int | the step with no result yet | partial: an unanswered tool call can be computed |
| *(recording itself)* | — | that the session **is** live | **no** per session (P6-IR-2). Until then, the canvas says the section reads "Recent". |

### `nothingNeeded`

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `items[]` (`kind: 'approved'`) | `{project, title, version, by, on}` | "◆ Approved 5a91c3e checkout-web › Round line-item prices to cents · by you, yesterday" | yes: `ChangeView.decisions[]` (`change_decisions`: actor, time, target version) |
| `items[]` (`kind: 'dismissed'`) | `{project, state, notProvenStatements}` | "○ Not proven · dismissed · demo › 1 statement not proven" | partial: `attention_acknowledgments`, export index only |
| `destructive[]` | `{command, change, at}` | "1 command that removes files ran: rm -rf .venv · Upgrade Stripe SDK to v14 · 15:58". Recorded, not judged; never counted. | **no** classification |

## What the store does not have (the Wave 2 / engine list)

1. **Whose move** per Change (`move`), and the state word in the Changes list (`state`, today one `/readiness` call per Change).
2. **Per-project and per-session liveness** (`projects[].recording`, the Recording now section). P6-IR-2.
3. **Permission prompts** (`prompts[]`): not captured.
4. ~~**"Told the agent"** (`noteCopiedAt`, the Waiting on the agent group): no event.~~ Now recorded (C30 R-LEDGER, `change_agent_messages`; write address 24). The Inbox holds no message text, so its "Copy it again" cannot copy here: on a row from the record it opens that thread's Repair-Loop sheet (`tellHref`), where Copy message copies the draft and keeps the act, rather than claim a copy that did not happen; the gallery's canvas row keeps its local toggle.
5. **Destructive-command classification** (`prompts[].effect`, `nothingNeeded.destructive`).
6. **Capture health over HTTP**:
   - the hook version;
   - run capture on or off;
   - Codex app-server status.

   Today these come from the CLI `doctor` only.
7. **Step numbers** for claims, runs and edits:
   - claims carry only an `event_id`;
   - nothing serves a step index.
8. **The proof chain**:
   - the link a line stops at (`stops[]`);
   - `chain.links`.
9. **A dismiss route**, and dismissed counts outside the export.
10. **`/api/hq/*`**: served since `5a01a45b` (docs/hq-next/API.md). This item is closed; it is kept so the list's numbers stay stable.
11. **`shortName`**: no nickname for a Change; `title` is the fallback.

## Design annotations (gallery only)

These render only when `this.props.data` is absent (`v.isDemo`):
- the "Demo records." banner;
- "Needs engine: no Codex App Server client exists yet.";
- "needs engine: prompt capture";
- "needs engine: kept in this browser only";
- "needs engine: reads “Recent”…";
- the three "Needs engine." paragraphs in the previews;
- the two "Illustrative …" paragraphs.

`inbox-data.test.js` checks two things:
- every such literal in `template.jsx` sits under a `v?.isDemo` gate;
- the live path renders none of them.
