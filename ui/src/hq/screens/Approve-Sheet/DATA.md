# Approve-Sheet — data contract

This is the approval sheet "Approve e71c5b0", drawn over the Change screen and its trace map. Every piece of content arrives through one object: `this.props.data` (live) or `DEMO` (`demo.js`, the design gallery). `logic.js` `renderVals()` builds every sentence, glyph and colour from it. `this.state` holds UI state only: whether the sheet is open, the name and note being typed, the run row being expanded, a refusal just shown, the copy result, and the history of acts pressed in this view, which starts from `data.decisions`.

In the gallery the demo switcher lays `DEMO.states.ended` (licence) and/or `DEMO.states.moves` (code moved) over the base. Live data carries its own `licence` and `moved`.

Times are local wall-clock `HH:MM` strings, formatted by the data layer.

## Fields

| Field | Type | Meaning | Must come from | Store has it |
|---|---|---|---|---|
| `project.name` | string | crumb | Change's project | yes: `/api/changes/<id>` → `/api/hq/projects/<id>` |
| `project.worktree` | path | where `.thruline/decisions/<v>.json` is written ("in ~/code/checkout-web") | the project's recorded worktree | yes: the decision route writes there (CLAUDE.md, T-S-4) |
| `change.title` | string | crumb, `main` label | `request.title` | yes: `/api/changes/<id>` |
| `change.sessions` | int | "Change · 1 session" | the Change's associations | yes: `associations.length` |
| `version.id`, `.commit` | short hashes | the version being approved, and its commit | the current implementation | yes: `implementation` / `code_moved` |
| `version.fullId` | string or null | not drawn: the served version id the decisions are compared with (C29) | the snapshot's version row `fullId` | yes: `GET /api/hq/projects/<p>/threads/<t>` `current_version_id` |
| `version.clean` | bool | "no unsaved changes". When false the clause is omitted (copy owed) | worktree status | no (`CodeMoved.uncommitted` is always false) |
| `session.day/.start/.end` | string, HH:MM | "today 16:01" — the start only; `.end` is not drawn (T10, t10.4: no composed clock-time range; the canvas's "today 16:01–16:09" is the one intended difference) | when the session started | yes: `/api/hq/sessions/<sid>` |
| `agent.name` | string | "Claude Code said", "Only you can answer it, in Claude Code" | session source | yes |
| `agent.lastEditAt` | HH:MM | "the agent's last edit was 16:04" (Bar set after the work) | last edit event | yes: `retrospective.work_recorded_at` |
| `readAt` | HH:MM | "read 16:09 ↻" before any act | response time | yes |
| `readyAt` | HH:MM | "At 16:09 this read: Ready for approval" | when readiness last turned Ready | no: readiness is derived at read time, not recorded |
| `request.text`, `.step` | string, int | request card in the trace map | prompt event | yes |
| `statements[]` | `{text, step, at, proof}` | what the agent said. `proof:'proven'` is the "Tests pass." row; `'recorded'` is the "Checkout total fixed." row, which is decided by what counts as done | claims and their states | yes: `/api/claims` + `/coverage` |
| `criteria.version` | string | "v1" | accepted scope version | yes: `accepted_scope.version` |
| `criteria.acceptedAt`, `.acceptedBy` | HH:MM, string | "accepted 16:06 by you" | the scope acceptance | yes: `accepted_scope.acceptance` |
| `criteria.setAside` | int | "0 set aside" | waivers / not-applicable | yes: `change_obligation_waivers` |
| `criteria.lines[]` | `{n, text, kind, link, state}` | the lines the approval rests on. `kind` is `'behaviour'` or `'gate'` on the canvas; live it is `'criterion'`, because the record types an accepted line no further. `link` is `'person'` ("You linked this yourself."), `'accepted_check'` ("The check you accepted."), `'named'` ("What ran names what this line is about.") or null ("Nothing is linked to this line yet."), read from the Change model's evidence items (`from-change.js` `linkOf`: basis `manual`, `declared`, `named`; a "does not bear" link is not something it rests on). `state` is `'proven'` | accepted criteria, each with its evidence link | partly: text yes (`accepted_scope.criteria`); link basis yes (coverage `items[].basis`, C30 census Approve-Sheet 5); `kind` (behaviour vs gate) **no**: no Gate badge is drawn live |
| `run.step/.command/.scope[]/.version/.exit/.durationS/.at/.cwd/.capturedBy` | mixed | the run every line rests on, and its detail row | the execution record | yes: exec record (run capture must be on) |
| `run.passed`, `.failed` | int | "5 passed, 0 failed" | runner tally | no for pytest (needs engine) |
| `trace.lanes.{request,work,execution,claims}` | int | lane headings in the trace map | counts of each lane's nodes | yes: `/api/hq/projects/<id>/threads/<tid>` |
| `trace.versions[]` | short hash[] | the earlier version bands | version history | unknown |
| `trace.edits[]` | `{step, file}` | "File changed" cards | edit events | yes |
| `trace.runs[]` | `{step, passed, failed, exit}` | "Run" cards | execution records | yes; tally no for pytest |
| `pending.step`, `.command` | int, string | a command with no result yet ("Still open, and it doesn't hold this up") | a run without an outcome | yes: exec record with no end |
| `reviewer.name` | string | the name the box starts with | none is recorded; the design pre-fills "Sam" | no: needs engine (OS user / git user.email on the decision row) |
| `decisions[]` | `{kind:'approved'\|'withdrawn', time, name, note}` | decision history on this version | `change_decisions` rows | yes: `/api/changes/<id>` `decisions` |
| `demoTimes.approve/.withdraw` | HH:MM | the gallery's clock for acts pressed in the demo (16:10, 16:14). Absent live; live acts take the record's time (shown "just now" until re-read) | design gallery only | n/a |
| `licence.plan`, `.approving` | `'pro' \| 'free' \| null`, bool | Free or Pro, and nothing else (CTO C21-b): approving a version needs Pro. `null` = the licence was not read; the chip then says "Plan not read" and Approve says why, in the licence page's words | `GET /api/licence`, through the ONE plan rule `ui/src/hq/data/plan.js` (C30 P0-6), which Settings and Team-View call too: Pro = `status === 'valid'` AND the server's own `entitlements.approve_version.allowed === true`. A document without that explicit answer is not read as a plan. Every trial field is ignored. | yes |
| `moved` | `{version, files[], step, at}` or null | the code moved while the sheet was open: refusal, re-check strip, approval stays on the old version | `code_moved` | yes: `/api/changes/<id>` `code_moved` (`now`, `changed_paths`); `step`/`at` of the moving edit: no |
| `storage.bytes` | int | "1.4 GB" | `GET /api/storage` | yes |

## Controls that write, and the route each maps to (wave 2: wired)

Both writes live in `actions.js`, as named constants beside the one function that uses each. `approve-sheet-live.test.js` checks that no other file here posts, and that exactly these two addresses are reached.

| Control | Address | Live behaviour |
|---|---|---|
| **Approve <version>** | `POST /api/changes/<id>/decisions` (write address 1), `{actor, seen_version}` | On only when all of these hold: a tracked Change, the engine's `readiness.ready`, a Pro licence, a name in the box, and no move since the sheet opened, and no accepted decision that blocks approval broken or with no proof found on exactly this version (C29, `decisions.jsx`, from `GET /api/hq/projects/<p>/decisions`). When off, its reason is in plain words: "Nothing counts as done yet, so nothing is ready." / "This version is not ready for approval." plus the engine's own readiness sentence / "Approving a version needs Pro. Everything else is free." / "An approval has to have a name on it." / "A decision you set blocks approving this version." with the first decision's own words and sentence, the box above listing each one with See where (→ the Brief). **Every answer is drawn (C30 P0-2, `actions.js` `answerOf`):** 200 `recorded` → drawn as approved from the server's own row, then the record is re-read; 200 `moved` → the "Changed while this was open" refusal with the files the server named (or "ThruLine could not name the files…") and See what changed, nothing re-read; 403 `licence_required` → "Approving a version needs Pro. Everything else is free." with See plans ↗, nothing re-read; any other refusal → "Not approved." and the reason at the control, nothing re-read. |
| **Withdraw approval** | `POST /api/changes/<id>/decisions/withdraw` (write address 2), `{actor, seen_version}` | needs a name; free on every plan. Its answer is drawn the same way: recorded → from the server's rows; anything else → "Not withdrawn." and the reason |
| **Copy the trailer** | none: clipboard only | |
| **Ask for changes** | none: off, with its reason | |
| **See plans ↗** | link to `https://thrulinehq.com/pricing` | |
| **A note for the record** | `change_notes` (one row naming the decision, appended by the approval itself in the same transaction), read back as `decisions.history[].note` | live: on; sent with the approval as `note` (at most 2,000 characters, only when there are words) and drawn in the history as "Note: “…”". The decision file does not carry it (schema 3). Writing it is free; approving is still the one paid act |

## Wave 2: where the live data comes from

`from-change.js` `selectApproveSheet(snapshot, tracked, {licence})` builds this screen's data. Its sources:
- **The ONE Change snapshot** (`data/change.js`) gives the request, statements, runs, edits and versions.
- **The tracked Change** (`live.js` → `readChangeDocuments` + `deriveChangeModel`) gives the criteria, each with its own state; the readiness answer; `decision.seen`; `decision.approvedFull`; decision history (`change.decisions.history`); and `code_moved`.
- **`/api/licence`** gives Free or Pro, through `ui/src/hq/data/plan.js`.

Live only:
- The decision history is the record's on every read. The gallery keeps the acts pressed in it.
- The reviewer's name starts empty, because the record holds none.
- Each "It rests on" row and each "What the agent said" row draws its own state (● Proven, ◭ Needs a re-check, ○ Not proven, ▪ Recorded). Both lists are now loops built from the design's first row.
- Trace-map cards are drawn only for records that exist, at most the design's three edits and three runs, newest last. The lane counts are the record's.
- "Bar set after the work" is drawn only when acceptance came after the agent's last edit.

## C20 (no trial anywhere), in the design too

The canvas's trial lines are replaced in the gallery as well:
- "Trial · 9 days left" → "Pro".
- "Your trial has 9 days left…" → "Approving a version needs Pro. Everything else is free."
- "Trial active / No licence (trial ended)" → "Pro / Free".
- "See plans ↗" → `https://thrulinehq.com/pricing`.

The new expected render is `docs/hq-next/reference/Approve-Sheet.c20.png`. It differs from the canvas render only in the plan line, the status-bar plan pill and the demo panel.

## Design annotations (gallery only, `v.isDemo`)

The whole "Demo state" panel renders only when `props.data` is absent: Start over, Licence (Pro / Free), The record, Name in the box, the switch hint, and the "Illustrative: … Needs engine: …" note.
