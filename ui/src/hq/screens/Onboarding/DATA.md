# Onboarding (First run): the data contract

`renderVals()` reads every piece of content from `const D = this.props.data || DEMO`
(`demo.js`). This file lists every field of that object: its type and meaning, the
real fact it must come from, and whether the store serves it today.

"Store today" was read from the code on 2026-10-07; no route was called.
- **yes**: a route serves it.
- **partial**: a route serves part of it, or serves it in another shape.
- **no**: nothing serves it.

Routes are in `crates/cli/src/uiserve.rs`. `/api/hq/*` is the read layer `ui/src/hq/data/api.js`
names (`/api/hq/load` among them). The server has served it since `5a01a45b` (see the Wave 2 section).

## Wave 2: where the live data comes from

`ui/src/hq/data/project.js` builds this object. `firstRunData()` takes:
- `GET /api/hq/projects`;
- `GET /api/hq/load`;
- `GET /api/storage`.

`index.jsx` reads them through `useFirstRun()` in `data/use-project.js`, and re-reads `/api/hq/load` every second while a load runs.

- **`folders[]`.** One per HQ project, in a loop built from the canvas's own folder row (the Git and no-Git variants):
  - `name`;
  - `path`, the root with home shown as `~`;
  - `agent`, from `sources`, e.g. "Claude Code and Codex";
  - `git`: `null` from `firstRunData` (C30 P0-7: nothing in the project list says it). Since C30 R-ROUTES, `index.jsx` folds in the capture route's git answer for each folder it has read: `true`, `false`, or `null` with `gitReason` when git could not answer. A folder never read stays `null` ("Git not checked yet");
  - `recorded`, true when `sessions > 0`;
  - `found`: `{sessions, prompts, edits, commands, tests, claims, threads}`, each a typed field of `ProjectSummary`;
  - `capture`: `null` from `firstRunData`; for the folder in front of the person, `index.jsx` folds in the capture route's answer (see "C30 R-ROUTES" below).
- **`storage.sessions`.** The sum of `sessions` across all projects.
- **`home`.** `storage.path` (from `/api/storage`).
- **`load`.** `GET /api/hq/load`: `running`, `finished`, `filesTotal`, `filesDone`, `eventsAdded` and `errors` count.

**Design addition: load progress on First run (intended difference).** The canvas has no space for import progress, so the product adds, live only:
- **A quiet status block** under the folder list, in the design's caption type (12.5 px `#A1A1AA`, and 12 px Geist Mono `#82828C` for the counts). It says one of:
  - "Recorded so far for thruline:" over "539 sessions · 505 prompts · 6,361 edits · 65,608 commands · 2,645 test runs · 7,142 claims · 144 threads";
  - while a load runs: "Reading 540 recordings — 312 done · 401,220 rows";
  - when it ends: "Read 540 recordings · N new rows".
- **The one primary action, "Load every session".** It reaches `POST /api/hq/projects/<id>/load` (`HQ_LOAD_ADDRESS`, a new write address). While a load runs, the button reads "Reading every session…" and is disabled.
- **When the load finishes,** the Inbox opens on that project.

The gallery is unchanged: no block is drawn, and the button stays "Continue with checkout-web".

The product opens on step 1. The gallery keeps the canvas's step 2.

**Whether hooks are installed** was not served over HTTP until C30 R-ROUTES; it is now, and step 3 reads it (next section). Before a folder is read, step 3 still says "ThruLine can’t tell from here whether thruline is set up to record", and its hooks row "Not checked yet".

**Install has no route in this version.** The button is disabled, with the title "Not available in this version. …", and the line under it says to install from a terminal with `thruline init`.

On the live path, the step-1 example panel ("What you'll see after the first session") is not drawn. The record holds no example chain for First run to show, and the canvas's own example is demo data.

## C30 R-ROUTES: step 3 reads the capture route (2026-10-08)

`index.jsx` reads `GET /api/hq/projects/<id>/capture` (`data/capture.js`) for the folder in front of the person, and no other:
- when that folder is chosen (and at once for the default folder), again when step 3 opens, and on every Look again;
- every 2 s while step 3 is open and the answer says waiting for a first step, and no longer once one is seen (`CAPTURE_POLL_FLOOR_MS`; never faster).

`capture-view.js` `withCapture` folds the answer into that folder as `capture` (`data/capture.js` `captureOf`: `{state: 'reading' | 'failed' | 'read', installed, health, …facts}`) and sets `git`/`gitReason` from it. `stepThree` turns it into every word step 3 draws. A `capture` without a `state` is the older contract (`{installed, health}`), still drawn as before.

`health` (the phase) is decided by the route's facts alone:

