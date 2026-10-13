# Settings: data contract

`demo.js` exports `DEMO`. It has the shape of `this.props.data`.

`logic.js` `renderVals()` reads content only from `D`:

- in the product, `D = this.props.data`;
- in the design gallery (no props), `D = DEMO`, with `DEMO.variants[state.demo]` laid over it when the DEMO STATE switcher selects a variant.

## How the screen derives its state

- **System state.** The screen's system state (`normal`, `dead`, `upgrade` or `stopped`) is derived from `D.sources.claudeCode.status` and `D.hq.stopped`. It is never read from the switcher.
- **Settings values.** The values a person can change (face, pause, the speaker, exclusions, masking, colour, density, motion, check permission, notifications, digest and keys) start from `D`; switches use `settingsOf(D)`, while live permission rows use `D.runPermission` directly. Named templates are read-only previews from the project's committed base policy. On the live page, pause, speaker, exclusions and exact-folder run permissions read their current standing from the server after every act; `this.state` holds only pending writes, refusals and the typed-path draft. The gallery keeps its canvas-local behavior.
- **Design-only parts.** These are gated on `v.isDemo`:
  - the DEMO STATE panel;
  - the "Needs engine …", "Illustrative values …" and "Served today …" notes;
  - the "Built on the run lane …" sentence;
  - the "time estimate · filled in at runtime" placeholder.

  When `data` is given, the product retains "Checking is never paid." The exclusions sentence instead says they leave matching entries out of views and exports while the sealed transcript remains.

## Status key

- **yes**: served over HTTP today.
- **cli**: exists only as a terminal command or a file, with no HTTP route.
- **partial**: something nearby is served, but not this fact.
- **demo**: an artboard fixture only; the live page uses a separate served field.
- **no**: nothing in ThruLine implements it.

Cited routes are in `crates/cli/src/uiserve.rs`.

## Fields

### Time, HQ and the project

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `readAt` | "HH:MM" | When HQ last read; also the time stamped on any act taken here ("Removed at", "paused", "agreed at", "Taken back at", "Tried again at") | Client clock | yes |
| `hq.stopped` | bool | HQ is not answering (page shows a stale read, all actions off) | Client-side: a failed read | yes (client) |
| `project.name` | string | Project the per-project settings act on (pause, speaker, footer) | Project list (`/api/hq/projects`, being built; or `/api/review-projects`) | partial |
| `project.paused` | bool | Recording paused for this project | Nothing (see below) | **no** |
| `project.speaker` | bool | Speaker hooks consented for this project | `thruline init` installs the speaker hooks globally (into `~/.claude/settings.json`), behind a consent prompt, not per project; no route reads it | **cli** (global, not per project) |

### Where the record comes from

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `sources.claudeCode.name` | string | Agent name | Fixed | yes |
| `sources.claudeCode.status` | `observing` \| `hooks_dead` \| `upgrade_pending` | Capture state | `GET /api/lanes` → `claude_code.live` (file times within 5 min) is the only served signal. Dead-hook diagnosis is `thruline sensor doctor` (cli). An upgrade pending makes every other command exit 3 (A193); no route serves it | partial |
| `sources.claudeCode.since`, `sources.claudeCode.lastStep` | "HH:MM" | Hook install time; last recorded step | Ledger's last sensor event | partial (`/api/lanes` has liveness only) |
| `sources.claudeCode.hookEvents` | `{count, first, last, notSubscribed[]}` | Subscribed hook events | `crates/core/src/sensor.rs` `PINNED_EVENTS` | **cli** |
| `sources.codex.name`, `sources.codex.status` | string, `not_answering` \| … | Codex capture | `GET /api/observation` serves the log observer's state (`enabled`, `state`, `events_written`, `last_observed_at`, …) when HQ runs with `--observe-codex DIR`, and `{enabled:false, state:"not_configured"}` otherwise. `/api/lanes` → `codex.live` | partial |
| `sources.runCapture.on` | bool | Run capture (the PATH shim) installed and intact | `thruline shim status --json` | **cli** |
| `sources.runCapture.tools` | `{named[], more}` | Wrapped tools ("pytest, cargo, npm and N more") | `crates/capture/src/shim.rs` `WRAPPED` | **cli** |

Two corrections to the canvas:

