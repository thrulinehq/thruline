# Command-Ask (⌘K: find, ask, act): data contract

`demo.js` exports `DEMO`. It has the shape of `this.props.data`. `logic.js` `renderVals()` reads content only from `D = this.props.data || DEMO`. UI state (typed text, active row, the open answer, the tab, the pinned chip, what was copied) lives in `this.state`.

## Where it renders

- **App.** `app.jsx` renders `<Command at onClose/>` when ⌘K is pressed. `index.jsx` passes `overlay: true` and `onClose`, so only the scrim, the palette and its answer render, fixed and centred over the current screen.
  - Esc runs a ladder: first it closes the answer, then it clears the field, then it calls `onClose`. While the ladder is clearing something, `stopPropagation` keeps the app's own Esc handler from closing the palette early.
  - A click on the scrim calls `onClose`, and so does following any link in the palette.
- **Design gallery** (`#/design/Command-Ask`, no props). The full artboard renders: the dimmed backdrop, the header, the footer and the DEMO STATE panel.
- **Design-only parts.** The backdrop and the DEMO STATE panel are gated on `v.isDemo`. The backdrop is the canvas's drawing of Change-Brief under the scrim; in the app, the real screen beneath supplies it. Neither part ever renders when `data` is given.
- **Header and footer.** They are fed from `D` (`v.top`), but only the full-artboard mode draws them.

## Status key

- **yes**: the store serves the fact today.
- **partial**: something nearby is served, but the fact must be derived from it.
- **no**: nothing serves the fact.
- **unknown**: not verified.

`/api/hq/…` is the new engine read layer being built in `crates/core/src/hq/` (thread view: prompts, edits, commands, claims, versions). It is untracked on this branch, so any fact it would serve is marked **unknown (hq)**.

## Fields

### The Change and its project

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `project.name` | string | Project display name (header, copied-answer footer) | `GET /api/changes/<id>` → `project_id`, then the project list `/api/hq/projects` | partial |
| `project.path` | string | Worktree path (shown only inside evidence `detail` text today) | Project's recorded worktree | unknown (hq) |
| `change.title` | string | Change title (header) | `GET /api/changes/<id>` → `request.title` (masked) | yes |
| `change.agent` | string | Agent that did the work ("You paste it into …") | Session harness on the Change's associations | partial |
| `version.id` | string (7-char) | Current version the palette acts on | `ChangeView.implementation` / `code_moved` | partial |
| `version.current` | bool | Whether it is the current version | Same as above | partial |
| `version.commit` | string | Commit it equals | Workspace identity → commit | unknown (hq) |
| `version.unsavedChanges` | bool | Dirty tree flag | Workspace identity | unknown (hq) |
| `recording` | bool | Whether capture is live (footer "· recording") | Sensor liveness (`thruline sensor doctor`); no HTTP route | no |
| `readAt` | "HH:MM" | When HQ read the record | Response time of the read | yes (client clock) |
| `askedAt` | "HH:MM" | When the person asked | Client clock at ask time | yes (client) |

### The record

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `record.steps` | int | Recorded steps in this Change | Count of the thread's events | unknown (hq) |
| `record.window.from`, `record.window.to` | "HH:MM" | First and last recorded step | Thread timeline | unknown (hq) |
| `record.hookInstalledAt` | "HH:MM" | Since when the hook records ("Before 16:01 … not recorded") | Sensor install time | no |
| `record.statements` | int | Statements in the Change | `/api/claims` for confirmed sessions | partial |
| `record.statementsNeedingCheck` | int | Statements that need a check | Coverage or readiness | partial (`/coverage`, `/readiness`) |
| `storage.total.value`, `storage.total.unit` | number, "GB"/"MB" | Footer total | `GET /api/storage` → `bytes` (format client-side) | yes |

### Actions and counts

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `check.command` | string | The command Check it again would run | Run lane's proposed check (`/api/changes/<id>/run`) | partial |
| `criteria.proposed`, `criteria.accepted` | int | Lines of what counts as done | `/api/changes/<id>/proposals`, `ChangeView.accepted_scope` | yes |
| `approval.waitsOn` | int | How many things approval waits on | `/readiness` | partial |
| `diff.edits`, `diff.files`, `diff.filesChangedAfterLastCheck` | int | What changed, in counts | Edits since the last run | unknown (hq) |

### Where the proof stops

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `proofStop.finishedRuns[]` | `{step:int, version}` | Runs the answer looked at ("Looked at: …") | Run records (exec.db) joined to steps | unknown (hq) |
| `proofStop.pendingSteps[]` | int[] | Commands with no result yet | Command records without an outcome | unknown (hq) |
| `proofStop.changedSince.file` | string | File changed after the last check ("See what changed in …") | Edits after the last run | unknown (hq) |

### What Find can search

**`findable[]`**: the record as Find searches it. Each item has `id`, `kind` and `step`, plus typed facts for its kind:

| Kind | Fields |
|---|---|
| `Request` | `text`, `by`, `at` |
| `Read` | `file`, `agent` |
| `Edit` | `op`, `file`, `versionAfter` \| `afterLastCheck`, `at` |
| `Run` | `command`, `passed`, `failed`, `exit`, `version`, `versionsBehind` |
| `Said` | `text`, `agent`, `at`, `statement:{state, version}` \| null |
| `Version` | `version`, `current`, `commit` |
| `Command` | `command`, `result` (null = no result yet), `removes` |
| `File` | `path`, `changedAt[]`, `runAt[]`, `uncovered`, `lastVersion` |

`renderVals` builds every meta line ("1 failed, 3 passed · 4b19a0c · exit 1") from these fields.

