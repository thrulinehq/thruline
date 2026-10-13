# What-Changed — data contract

The screen reads one object: `this.props.data || DEMO` (`demo.js`). Without
`data` it is the design gallery, and three things appear only there:
- the canvas annotations:
  - the "Demo records." banner with "Replay from …";
  - the "engine: diff route not built yet" note;
  - "Line counts are illustrative";
  - "Declared paths are engine work, not built yet";
  - "Step 62 is illustrative";
- the canvas's played-out "Check it again", which finishes after 1.8 s with
  the illustrative run in `afterCheck`.

With `data`, "Check it again" only enters the checking state. The screen never
fabricates a result. It turns covered only when a finished run on the current
version is in `runs`.

The screen shows every file the Change touched and every line every edit
added or removed. For each file it shows the edit steps, the version each
edit made, which run covers it, and how that coverage stands on the current
version. The comparison can run from any recorded version to now.

Field names match `Change-Brief/demo.js` wherever a field means the same thing
there: `project`, `machine`, `view`, `change.stages`, `version`, `versions`,
`runs`, `edits`, `statements[].proof`, `recheck`.

Conventions:
- **step** is the record's step number inside its own session.
- **time** is `HH:MM` local.
- **Store today** says whether the local store serves this fact now:
  - **yes**, with the route or table;
  - **partial**, with what is missing;
  - **no**;
  - **unknown**.
  - `crates/core/src/hq/*` is the engine work in flight for `/api/hq/`. It is not routed yet.

## Fields

### Frame

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `project.id`, `project.name`, `project.worktree` | string | project key, name and directory | project record | yes / yes (`/api/hq/projects`, not in `uiserve.rs` yet) / partial |
| `machine.recordsBytes` | int, bytes | footer: "ThruLine is keeping 1.4 GB …" | `GET /api/storage` | yes |
| `view.readAt` | `HH:MM` | "read 16:04 ↻" and "At 16:04 this read:" | the adapter's clock at fetch | n/a — a client fact |
| `change.id`, `change.title` | string | Change id and title | `/api/changes/{id}` (`request.title`) | yes |
| `change.recording` | bool | "Recording", and "· recording" in the status bar | sensor liveness | unknown |
| `change.followingLive` | bool | "· following live": the screen re-reads as work lands | the adapter (`useResource` `every`) | n/a |
| `change.stages[]` | `{id, status}` × 6 | Define · Prepare · Build · Verify · Review · Approve. `status` is `done` ✓, `current` ●, `not_started` ○ or `not_reached` —. | the Change's stage derivation | partial — the legacy six-stage strip in `ui/src/workspace/change-model.js`; not served |
| `version.id` | string | the version the screen speaks for, the newest recorded one | the newest entry of `versions` | partial — `ImplementationView.snapshot_id` / `base_revision` |
| `version.commit` | string | "= commit 3f9a1c2" | the version record | unknown |
| `version.unsavedChanges` | bool | "no unsaved changes" or "unsaved changes" | worktree dirtiness at record time | unknown |

"(current)" is drawn when `version.id` is the last entry of `versions`.

### `versions[]` — the compare ruler

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `versions[]` | array, oldest first | every recorded version; the last is now. One ruler band per version before now, plus the "now" band. | no |
| `.id` | string | version id | no |
| `.madeByStep` | int \| null | the edit that made it. When the first version has one, the code before that edit was never recorded, and the ruler draws "before step 16 · ◌ not recorded". | no |
| `.recordedAtStep` | int \| null | the step where ThruLine recorded it | no |

Each band's marks and its spoken text are derived from what happened while
that version was the code:
- the edit that made it: ± edit, and ◭ when the edit came after the newest run that held;
- runs on it: ✕ failed, ● held, ⋯ no result;
- statements said on it: ❝, from `statements[].versionBefore`.