- **Hook events.** The canvas says "10 hook events · … installed per project". The code pins 7 events (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SessionEnd`, `PreCompact`) plus a dormant `StopFailure`. `init` installs them globally.
- **Codex "app server".** The canvas describes Codex capture through an "app server". The only Codex path that exists is log observation (`/api/observation`). No app-server client exists; the canvas's own note says so.

The canvas's "8 more" tools is also wrong. `WRAPPED` names 15 candidates, of which 12 are left after pytest, cargo and npm, and only the ones present on the machine get a wrapper.

### Storage (L1.1)

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `storage.root`, `storage.mode` | string | Where records live; folder mode | `GET /api/storage` → `path` (+ `also`, `also_command_log`); the mode is not served | partial |
| `storage.files[]` | string[] | Technical list of stores | Fixed product knowledge | yes |
| `storage.parts[]` | `{key, name, mb, swatch, what, whatT, lose, stays[], goes[], restsOn:{change, ready}\|null}` | Size by part, and what removing it would lose | See below | partial |

`storage.parts[]` in detail:

- **What exists.** `GET /api/storage` → `bytes` and `parts[{key, bytes}]` with keys `records`, `command_log`, `preserved_transcripts`, `digest`, `export` and `other`.
- **What does not exist.**
  - The design's parts (a project, run output older than 30 days, demo, saved versions) are not the served parts.
  - Sizes by project are not served.
  - Nothing serves "what removing it loses" or "what an approval rests on".
  - **There is no remove route**, so Remove… cannot work.

### What ThruLine can see from each agent

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `capabilities[]` | `{key, label, cc:{mark, reason, tech}, cx:{…}}`; `mark` is `yes` \| `partly` \| `notyet` \| `no` | What each agent's capture can see | `docs/ADAPTER-CAPABILITIES.md` exists, but its rows are different (transcript ingest, re-ingest, byte preservation, vendor-sweep survival, repo identity, session status, Assured Capture, lifecycle hook sensor, recovery injection) | **no** (not served; rows differ) |
| `unsupportedAgents[]` | `{name, reason}` | "Gemini · Not supported" | `/api/lanes` → `gemini.roadmap: true`; ADAPTER-CAPABILITIES.md | partial |

### Pause and leave out

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `exclusions[]` | `{key, path, note, on, builtIn, source, basis}` | Paths left out of shown and exported views | `GET /api/hq/projects/<id>/capture` → `holds.exclusions[]`; active defaults and typed paths from the capture-hold record | yes (C46) |

The live page acts only for the project in front of the person. `holds.exclusions_honoured` must be true before its rows or Add a path can write. The three built-in rows keep the artboard labels `.env  .env.*`, `secrets/` and `generated files`, with on/off and notes from the server; typed rows show their recorded key. The checkbox sends `excluded` or `included` through write address 27, then re-reads capture holds. The typed path editor is live-only and sends the same act. A refusal keeps the draft and says nothing was saved. The page says: “Never show or export these paths. ThruLine's sealed copy of each transcript still holds what Claude Code wrote.” The server trims and validates a typed path; it does not change the sealed copy. The design gallery still draws its original note and three local checkboxes. The editor is the new control proposed for CTO review under C30 R-NAV and DESIGN-SPEC §D7.

### Checks and permissions

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `checkPermissions[]` | `{project, allowed:true\|false\|null, at?:{time, day}, hasRecords}` | The gallery's sample project rows | Demo data only; live permission is folder-scoped below | demo |
| `runPermission` | `{status, folders:[{id, folder, state, actor, at, change_id, latest_row, historical, action}]}` | Who allowed checks in each exact recorded worktree folder, including historical grants | `GET /api/hq/projects/<id>/run-permissions`; Settings uses guarded Change permission writes and historical revoke address 31, then re-reads | yes (C46) |
| `checkWhere` | `{project, folder, choices:[{change, folder}]}` | The current Change folders whose Check sheets can show other run environments | The selected project's `GET …/run-permissions` rows; each choice opens the existing Check sheet, which reads its own served environments | yes |

The live permission rows are one per exact recorded folder under the selected HQ project. The server gives Settings a safe display label and opaque folder id, never a raw path. A current Change can grant or revoke only with the folder and latest-row guards; a standing historical grant can be revoked through row 31. An old folder cannot be granted from Settings. A missing or untrusted cache is not an active permission. Loading, failure and pending states keep actions disabled, and every write answer is followed by a fresh read before the row claims a new state. The gallery retains its three sample rows. The live row scope and historical control are approved by C48 §5, as recorded in `docs/hq-next/C46-SETTINGS-RUN-PERMISSION-CTO-SHEET.md`.

“Look somewhere else…” uses only current Change IDs in those same served folder rows. One current folder opens that Change's existing Check sheet; several offer a folder choice first. With no current Change or an unread folder list, Settings says why and offers no guessed destination. The Check sheet reads its own list of environments and check, so opening it starts nothing. “Run it here” still uses the existing token, permission and run guards; the server refuses unsafe or stale places. A completed run or closing the sheet re-reads Settings permissions.

### Rules

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `templateRead`, `criteriaTemplates[]` | `{status, rows, detail}`, `{key, text, lines[]}` | Named lines from the selected project's committed base policy; each line is previewed, then may be picked in What counts as done | `GET /api/hq/projects/<id>/done-templates`; project identity is checked before showing rows | **yes, read-only** |
| `policy.path`, `policy.text` | string | The policy file path; its full contents are not served | `.thruline/policy.yml`, parsed by `crates/policy` (`parse.rs`) | **partial** |
| `policy.wouldNotBeReady` | `{changes, ofLast}` | Live preview: "2 of your last 10 Changes would not be ready" | Nothing | **no** |
| `policy.example.name` | string | A Change's short name for the "What counts as done for …" link | Change list | partial |

### Privacy

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `masking.detector`, `masking.masked` | string, int | "detector v2 · 3 masked · best-effort" | `crates/core/src/redact.rs` `DETECTOR_VERSION = "v2"`; each masked text carries a `MaskRecord` | partial (per text, not a page total) |
| `egress.localModel` | bool | Local model notes on or off | `GET /api/mind/status` → `available` (a live check). There is no on/off setting; it is configured only by `THRULINE_LANGUAGE_*` environment variables | partial (no setting) |

### Preferences

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `prefs.face` | `plain` \| `technical` | Plain or Technical face | `GET` / `POST /api/preferences` `face` (C30 R-PREFS): the one stored face, drawn by every screen's top-bar button and by `t` | yes |
| `prefs.maskedCopyOnExport` | bool | Make a masked copy on export | `masked_copy`; the whole story's export rail starts with its redacted copy ticked when true (the labelled file itself is T-5-4's) | yes |
| `prefs.colourVision`, `prefs.density`, `prefs.motion`, `prefs.keys` | bool / enum | Appearance and keyboard | `colour_vision`, `density`, `motion`, `keys`: attributes on the app root and the Inbox / Changes rows (appearance.css, data/palette.js) | yes |
| `prefs.notifications`, `prefs.weeklyDigest` | `{ready, not_backed, agent_waiting, recording_stopped}`, bool | Four open-page browser notifications and the weekly card | `GET`/`POST /api/preferences` store four Off-by-default switches and the weekly choice; first switch-on asks browser permission. Not backed polls opaque Stop-audit finding IDs; the other three follow only the open project. The weekly card reads `/api/hq/weekly-summary` once when eligible and exports Markdown on demand. | yes (C51) |

On the record every value is the stored one (`appearance.js` `storedSettings`); the gallery keeps `DEMO.prefs`. A write that is in flight, a refusal, a read still out and a read that failed are each said beside the control (`appearance.js` `appearanceText`), and the words are the ones every screen uses (`data/preference-words.js`).

### Previews and upgrade

| Field | Type | Meaning | Real source | Status |
|---|---|---|---|---|
| `statePreview` | `{version, scope[]}` | Sample for the colour preview pill | Any proven statement, or fixed sample text | n/a (illustrative) |
| `upgrade.estimate` | string \| null | Pending upgrade's time estimate | `upgrade::estimate_phrase` (cli); no route | **cli** |

### Design only

- `variants`: what the canvas's DEMO STATE switcher lays over DEMO (`dead`, `upgrade`, `stopped`). It is read only from `DEMO`.

## Settings that exist in the product today, and what the design shows with nothing behind it

**Exist (served or real today):**

- **Records kept on this machine** (`GET /api/storage`): total size, location and size by part.
- **Codex log observation** (`GET` and `POST /api/observation`): state, plus pause and resume. The pause is held in memory and only works when HQ was started with `--observe-codex`. This is the only real "pause".
- **Agent liveness** (`GET /api/lanes`): whether Claude Code and Codex wrote recordings in the last 5 minutes; Gemini is on the roadmap.
- **Local model availability** (`GET /api/mind/status`): available or not, with model and provider.
- **Plan and licence** (`GET /api/licence`): read only; a licence is installed from the terminal.
- **Integration presentation and actions** (`GET /api/integrations?session=`, `POST /api/integrations/action`): 23 actions.
- **From the terminal only:**
  - `thruline init` (global), which installs the sensor hooks, the MCP server, the speaker hooks behind consent, and the optional stop gate with `--gate`; `--undo` removes them;
  - `thruline sensor doctor`, `thruline doctor`;
  - `thruline shim install|uninstall|status` (run capture);
  - `thruline upgrade`;
  - `.thruline/policy.yml`;
  - `.thruline/redact.yml`;
  - the redaction detector `v2`.

**At the Wave 2 baseline, the design showed these controls with nothing behind them:**

- Pause recording per project (Claude Code).
- Never record these paths.
- Per-project check permission, and where checks run.
- Reusable done-criteria lines, and the policy's live preview.
- Sizes by project, and Remove.
- The capability matrix as drawn.
- The speaker as a per-project switch.
- Local model notes as an on/off switch.
- A masked-copy-on-export preference.
- Notifications and the weekly digest.
- Colour-vision variant, density, motion and the single-key shortcut mode.
- A Codex app-server connection and "Try again".
- Recording state on this page, beyond file-time liveness: hooks dead or upgrade pending.

At that baseline, no control on this page wrote anything. C46's pause, speaker and exclusions work above supersedes those three rows.

## Wave 2: what the live page reads, and what it says where nothing backs a control

`index.jsx` reads the six addresses in `writes.js` `READS`. `from-server.js` turns them into this contract (pure, tested); `live.js` says each fact in plain words after the gallery values are computed, so `#/design/Settings` is unchanged (0.00%).

| Shown | From | Live behaviour |
|---|---|---|
| Records kept, total and by part | `/api/storage` `bytes`, `parts[]`, `path` | "ThruLine is keeping 5.4 GB …" from the served total; parts named by key; Remove… disabled ("Removing is not available in this version: ThruLine removes nothing yet.") |
| Claude Code row, pill, status bar | `/api/lanes` `claude_code.live`, `window_minutes` | "Recording now" when a recording was written in the window, else "Nothing new"; hook list corrected to 7 + 1 dormant, installed for every project by `thruline init` |
| Codex row | `/api/observation`, `/api/lanes` | "Not watched live" / "Watched live" / "Paused by you"; Try again re-reads `/api/observation` |
| Pause switch | `/api/observation` | writes `POST /api/observation {action}` only when HQ watches Codex; otherwise disabled with its reason |
| Local model notes | `/api/mind/status` `available` | "Running" / "Not running" / "Not read" |
| Plan | `/api/licence` `status` and `entitlements.approve_version.allowed`, through the one rule `ui/src/hq/data/plan.js` `planOf` (C30 P0-6: Settings, Approve and Team call it) | "Plan: Pro" only for a valid licence the server lets approve; "Plan: Free" for any other answer it can read; "Plan" alone when the answer can't be read. Then "Approving a version needs Pro. Everything else is free." + See plans → https://thrulinehq.com/pricing |
| Who allowed checks | `/api/hq/projects` | one row per project (a loop of the canvas's own row markup), "Not recorded", with its reason |
| Run capture | none (CLI `thruline shim status`) | "Run capture: shown in a terminal"; the 15 wrapped tools named |
| Face, density, motion, colour vision, single keys, masked copy, four notifications, weekly summary | `GET` / `POST /api/preferences` (`data/preferences.js`, C30 R-KEYS and R-PREFS) | each control reads the stored value and writes it; the sentence beside it says what is in flight or was refused. Notifications require browser permission before On is kept; the weekly summary is a dismissible first-open card with Save as Markdown. |
| Speaker and exclusions | the project install plan and `GET /api/hq/projects/<id>/capture` holds | live when read; writes at addresses 20/30 and 27; exclusions are applied when shown or exported (C46) |
| Templates and policy preview | `GET /api/hq/projects/<id>/done-templates` | named lines previewed from the base branch; absence, empty policy, unreadable policy and read failure are distinct. Offer/Stop stays disabled because Settings has no action to save a changed base policy. The hypothetical policy impact count stays unavailable. |

`/api/integrations` is not read: it answers per session (`?session=`), and describes one session's integration, not a setting.
