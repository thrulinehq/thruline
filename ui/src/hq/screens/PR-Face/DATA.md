# PR-Face: data contract

`renderVals()` reads `this.props.data || DEMO` (`demo.js`). A live page's `data` is `prFaceFrom(snapshot, ctx)` (`from-change.js`). The snapshot is the ONE Change model, `ui/src/hq/data/change.js`; the snapshot and the tracked Change's documents and `deriveChangeModel` reading come through `Repair-Loop/live.js` `useLive`, read again every 20 s. The live address is `#/changes/<id>/pr?project=<pid>`.

The page is a **preview** (C30 R-PR): the note for this Change's pull request, and what the ThruLine Action would post for the Change's folder. ThruLine posts nothing and makes no network call; there is no post button. Its one way out is **Copy as Markdown**, which copies `data.copy`: the comment exactly as drawn, then the Action setup line drawn under it. **Work nobody tracked has no note**: the page says what a tracked Change's note shows and offers **Track this work as a Change** (the existing client, `Repair-Loop/actions.js` `trackThenReread`, `POST /api/first-use/change {project, thread}`).

## `ctx`

| Field | Meaning | Source |
|---|---|---|
| `now` | the read time | `Date.now()` at render |
| `tracked` | `{id, change, model, …}` or null | `useLive` (`readTracked`: `readChangeDocuments` + `deriveChangeModel`) |
| `pr` | `{doc, error, loading}` | `read.js` `usePrPreview`: `GET /api/changes/<id>/pr` (`crates/cli/src/uiserve/pr_preview.rs`, docs/hq-next/API.md) |
| `first` | `{facts, readAt}` | `index.jsx`: the facts of the first read on this page |
| `track` | `{busy, said}` | `index.jsx`: where "Track this work as a Change" stands |

## Fields

| Field | Type | Meaning | Live source |
|---|---|---|---|
| `live`, `tracked` | boolean | a live page; a tracked Change | — |
| `board.crumbKind`, `board.crumb` | string | "preview", the project | constant; `snapshot.project.name` |
| `host.title` | string\|null | the title row | `snapshot.change.title` |
| `host.meta` | Inline[] | the second host line | tracked: the project, then the route's `git.branch` and `git.head` (7), or "no branch, head …" (detached), or "branch not read: <reason>", or "reading the branch…"; untracked: the project and the version |
| `host.number`, `host.author` | null | no pull request is linked | — |
| `comment.readAt` | string | `YYYY-MM-DD HH:MMZ` (UTC) | `now` |
| `note` | Note\|null | the comment (below); null when untracked | `composeNote(noteFacts(snapshot, ctx))` |
| `markdown` | string\|null | the comment's Markdown | `markdownOf(note)` |
| `setup` | Inline[]\|null | the Action setup line, drawn under the comment | `setupOf({newest, checkName})`: the Action's own command (`packaging/action/action.yml`) on the folder's newest receipt, else `latest.json` (`docs/book/src/ci.md`) |
| `copy` | string\|null | exactly what Copy as Markdown copies | `copyOf(note, setup)` = `markdown` + `\n` + the setup line + `\n` |
| `approval` | `{href, label, stands}`\|null | "The approval it waits on →" | `routes.js` `href('approve', …)`; "Approved on <v>" (and "· code changed since") when `change.decisions.standing` names an approval |
| `checkRun` | object\|null | section 3 | the route: `check_run.name`, `folder`, `receipts` (none → the Action would post nothing; some → the newest, unchecked here, with the command), `badge.states` |
| `trailer` | `{text, subject, note}`\|null | section 4, the commit trailer | `ThruLine-Approved: ` + the standing approval's version (`change_decisions`, through `model.decision.version`), else none and why |
| `badge` | `{lines}`\|null | section 4, the README badge | the CLI's words, served (`badge.states[].label`); no badge is drawn |
| `edited` | `{title, lines, delta}`\|null | the edited-in-place panel | `changesSince(first.facts, facts)`: runs, edits, statements added, line states, approval |
| `untracked` | `{why, what, next, busy, said}`\|null | the offer for work nobody tracked | `UNTRACKED`, `ctx.track` |

## Note (markdown.js)

`{ marker, title, lines[], table|null, tableNote|null, tableAfter|null, layers[{open, summary, items[]}], version, footer, firstLines[] }`. Every text is an Inline list: a string (ThruLine's words), `{code}`, `{rec}` (recorded words, escaped), `{strong}`, `{em}`, `{glyph}` and `{stop, label}`. The preview (`inline.jsx`) and the Markdown (`markdownOf`) are both drawn from it.

| Part | Live facts (`noteFacts`) |
|---|---|
| marker | `change.id`, `currentVersion`, `accepted_scope.version` |
| title | `change.title`, `currentVersion` |
| line 1 | the standing approval (`decisions.standing` + `model.decision`: version, moved since, actor, time); else not accepted; else `model.readiness.ready`; else the lines not proven, with the model's leading state word |
| line 2 | `model.readiness.blocker.reason` — the one reason every surface gives |
| line 3 | to settle it: nothing left (ready or approved; the last run's UTC time), accept what counts as done, or run the Change's check |
| line 4 | for approval: who approved and when; the approval names an earlier version; not accepted; not approved yet; or waits on N of M lines |
| proof stops | per line: ⑤ for Needs a re-check, ② for a line nothing rests on |
| table | **one row per line of what counts as done** (the first 12; the rest are counted): the model's state mark, the line's words, its state word, and its chain — accepted (UTC) → the first piece of evidence → the Proven line, the freshness sentence, or the state line (or the coverage sentence when nothing rests on it) |
| layer 1 | every statement the agent made, each with its own chain (the last 8; the rest counted) |
| layer 2 | `runs[]` with `kind: test` |
| layer 3 | sessions, steps, edits, commands, versions, the earliest time, runs with no result |
| layer 4 | the lines not proven, as a prepared hand-off (not sent) |
| version | the current `versions[]` row: `commit`, `unsavedChanges` |
| footer | `currentVersion`, the read time, `masking` |

**Absences said in words, never 0:** no title, no version, no tally, no result, no masking record, no recorded time, nothing accepted, no line, no approval, no receipt, no branch.

**Times:** UTC with `Z`, from served ISO timestamps only.

## Design narrative (gallery only, `isDemo`)

Drawn only at `#/design/PR-Face`: the illustrative caption; the 16:09 panel's fixed text; the three version↔commit forms and the host warning; section 3's ruling R17 table; section 4's trailer and two badges as drawn; and **section 5, the six contrasts**, which never reach the product (C30: annotations are not features).
