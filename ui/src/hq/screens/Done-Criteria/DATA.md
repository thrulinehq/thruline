# Done-Criteria — data contract

This is the Define stage: "What counts as done?". The person accepts, edits or drops each proposed line, chooses what the Change covers, and records a version. Every piece of content arrives through one object: `this.props.data` (live) or `DEMO` (`demo.js`, the design gallery). `logic.js` `renderVals()` builds every sentence, glyph and colour from it.

`this.state` holds UI state only:
- each line's decision while it is being made, which starts from `criteria.lines[].state`;
- candidate links, from `criteria.lines[].linked`;
- the open editor, the composer and templates panel, the folder choices (from `covers[].included`), and the face (the altitude is not state: Brief, Explore and Workbench are links to the Change's three views, C30);
- (live, C30) the lines the person added (`added`: `{text, state, template?}`) and what they marked out of scope (`outOfScope`): a draft, written only by the accept act;
- the version number being drafted, from `criteria.version`.

In the gallery the demo switcher loads one of `DEMO.states` (Fresh / Reviewing / One dropped / v1 recorded). Live data carries those facts on `criteria`.

Times are local wall-clock `HH:MM` strings, formatted by the data layer.

## Fields

| Field | Type | Meaning | Must come from | Store has it |
|---|---|---|---|---|
| `project.name`, `change.title` | string | crumb | Change | yes: `/api/changes/<id>` |
| `version.id`, `.commit`, `.clean` | hash, hash, bool | crumb "e71c5b0 (current) · = commit 3f9a1c2 · no unsaved changes" and every "on e71c5b0" | implementation | id/commit yes; `clean` no |
| `steps` | int | "Build 19 steps", status bar | Change event count | yes: `/api/hq/.../threads/<tid>` |
| `readAt` | HH:MM | "read 16:04 ↻" before acceptance | response time | yes |
| `recordingSince` | HH:MM | "ThruLine began recording at 16:01" | first captured event | yes |
| `request.text/.by/.at/.step` | string, string, HH:MM, int | "You asked" and "Your request" | prompt event | yes |
| `agent.name` | string | "Claude Code said" | session source | yes |
| `agent.lastEdit.at/.step` | HH:MM, int | "Agent's last edit 16:04 · step 56"; the bar-set-after-the-work rule | last edit event | yes: `retrospective.work_recorded_at` |
| `claim.text/.at/.step` | string, HH:MM, int | the claim the screen asks a bar for | claim row | yes: `/api/claims` |
| `claim.word` | string | the claim's verb quoted back ("“fixed”", "“Fixed” now points at") | the claim's predicate | no: needs engine (claim parse) |
| `notCheckable.quote`, `.shownBy[]` | string, int[] | "“fix it” — no command can observe it. Lines 1–3 are what would show it." | the proposer's not-checkable residue | no: needs engine |
| `criteria.version` | int | v1, v2 … | scope version | yes: `accepted_scope.version` / `/proposals` |
| `criteria.recorded`, `.acceptedAt`, `.acceptedBy` | bool, HH:MM, string | whether this version is accepted, when, and by whom | scope acceptance | yes: `accepted_scope.acceptance` |
| `criteria.lines[].n`, `.text` | int, string | each proposed line | `GET /api/changes/<id>/proposals` | yes |
| `criteria.lines[].kind` | `'behaviour' \| 'gate'` | grouping ("Behaviour · 3", "Gate · 1"), and the wording of its plan | proposal kind | no: criteria are plain strings today |
| `criteria.lines[].source` | `{kind:'words'\|'name'\|'check', quote?, steps?}` | "from your words “…”", "from the Change's name “…”", "a check that must pass" | proposal provenance | unknown (`/proposals` carries a source; per-line quote: no) |
| `criteria.lines[].check` | string | the command that would prove the line | accepted check profile | yes: `accepted_scope.check_profile` |
| `criteria.lines[].namedTest` / `.ranAtSteps[]` | string / int[] | "named test test_single_discount" / "steps 22, 42" | per-line test mapping / run steps | no: "Named tests per line: needs engine" |
| `criteria.lines[].evidence` | `'candidate' \| 'looked_at' \| 'check'` | what the record holds for the line: a run that could be linked; runs looked at that did not include its scope; or the check itself | `/coverage` + `/applicability` | partly: coverage yes, candidate classification unknown |
| `criteria.lines[].state` | `'proposed' \| 'accepted' \| 'dropped'` | the person's decision | scope acceptance (accepted vs not in the version) | accepted yes; **`dropped` no**: the record has no row for a drop without DDL (C30), so live says "Dropped · stays out of vN", never "recorded" |
| `criteria.lines[].proposal` | int \| null | the proposal number a line is accepted by; `null` for a line of the standing version (kept by its words when the version is changed) | `/proposals` | yes |
| `criteria.lines[].linked` | bool | a person linked the candidate run | `change_criterion_evidence` basis `person` | yes |
| `criteria.lines[].provenOn` | hash (optional) | the version the line is proven on, for "0 proven on e71c5b0" | coverage on the current version | yes: `/coverage` |
| `recheck.command` | string | "One re-check reaches every line: …" | the check profile's widest command | derivable from `check_profile` |
| `lastRun` | `{startStep, step, command, file, version, exit, passed, failed, capturedBy}` | step 42 → 46, the candidate run | exec record | yes, except pytest tally (no) |
| `earlierRun` | `{startStep, step, version, exit, passed, failed}` | step 22 → 26 | exec record | yes, except tally |
| `pendingRun.step` | int | "step 60 has no result yet" | exec record without an end | yes |
| `movedEdit.file`, `.step` | path, int | "checkout/api.py changed at step 56" | edit after the run | yes |
| `uncovered` | path | "No finished run has included checkout/" | scope minus runs' scope | derivable from `/applicability` |
| `covers[]` | `{path, editSteps[], holdsCheck?, included}` | What the Change covers: folder chips, notes, and the conservative "Depends on" rule | edits grouped by folder; `allowed_scope` | partly: edits yes; folder suggestion no ("per-line file lists: needs engine") |
| `covers[].editSteps` | int[] | the steps of the recorded edits under a folder, matched for both `src/` and the served `src/**` | edits | yes |
| `outOfScope[]` | string | what the standing version marks out of scope | `accepted_scope.exclusions` | yes |
| `templates[]` | `{name, from, lines[]}` | Templates from Rules | `GET /api/hq/projects/<id>/done-templates` (`templates.js`): `.thruline/policy.yml` in the project's folder | the read and templates, yes: the base-branch policy supplies named lines when its templates section is present; `templatesNote.why` says when none can be offered |
| `templatesNote` | `{why, detail}` | the panel's sentence: no file, a file with none, a file ThruLine could not read | the same read | yes |
| `stages[]` | `{id, status}` | the six-stage strip's glyphs (Define is this screen's own) | the one Change model (`data/change.js` `change.stages`) | yes |
| `act.project`, `act.thread` | string | what "Track this work as a Change" names | the snapshot | yes |
| `track`, `act.accepting` | object | set by `index.jsx`: untracked / reading / busy / the refusal; the accept act's answer (busy, the refusal in the server's words, the version written) | the screen | n/a |
| `storage.bytes` | int | "1.4 GB" | `GET /api/storage` | yes |
| `demoTimes.accept` | HH:MM | the gallery's acceptance time (16:06). Live uses `criteria.acceptedAt` | design gallery only | n/a |
| `states.*` | `{lines[], links[], recorded}` | demo variants | design gallery only | n/a |