| phase | when | step 3 draws |
|---|---|---|
| unknown | a settings file could not be read and none read holds a recorder, or the hooks are in and the record could not be read | "Can’t tell", with the route's reason; the sensor-doctor card when the settings could not be read |
| none | no ThruLine recorder in any settings file Claude Code reads for the folder | the canvas's not-installed state |
| problem | a recorder is installed and a ThruLine hook names a program that is not on this machine | the canvas's problem state; "Not recording" only when every recorder's program is gone, else "Can’t run" with the count |
| waiting | installed, not known broken, no step through the hooks yet | the canvas's waiting state; when the record already holds steps read from the recordings, it says so |
| recording | installed, not known broken, and the hooks have delivered | the canvas's recording state, with the first and latest delivery times and the hook version when served |

A read still out reads "Checking"; a failed read says the server's sentence. Neither becomes installed or not installed.

What the route does not read (the MCP server, speaker hooks, run capture) is "Not checked yet" once the hooks are installed, never "Installed"; before that those rows are the person's step-2 choices. The run-capture steps ("Turn on run capture · in your terminal") are drawn in the gallery only, because they promise the shim's own output, which nothing serves.

**First steps recorded** (`first-ask.js`): while recording on step 3, `GET /api/hq/projects/<id>` and the thread view of the thread that started first give the first ask, its step (the one Change model's step, `changeSnapshot`) and the thread's title. "Review what counts as done →" goes to that thread's What counts as done. With no thread in the record, the generic card is drawn and the link has no address.

## The founder's requirement, and what the canvas does not draw

**What the founder asked for, 2026-10-07:** First run must load every session for
the chosen project and show the work it found. That means every Claude Code and
Codex session on this machine for that folder, and in each one:
- every prompt;
- every edit;
- every command and test run, with how it ended;
- every claim.

The canvas's First run predates that requirement.
- It draws the first ask and the Change proposed from it (step 3, "First steps recorded").
- It draws a folder count ("Folders where an agent has worked · 2").
- It has **no slot** for:
  - an import's progress;
  - what the import found per folder (sessions, prompts, edits, runs, claims, Changes).

Those facts are listed under "Required, not yet drawn" below. They are **not** in
`DEMO`, because DEMO holds only what the canvas shows, and inventing demo numbers
for them would be a fabrication. Wave 2 needs a design slot for them; the founder
or the designer must supply it.

## What the template can draw today

`template.jsx` is the canvas markup:
- **2 folder slots** (`folders[0]`, `folders[1]`). The heading count is `folders.length`.
- **3 speaker-example lines**.
- **1 example proof chain**, in step 1's side panel.

Turning the folder list into a `.map` is a markup change for Wave 2.

The **state of the chosen folder's capture** (`capture.installed`, `capture.health`) selects:
- the step-3 headline;
- the state card;
- the capture-health rows;
- the footer action;
- the storage line.

In the design gallery the DEMO STATE switcher sets those two values instead
(Not installed · Waiting · Recording · Hooks can't run). In the product the
switcher is not drawn, and the record decides.

UI state stays in `this.state` and is not content:
- the current step;
- the chosen folder;
- the MCP, speaker and run-capture choices;
- which command was copied.

Product constants stay literal in the markup, because they are properties of the
install pack, not of a project:
- "10 events", "3 tools";
- "Up to 3 lines", "3.5 s";
- "127.0.0.1 only".

## Fields

### Top level

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `home` | string (path) | "Where it keeps it": the store folder | the store folder | **yes**: `GET /api/first-use` → `store.folder` |
| `storage.sessions` | int | the sidebar footer: "ThruLine is keeping the record of N sessions on this machine." 0 reads "ThruLine is keeping nothing on this machine yet." | the sessions kept | partial: `/api/first-use` → `store.bytes` and `candidates[].recordings`; `/api/storage` → `bytes`. Nothing serves a session count directly; it is the sum of `candidates[].recordings` for registered projects. |
| `states` | `{none, waiting, recording, problem}` → `{storage}` | **gallery only**: what changes with each DEMO STATE variant | — | — |

### `folders[]`: every folder on this machine where an agent has worked

| Field | Type | Meaning | Real source | Store today |
|---|---|---|---|---|
| `id` | string | folder key | candidate key | **yes**: `/api/first-use` → `candidates[].key` |
| `name` | string | "checkout-web" | folder label | yes: `candidates[].label` |
| `path` | string | "~/code/checkout-web" (home shortened to `~`) | folder path | yes: `candidates[].key` / `source` (path; the client shortens it to `~`) |
| `agent` | string | "Claude Code": the agent seen working there | the harness of its sessions | partial: `candidates[].source`; `/api/sessions` → `harness`. `codex_recordings` is reported separately. |
| `git` | bool | "Git repository" / "No Git repository" | `.git` in the folder | **yes**, per folder read: the capture route's `git` (C30 R-ROUTES) |
| `recorded` | bool | "Not recorded yet" vs recorded. The canvas never draws the recorded word; "Recorded" is a stand-in until the design names it. | the folder is a registered project | yes: `candidates[].registered` |
| `capture.installed` | bool | ThruLine's hooks are installed for this folder | the hook pack in the folder's settings | **yes**: the capture route's `claude_code_hooks.installed` (C30 R-ROUTES) |
| `capture.health` | `'none' \| 'waiting' \| 'recording' \| 'problem'` (+ `'unknown'`) | the step-3 state. `problem`: the hooks point at a ThruLine program that is gone. | the doctor, plus the first event received | **yes**: derived from the capture route's facts (`data/capture.js` `captureFacts`); liveness is a re-read every 2 s while waiting |
| `firstSession.startedAt` | `HH:MM` | "Session started 16:01", "Received · 16:01", "Recording · since 16:01" | first event of the first recorded session | yes: `/api/sessions` → `first_event_at`; `candidates[].first_recorded` |
| `firstSession.agent` / `.hookVersion` | string | "Claude Code (hook v0.4)" | harness / hook pack version | agent: yes; hook version: **no** |
| `firstSession.firstAsk` | `{step, text, proposedChange}` or `null` | "You asked · step 6 “…” → ThruLine proposes a Change from it: Fix double-applied discount on checkout total". `null` shows the generic "Your first request shows here once it is recorded." | the first person-typed prompt, and the Change proposed from it | partial: `/api/first-use/proposal` → `proposed{title, request, request_event}` is built from the **most recent** work, not the first prompt. There is no step number. `crates/core/src/hq/prompts.rs` classifies prompts, but it is not compiled in yet. |

### `speakerExample[]`: the lines a speaker hook would add (step 2)

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `text` | string | one receipt-carrying line, in the speaker vocabulary ("claimed, and no proof found in the record") | partial: the digest cache `~/.thruline/digest/<project-key>.md` and its receipts sidecar, written by the speakers. No HTTP route. Before any record exists for the folder there is nothing to show; the canvas labels these lines "example · wording illustrative". |
| `step` | int | the step the line ends at | as above (the receipts sidecar) |

### `example`: one statement's proof chain (step 1's side panel)

This is "What you'll see after the first session". In the product it should be a
real chain from the folder's imported sessions when one exists. Until then, a fixed
example labelled as one. **Decision owed:** the canvas's own label, "Illustrative:
from the demo record, not your project.", is a design annotation and is not drawn
in the product.

| Field | Type | Meaning | Store today |
|---|---|---|---|
| `said` | `{agent, quote, step, at}` | ① "Claude Code said ❝Tests pass.❞ 16:03 · step 52" | partial: `/api/claims` (no step index) |
| `restsOn` | `{step, command, passed, failed, exit}` | ② "pytest -q tests/test_totals.py · 4 passed, 0 failed · exit 0" | partial: claim evidence / run records |
| `ranOn` | `{step, version, versionsOlder}` | ③ "version 9f2ad3e · 1 version older" | partial |
| `since` | `{step, file, at}` | ◭ "Changed after · checkout/api.py 16:04" | partial |
| `stops` | `{version, file}` | "Proof stops here: Nothing on e71c5b0 has run with checkout/api.py in scope." | **no** chain model |
| `so` | `{state, version}` | ⑥ "○ Not proven · version e71c5b0" | partial (claim verdict) |

The first-time-test answers in that panel are formatted from these fields:
- "What first?" from `stops.file`;
- "Why believe it?" from the four step numbers.

## Required by the founder, not yet drawn (needs a design slot)

These are the per-folder facts that "load every session and show the work it found" needs:

| Fact | Type | Store today |
|---|---|---|
| `folders[].sessions` | int: sessions found for this folder, by agent (Claude Code, Codex) | partial: `candidates[].recordings`, `not_read`, `codex_recordings` |
| `folders[].import` | `{state: 'idle' \| 'reading' \| 'done' \| 'failed', read, total, notRead}`: the load's progress, shown while it runs | partial: `POST /api/first-use/register` → `{read, not_read, recorded_work, steps}` (one blocking call; there is no progress); `/api/hq/load` is planned and not served |
| `folders[].found.prompts` | int: person-typed prompts | **no** (the `hq/prompts.rs` classifier is not compiled in) |
| `folders[].found.edits` | int: file edits, with files and lines | **no** (`hq/edits.rs`, not compiled in) |
| `folders[].found.commands` / `.tests` | int each, plus how they ended (succeeded, failed, unknown) | **no** (`hq/commands.rs`, not compiled in) |
| `folders[].found.claims` | int: claims the agent made | partial: `/api/claims` per session |
| `folders[].found.changes` | int: Changes (threads of work) connected from the prompts | partial: `candidates[].changes` (registered projects only) |

## Design annotations (gallery only)

These render only when `this.props.data` is absent (`v.isDemo`):
- the DEMO STATE panel;
- "Removal: NEEDS ENGINE (L1.1)";
- "A live “waiting…” needs a liveness field: NEEDS ENGINE (P6-IR-2)…";
- "The Install button needs an install route: NEEDS ENGINE…";
- "Illustrative: from the demo record, not your project."

`onboarding-data.test.js` checks two things:
- every such literal in `template.jsx` sits under a `v?.isDemo` gate;
- the live path renders none of them.

Kept as product text, flagged for the designer: "example · wording illustrative",
above the speaker lines. If the lines come from the person's own record, the word
"illustrative" stops being true.
