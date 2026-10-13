# Step-View — data contract

The page at `#/changes/<id>/step/<n>[?project=<p>]` (C30 R-ROUTES "Step view"). It is not a canvas artboard: there is no demo record and no gallery form. It is the target of every receipt chip ("Run step 12 ›", "Edit step 9 ›", "Said step 4 ›") and shows one step of the record, the receipt it was read from, and the session's own lines around it.

Reads only. It reaches no write address.

## What a step is

A step is a row's place in the thread's record: the Change's asks, edits, commands and claims, in the order they were recorded (time, then ask < edit < command < claim, then served order). The numbers are the ones every other screen uses — `data/change.js` numbers each row once (`stepOf`), and `snapshot.steps[]` is that numbering written out.

A check ThruLine itself ran (from HQ's run sheet) is **not** in the agent's record. It has no step (`step: null`), so it has no page here and its chips do not link.

## Address

| Part | Meaning |
|---|---|
| `<id>` | a thread id (16 hex) or a tracked Change id (`chg_…`), as every Change address |
| `<n>` | a whole number from 1. Anything else (`0`, `-3`, `1.5`, `abc`, no segment) parses to `step: null`, and the page says there is no such step: it never redirects and never falls back to the Inbox |
| `?project=<p>` | the HQ project id; chips carry it so the thread is found without searching every project |

`routes.js`: `href('step', {change, step, project})`, `parse('#/changes/<id>/step/<n>')` → `{screen: 'step', change, project, step}` (`step: null` and `asked` for an address that names none). No canvas link names the page (`LINK_TARGETS` has no `Step-View`): a step needs its number, which no canvas link carries.

## Reads

| Read | For | Route |
|---|---|---|
| the Change snapshot | the step, its neighbours, the project and title | `use-change.js` → `change.js` (`GET /api/hq/projects/<p>/threads/<t>`, plus the tracked Change's documents when there is one) |
| the session timeline | the lines around the step | `GET /api/hq/sessions/<sid>` for the step's own session only, read once per session |

The server masks every string it serves (`redact::mask_recorded`). The page writes the home folder as `~` in every path and every word it draws (`paths.js`).

## `snapshot.steps[]` (added C30 R-ROUTES, `data/change.js`)

One entry per row, `steps[k].step === k + 1`:

| Field | Type | Meaning | From |
|---|---|---|---|
| `step` | integer ≥ 1 | the step number | position in the ordered record |
| `kind` | `prompt` \| `edit` \| `command` \| `claim` | which row it is | the served array it came from |
| `id` | string | the row's id (`<event>`, `<event>#<n>`) | the served row's `id` / `event_id` |
| `eventId` | string | the session line it is: the id with any `#<n>` cut | `event_id` |
| `ts`, `time` | RFC 3339, `HH:MM` | when it was recorded | `ts` |
| `sessionId`, `agent` | string | which session and which agent | `session_id`, the session's `source` |
| `version` | short id or null | the version current at the row (never adds a version to `versions[]`) | `version_id` |
| `receipt` | `{file, line}` or null | the recording and line it came from, the home folder as `~` | `raw_file`, `raw_line` |
| `prompt` | `{text, segments, work}` or null | an ask: its words, what was typed and pasted, whether it was work or conversation | the served prompt |
| `edit` | `{file, op, action, source, linesAdded, linesRemoved, ok, makesVersion, hunks}` or null | an edit; `hunks` is the served array (lines as served, the home folder written as `~` when drawn) | the served edit |
| `command` | `{command, isTest, isCommit, outcome, exitCode, tally, tallyLine, output, outputTotal}` or null | a command; exit, tally and output exactly as the record states them (`null` when it states none) | the served command |
| `claim` | `{text, claimKind, verifier, statementId}` or null | a sentence the agent said that rules apply to | the served claim |

## What the page draws, by field

| Part | Fed by | Never |
|---|---|---|
| heading ("Claude Code edited a file", "ran a test", "ran a command", "committed", "said", "You asked") | `kind` + `edit.action` / `command.isTest`/`isCommit` / `prompt.work` | a verb the row does not support |
| the record | the row's own detail | a line the record does not hold (an edit with no hunks says so) |
| "Where this came from" | `receipt`, `time`, `version`, the session's place among the Change's | a recording or line the row does not name (it says so) |
| "The session around this step" | the session timeline's items next to the item with this `eventId`: three before, three after, the step marked, the rest counted | other lines when the step is not among them (it says so); anything while the read has not answered ("Reading the session…") or has failed (it says so) |
| "Around this step" | `steps[]` within three of this one, each a link to its address | a number the record does not hold |
| "no such step" | the address's `asked`, the Change's `stepCount` | a redirect; a link to the Inbox |

## Chips elsewhere (the wiring)

`Step-View/link.js` is the one place that decides whether a chip may link: `stepHref(at, step)` is `href('step', …)` for a whole step ≥ 1 and `undefined` otherwise. `Step-View/chip.jsx` draws a chip as the canvas's own element in the design gallery, as a link on a live address with a step to open, and as plain text (never a dead button) on a live address without one.

| Screen | Chips that open a step |
|---|---|
| Every-Statement 20 | Said, Run, Version, Edit; "How this was decided" opens the record the verdict turns on (the edit that moved the proof, else the run that did not hold, else the run it rests on). "Accepted" names a version of what counts as done, which sits at no step: text. |
| What-Changed 11 | the record chips (Run, Edit) and each hunk's "Edit step …›" |
| Whole-Story 4 | "Run step N ›" under "What the agent said". A line of what counts as done has no run of its own to open yet: text. |
| Change-Brief 8 | the plates' record chips (Said, Run, Version, Edit) and the steps named in "How this was decided" |
| Repair-Loop | the run chip, when the run is in the agent's record; a check ThruLine ran keeps its link to The whole story, where the run is listed |

## Store today

| Fact | Store has it |
|---|---|
| step numbers, kinds, times, receipts, detail | yes — the thread view |
| the session's own lines | yes — `GET /api/hq/sessions/<sid>` |
| the step a version was recorded at (`versions[].recordedAtStep`) | no — null live, so no Version chip is drawn live |
| the step at which what counts as done was accepted | no — null live, so "Accepted" is text |