## Controls that write, and the route each maps to (wave 2: wired)

Both writes live in `actions.js`, as named constants beside the one function that uses each. `done-criteria-live.test.js` checks that no other file here posts, and that exactly these two addresses are reached.

| Control | Address | Live behaviour |
|---|---|---|
| **Accept what counts as done (vN)** | `POST /api/changes/<id>/scope/accept` with `{criteria: [n…], edits: {n: wording}, lines?: [text…], exclusions?: [text…], allowed_scope: [folder…], actor: 'you', replacing?}` | Sends the proposals the person accepted by number with the wording they changed, every other accepted line (one they added, or one of the standing version they keep after "Change it") by its words, what they marked out of scope, and the folders they kept. Dropped and still-proposed lines are not named. `replacing` is set when the person pressed "Change it". The bar says the answer: "Recording…", the refusal in the server's words (whole in the line's title), or "Recorded as vN. Reading it back…" until the record shows it; the draft is cleared once a version is written. |
| **Track this work as a Change** | `POST /api/first-use/change` `{project, thread}` (write address 14), through `Repair-Loop/actions.js` `trackThenReread` | Drawn where Accept would be, only for work not tracked as a Change; then the Change's proposals are read. Accepts, approves and sends nothing. |
| **Accept / Edit / Drop / Restore / Save / Add a statement / Add to out of scope / a template / folder chips** | none: they change this screen's draft only | They are written by the one accept act. Off, with the reason, for untracked work. Changing a recorded version keeps every folder it covers (the accept act only adds folders). |
| **Link step N / Unlink** | `POST /api/changes/<id>/links` (write address 4), `{criterion, kind: 'check_run', evidence, act, actor: 'you', reason: ''}` | Offered only on an accepted line the engine reads as having a passing run nobody linked (`coverage-view.js` `unlinkedCompletedRun`). Never offered before the version is accepted. |
| **Start from a template** | `GET /api/hq/projects/<id>/done-templates` (read) | Each template row adds its lines as proposals (a line already in the list is not added twice); the panel says what the policy file holds. |

## Wave 2: where the live data comes from

`from-change.js` `selectDoneCriteria(snapshot, tracked)` builds this screen's data. Its sources:
- **The ONE Change snapshot** gives the request, the claim the bar is asked for, the runs, the edits, the recheck command and the storage size.
- **The tracked Change** gives:
  - the proposed lines (`proposals.criteria`) and the kept note (`proposals.notes[0]`);
  - the accepted version, each line's engine state and sentence (`deriveChangeModel` `criteria[].state/.line`), and `accepted_scope.allowed_scope`;
  - the unlinked passing run.
- **Folders.** Without a tracked Change, the folders shown are the recorded edits grouped by their top folder.

Live only:
- What is recorded comes from the record on every read. The screen's draft stays in state until it is accepted.
- A recorded line draws the engine's state in the design's marks: ● Proven, ◭ Needs a re-check, ◐ In progress, ○ for the rest.
- "Bar set after the work" is drawn only when acceptance came after the agent's last edit.
- The "Not checkable as written" block is drawn only when the proposals kept a note.
- A long request is cut to 180 characters in "You asked".

## Design annotations (gallery only, `v.isDemo`)

These render only when `props.data` is absent:
- the `Demo state` switcher;
- the "needs engine: shown as designed" tag on Templates;
- the "Design note · engine status" section with its divider;
- the canvas's "Adding here isn't built yet (needs engine)" wording. Live says it in plain words (above).