### `files[]` and `edits[]` — every line of code

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `files[]` | array | every file the Change edited or ran, grouped by kind | union of edited paths and run scopes | partial — edited paths via `hq/edits.rs`; kind no |
| `files[].path` | string | path relative to the worktree, masked | | yes (`FileEdit.path`) |
| `files[].kind` | `code` \| `tests` \| `config` \| `generated` | the group it is listed under. Empty kinds are summarised ("Config 0 · Generated 0"). | path classifier | no |
| `selectedFile` | path | the file open when the screen first draws | n/a — UI | n/a |
| `edits[]` | array, step order | every edit in the Change | edit records | partial — `crates/core/src/hq/edits.rs` (`FileEdit`, `Hunk`); not routed |
| `edits[].id` | string | edit id | event id | partial |
| `edits[].step` | int | the edit's step: the "Edits" column ("16, 36" or "56 · 16:04"), the hunk chip, ruler marks | | partial |
| `edits[].sessionId` | string | session | | partial |
| `edits[].time` | `HH:MM` \| null | when; null when not recorded | event timestamp | partial |
| `edits[].file` | string | the file this edit changed (one `FileEdit` per file) | `FileEdit.path` | partial |
| `edits[].op` | `edit` \| `write` \| `multiedit` \| `notebook` \| `patch` | the tool's operation | `FileEdit.op` | partial |
| `edits[].makesVersion` | string | the version the code became: "→ 9f2ad3e", and which ruler band the edit sits in | versions × step | no |
| `edits[].hunks[]` | array | each contiguous change | `FileEdit.hunks` | partial |
| `.hunks[].oldStart` | int | the first line, before this edit, of the hunk's first old-side line | `Hunk.old_start` | partial — absent when computed from the request rather than the tool's patch |
| `.hunks[].newStart` | int | the same line after this edit | `Hunk.new_start` | partial — same gap |
| `.hunks[].context` | string | the enclosing function shown after the `@@` header ("def checkout_total(request):") | file content at the version | no |
| `.hunks[].lines[]` | `{op: 'context' \| 'add' \| 'del', text}` | every line in order, as recorded, masked | `Hunk.removed` / `Hunk.added` | partial — the engine's `Hunk` keeps **removed and added lines only; no context lines** and no order between them |

Derived from the hunks and never stored:
- the `@@ -o,n +o,n @@` header;
- every line number in both gutters;
- per-file and per-range "+3 −1";
- "N hunks";
- "N files · M changed since …".

When the range starts before an earlier edit to the same file, the old side is
renumbered against the range's start. That is how "whole Change" draws step
36's hunk at old line 24, where it was at 25 against 4b19a0c.

### `runs[]`, `statements[]`, `recheck`

| Field | Type | Meaning | Must come from | Store today |
|---|---|---|---|---|
| `runs[]` | array, step order | every command run in the Change. Feeds "Covered by", ruler marks, the run chip, the blame beside each added line, "Looked at: …" | command records | partial — `crates/core/src/hq/commands.rs`; `EvidenceView` (`change_evidence.rs`) |
| `.id`, `.step`, `.sessionId`, `.time` | | identity and when it ran | | partial |
| `.version` | string | the version it ran on | | no |
| `.command` | string | the command text, masked | | yes (hq/commands) |
| `.scope` | string[] | the paths it named. A file listed here is "the check" ("Ran · step 46"). A folder (`checkout/`) puts the files under it "in scope". | command arguments | no |
| `.status` | `finished` \| `no_result` \| `running` | whether a result is recorded | | yes — hq/commands `succeeded`/`failed`/`unknown`, mapped |
| `.exitCode` | int \| null | only when the result states one | | yes |
| `.passed`, `.failed` | int \| null | the tally ("4 passed, 0 failed") | the runner's summary line | **no** |
| `.durationMs` | int \| null | duration | | no |
| `.capturedBy` | `thruline` \| … | capture source | | partial |
| `.declaresPaths` | bool | whether the check declares what it reads. False means it counts for every file the Change covers (the "this check doesn't say which files it depends on" sentence). | check profile | no |
| `statements[]` | array | the agents' checkable statements. Their `step` places ❝ on the ruler. | `/api/claims` | partial |
| `statementId` | string | the statement this screen answers for ("for ❝Tests pass.❞ Claude Code · step 52") | the route's parameter | n/a |
| `statements[].text`, `.agent`, `.step`, `.time`, `.sessionId`, `.versionBefore` | | as on Every-Statement | | partial / no for `versionBefore` |
| `statements[].proof.verdict` | `not_proven` \| `needs_recheck` \| `proven` | the header pill | | partial |
| `statements[].proof.restsOn` | `{kind:'run', runId, basis}` | "the proof": the run it rests on. Its version is the "Only since the proof" range start. | | partial |
| `recheck.runner`, `.scope`, `.on`, `.permission` | | "pytest -q tests/test_totals.py checkout/ · on e71c5b0 · asks once for this project" | `/api/changes/{id}/run`, `/run/permission` | partial / yes |
| `afterCheck` | object | **demo only.** The canvas's illustrative step 62, its verdict and stages after the check. The live path never reads it. | — | — |

## Coverage

Coverage is derived, never stored. It uses the newest run in `runs` that held,
meaning it is `finished` and exited 0 (or, with no exit code, failed 0). For
each file:

| Coverage | When | Pill | Mark beside each added line |
|---|---|---|---|
| now | the run is on `version.id` and reads the file | "Proven · e71c5b0", neutral solid mark | solid |
| older | the run is on an older version, and the file's last edit made that version or an earlier one | "Needs a re-check · proof is on 9f2ad3e" | hatched |
| none | the file changed after it | "Not proven · no check covers it" | dashed |

Green is used only on the header pill's "Proven"; it never appears beside a line.

## What the store does not have today, in short

- context lines and line order inside a hunk;
- the enclosing function;
- line numbers for edits computed from a request;
- the version each edit made and each run ran on, and the version list;
- file kinds;
- run scope;
- the test tally;
- run duration;
- declared check paths;
- `versionBefore`;
- version commit and dirtiness;
- the served stage strip;
- whether a sensor is recording.