- **Source:** prompts, edits, commands and claims from the thread view; versions from workspace identity. **Status: unknown (hq)**. `/api/hq` thread view `PromptItem` and related types exist in `crates/core/src/hq/view.rs`.
- **Search:** the canvas notes **"NEEDS ENGINE: the search behind 'Looked for'"**. Today Find filters only the items it was given, client-side.

**`nearProofStop[]`** (ids): what Find lists before anything is typed. It must come from the proof-stop derivation. **Status: no**.

### Ask

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `questions[]` | `{key, text, reads}` | The questions this record can answer, and what each reads | An Ask engine | **no** |
| `evidence{id}` | Per kind: `{kind, step, text, …, detail}` | Receipts an answer cites. `detail` is the pinned explanation | The same events as `findable`, with file and line receipts | unknown (hq) |
| `answers{key}` | `{state, word, about, lines[{pre, quote, code, post, steps[]}], evidence[], looked, action, seeChanged?, reason?, draft?}` | The answer to each question | An Ask engine | **no** |
| `story` | `{text, agent, step, at, state, version, why, lastVersionBefore}` | The statement the Story tab shows | Claims and coverage | partial |

- **`answers{key}` detail.** The answer is built from record fields only:
  - `state` is one of `not_proven`, `changed`, `no_result` or `prepared`.
  - `about` gives the pill meta: `{version}`, `{run}`, `{step}`, `{statements:{notProven, of}}` or `{prepared}`.
  - `draft` is the Tell-the-agent message.
- **Nothing computes `answers` today.** Every answer, its sentences and the drafted message are demo text. Wave 2 must either derive them from typed facts or show "The record doesn't say", the silent answer, which `renderVals` already builds from `record.steps`, `record.window` and `record.hookInstalledAt`.

### Design only

- `presets`: the DEMO STATE switcher's three typed queries. These are UI state, never data, and are read only from `DEMO`.

## What the store does not have today

These must be built before Command-Ask can be live:

1. **An Ask engine.** Nothing serves `questions`, `answers` or `nearProofStop`. The canvas itself flags "the search behind 'Looked for'" as needing engine work.
2. **The thread record.** Wave 2 needs a thread view of every step, with typed run, edit, command and version facts, through `/api/hq/…`. This is being built and is not verified here: `findable`, `evidence`, `record.steps`, `record.window`, `proofStop` and `diff`.
3. **Recording liveness and the hook install time.** `recording` and `record.hookInstalledAt` have no HTTP route.
4. **The version's commit and dirty flag** (`version.commit`, `version.unsavedChanges`). Not verified.

The design copy keeps a few things fixed regardless of data:

- "Recording · following live" in the header.
- The action labels and key hints.
- "Answers come from your recorded sessions. Nothing leaves this machine."

## Wave 2: what ⌘K reads

`index.jsx` reads `/api/hq/projects` (current project via `data/project.js` `currentProjectId`), `/api/hq/projects/<id>` (its Changes), and, when the address names one of its threads, `/api/hq/projects/<id>/threads/<tid>` through `data/change.js` `changeSnapshot`. `from-change.js` turns the snapshot into this contract; `live.js` says each row. Steps are the snapshot's steps, so ⌘K and the Change screens name the same step for the same row.

- **Find:** the open Change's request, edits, runs, commands, statements and files, then the project's other Changes by title and most-edited file. With no Change open, the project's Changes. Prompts of other Changes are not searched (that would take one read per thread).
- **Ask:** ask-hq's five questions. A tracked Change is answered by `ask-hq.js` over `deriveChangeModel` (pass it as `hq`); the reader for tracked documents (`data/use-change.js`, W2-B) has not landed, so today every Change is answered from its thread: changed since the last passing test run, open claims ("Claimed, and no proof found in the record"), test runs with no result, and approval resting on what counts as done. What the thread cannot ground is the three-part "The record doesn't say" answer in ask-hq's words.
- **No write, no model, no network** beyond this machine's server.

## C30: what ⌘K reads now (supersedes the Wave 2 section above)

`index.jsx` reads the open Change through the one Change loader (`data/use-change.js` `useChange`): a thread id or a tracked `chg_` id, its tracked documents and run sheet folded into one snapshot. With a Change open, the project is the Change's own. It also reads `/api/hq/projects/<id>` (the project's Changes), `/api/hq/projects/<id>/threads/<tid>` (the open Change's own thread view, for the one fact the snapshot does not carry: which edits delete a file), `/api/hq/projects/<id>/search` (`data/search.js`, what is typed, across the record) and `/api/hq/projects/<id>/decisions`. Every one is a GET.

- **Find:** the open Change's request, edits, runs, commands, statements and files from the snapshot, each with its step; then, from the record search, what the agent **read**, each **version**, and every kind of the project's other Changes, each linking to the Change screen that shows it (`live.js` `searchHitRow`); then the project's Changes by title and most-edited file, and its decisions. With no Change open, the search runs across every Change. "n more" adds the search's own `total`. A search that failed is said in the Find group.
- **Ask:** the design's five questions (`from-change.js`), answered from the snapshot's typed fields, tracked or not: did it finish (the statement the Brief is about and the run it rests on), what changed after the last check (edits after the last passing test run), was anything destructive (commands on the one list `thruline explain` keeps, `live.js` `DESTRUCTIVE`, and edits the thread view types as deletions), what is not proven (each statement that is not, and why), what should I tell the agent (the check sheet's own message, `tell.js`). What the snapshot cannot ground is "The record doesn't say": what was looked for, what is missing, what would answer it.
- **↵** on an Act, Go to or Find row follows that row's own link; Approve stays inert (`logic.js` `follow`).
- **No write, no model, no network** beyond this machine's server. "Copy for the agent" writes to this machine's clipboard and sends nothing.