## Limits of the current markup (wave 2 must change the template, not the data)

- **The stage strip:** it draws exactly six named stages. Glyph and colour come from data; the names are fixed.
- **The "Proof stops here" panel:** it shows only for a file changed after the proof, and only before the check. Its text comes from data.
- **The header's "Ready for approval" and the "Approve" button:** these appear whenever a run that held on the current version is newer than the proof. That is the canvas's rule, not a readiness answer from `/api/changes/{id}/readiness`; wave 2 should feed readiness here.

## Wave 2 — on the real record (W2-C)

**Where the data comes from.**
- `index.jsx` reads the ONE Change snapshot through the shared hook `data/use-change.js`, which calls `data/change.js`.
- `from-change.js` (`whatChangedFromChange`) selects this screen's contract from it.

**INTERIM: hunks are not in the snapshot yet.**
- The snapshot's edits do not yet carry their lines. The engine serves them (API.md §Edit), but `change.js` drops them.
- Until it carries `edits[].hunks` and `edits[].source`, `index.jsx` re-reads the same thread, named by `snapshot.project.id` and `snapshot.change.threadId`.
- `withServedHunks` then joins each served edit's hunks and source onto the snapshot's edits by id.
- The orchestrator has been asked to route this to W2-B. When the snapshot carries them, the extra read stops by itself (`hasLines`).

**Every edit, every line.**
- All hunks of all edits in the chosen range are drawn, every line. Nothing is cut.
- The diff box scrolls, and the coverage grid is capped at 170 px with its own scroll, so the diff box keeps its height.
- **A hunk whose start the record does not state** (`source: apply_patch` or `request`):
  - its header reads "@@ line numbers not recorded @@";
  - both gutters are empty;
  - a request-read hunk adds "from the request · no surrounding lines".
- **Counts:**
  - "+a −r" is counted from the hunk lines;
  - an edit without lines uses its served `linesAdded`/`linesRemoved`;
  - "lines not recorded" is shown when neither exists.
- The engine's `Hunk` now has context lines, `header`, `old_start` and `new_start`, so the wave-1 gap ("no context lines") is closed for `structured_patch` edits.

**What the selector passes or decides.**
- **`runs`:** test runs only. A command that is not a test never covers a file.
- **`runs[].outcome`:** passed through.
- **`files`:** every edited file in first-edit order, then every single file a run named.
- **`files[].kind`:** decided by a stated path rule, `kindOf`:
  - `generated` for dist/build/target/node_modules, `.min.js`/`.map` and lock files;
  - `tests` for tests/ spec/ `__tests__/` directories, `*.test.*`, `test_*` and `*_test.go`;
  - `config` for json/toml/yaml/ini/cfg/conf/env, rc files, Makefile, Dockerfile and `.github/`;
  - `code` otherwise.
- **`selectedFile`:** the last edited file.

**Rules this screen now applies by step, because the real record has several sessions.**
- **Coverage:**
  - a file is covered by a run only when its last edit came before that run;
  - `now` when the run is on the current version;
  - `older` otherwise.
  - Before this, version order decided it, and that misread edits that share a version with the run.
- **"Only since the proof":** edits after the proof's step.
- **"Since <version>":** edits after the edit that made that version.
- **"Whole Change":** every edit, even when the proof ran on the first version.
- **"The proof":**
  - the run the focus statement rests on;
  - failing that, the newest test run that held;
  - failing that, none. The page then says "nothing in the record has checked it" and "no run in the record backs it".

**Presentation on the live path.**
- **Absolute paths** (work done in another checkout) are shown from the folder every file of the Change shares.
- **The ruler:** a version band with more than 12 marks shows 11 and "+N more"; its spoken text still names every one.
- **"Ready for approval" and "Approve V":** appear only when the shared snapshot says `change.state === 'ready'`, which comes from tracked readiness. This screen never reads readiness itself; the gallery keeps the canvas's played-out check.
- **Recording:** "Recording not known" when the snapshot does not say.

**Controls (C30 R-RUN).** "Check it again" runs the tracked Change's own check, through the one run client (`data/run.js` → `Repair-Loop/actions.js`).
- When the server would start it, the press opens the check sheet already running (`#/changes/<id>/check?run=1`): the press is the consent. The sheet reads the intent once and takes it out of the address before anything starts, so a reload or Back never starts a second run.
- When the server says it will not start one, the control is off and carries the server's own reason and what would let it run (`runActOf(...).blocked`).
- Work not tracked as a Change: the press opens the check sheet's ready form, which offers "Track this work as a Change" (C26 b).
- While a check of this Change runs (the run sheet's `job`, folded into the snapshot by `data/change.js` `withRunSheet`), the screen draws it running from the record, and "Stop this check" reaches `POST …/runs/<run>/cancel`.
- This screen holds no write address of its own.
