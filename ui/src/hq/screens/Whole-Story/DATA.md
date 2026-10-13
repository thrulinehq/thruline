# Whole-Story — data contract

`logic.js` reads content only from `const D = this.props.data || DEMO` (`demo.js`). `this.state` holds UI state only: export format, redaction classes ticked, saved, copied, a failed save or copy, and the face (Plain / Technical). With `props.data` present, `isDemo` is false, and these design annotations are not drawn:

- the rail's "Design notes · not in the file" section;
- the per-format engine notes ("NEEDS ENGINE: An HTML copy that re-checks…", "ENGINE: The verification-summary shape…").

The template gates both on `v?.isDemo`, and `fmtEngineShow` is also `isDemo && …` in `logic.js`.

This is one Change's whole story at one version: the review packet as a page. Most facts exist in the store across several Change routes. The gaps are noted per row.

Times are local `HH:MM` on `story.date` at `story.utcOffset`. "step N" is a record step number. Versions are 7-character ids.

| Field | Type | Meaning | Real source | Store has it? |
|---|---|---|---|---|
| `project.name`, `project.root` | string | project and its worktree (`~/code/checkout-web`) | project record (recorded worktree) | yes — `/api/hq/projects/<id>`; the root must be masked/home-relative before serving |
| `change.id`, `change.title` | string | the Change; `id` is used as the slug in the export file name | Change | yes — `GET /api/changes/<id>` |
| `version.id`, `.commit`, `.current`, `.unsavedChanges` | string, string, bool, bool | the version described, its commit, whether it is the latest, and whether the tree had unsaved changes | Change versions | partly — `/api/changes/<id>`; commit ↔ version and the clean-tree flag are unverified |
| `recording.following` | bool | "Recording · following live" | capture state | unknown |
| `readAt` | 'HH:MM' | page read time | server clock | no |
| `storage.bytes` | int | drawn as `x.y GB` | storage census | yes — `/api/storage` |
| `story.{date, utcOffset, from, to, asOf}` | ISO date, '+HH:MM', 'HH:MM'… | the day and span the record covers, and the "as of" moment | first/last recorded step of the Change | partly — derivable from session timelines (`/api/hq/sessions/<id>`) |
| `sessions[]` | `{agent}` | sessions in the Change ("1 session · Claude Code") | session associations | yes — `change_session_associations`, `/api/changes/<id>/associations` |
| `recordedSteps` | int | steps in the record for this Change | session timelines | yes — derivable (`/api/hq/sessions/<id>`) |
| `approval.{version, by, at, criteriaVersion, decisionFile}` | — | the version-bound approval | `change_decisions` (T-S-4), `.thruline/decisions/<version>.json` | yes — `decisions` in `GET /api/changes/<id>` (schema v3) |
| `criteriaAccepted.{version, by, at, afterWork, lastAgentEditAt}` | — | when "what counts as done" was accepted, and whether that was after the agent's last edit ("bar set after the work") | `change_scope_acceptances` + the last edit's time | partly — the acceptance is stored; `afterWork` and `lastAgentEditAt` are derived |
| `timeline[]` | bands `{kind: 'unrecorded'\|'session'\|'version', version?, current?, events[]}` | "How it went": one band per stretch of the record | session timeline grouped by version | partly — `/api/hq/sessions/<id>` (timeline), `/api/hq/projects/<p>/threads/<t>` (versions); the grouping is new |
| `timeline[].events[]` | `{kind: 'gap'\|'prompt'\|'run'\|'statement'\|'edit'\|'accept'\|'approve', step?, at?, passed?, failed?, restsOn?, by?, criteriaVersion?, before?}` | one event; `restsOn` marks the lit thread (what the approval rests on); a run with `passed: null` has no result yet | prompts/edits/commands/claims in the thread view; run tallies (`crates/core/src/hq/tally.rs`, `null` when the runner printed no summary) | partly — prompt/edit/command/claim items exist in `hq::view`; `restsOn` needs the readiness derivation |
| `criteria[]` | `{n, gate, text, linkBasis: 'person'\|'run_names_subject'\|'accepted_checks', proof}` | accepted criteria; `linkBasis` picks the "why it was looked at" sentence | `change_criterion_evidence` (basis) | yes — `/api/changes/<id>/coverage` |
| `criteria[].proof` | `{state, restsOn:{kind, step}, ranOn, changedSince, scope[]}` | the ①–⑥ chain for one line | coverage + freshness | partly — `/api/changes/<id>/coverage`, `change_criterion_freshness`; `scope` per run is unverified |
| `runs[]` | `{step, at\|null, command, destructive, destroys?, passed\|null, failed\|null, version\|null, capturedBy?: 'thruline', scope?, permissionAskedAt?}` | every run, in the order shown; `null` result = no result yet | `change_run_executions` + command items + tally | partly — commands and tallies yes (`hq::view` `CommandItem`/`TallyItem`); `destructive`/`destroys` and `permissionAskedAt` are unverified |
| `statements[]` | `{agent, step, at, quote, kind: 'checkable'\|'done_claim'\|'unchecked', claim?, word?, versionBefore?, checks?}` | what the agent said. `claim` = the checkable proposition ("tests pass"); `word` = the unverifiable word in a done claim ("fixed"); `checks` = how the record read it over time | claims (`/api/claims`, `hq::view::ClaimItem`) | partly — quotes and steps yes; `checks` history (state at 16:04, then 16:09) is not served |
| `statements[].checks[]` | `{at, state, version, restedOn?, changed?:{file, step}, stoppedAt?, restsOn?:{step, matchedBy, sameCommandAs, scopeAdded}}` | each re-read of a statement | readiness/freshness re-evaluations over time | **no** |
| `findings.{foundAt, groups[]}` | groups of `{findings:[{n, text, where, destroys?}], settlement:{kind:'settled'\|'nothing'\|'open', at?, text, ref?}}` | findings and how each group was settled | `change_review_findings` + `change_review_finding_standings` | partly — `/api/changes/<id>/findings`; the grouping by settlement and the engine-written `text`/`where` sentences are unverified |
| `decisions[]` | `{kind:'accept'\|'run'\|'approve', at, …, stateAfter?:{state, version}, file?}` | the version-bound decision history | scope acceptances, runs, `change_decisions` | partly — each source exists; no merged history route |
| `gaps[]` | `{kind:'before_capture'\|'no_result'\|'code_at_statement'\|'edit_line_counts', at?, step?, askedAt?, lastVersion?}` | what the record does not hold | capture start, pending runs, version-at-claim, edit line counts | partly — derivable; no route |
| `integrity.{ok, checkedAt, steps, chainHead}` | bool, 'HH:MM', int, sha256 hex | the hash-chain check over this Change's steps; `chainHead` is drawn as `8…4` when it is a full 64-hex hash | `thruline verify` over the events | unknown — the verify exists as a CLI; no route serves it per Change (the canvas note cites IR #19) |
| `masking.{detector, masked}` | string, int | masking applied before the page was served | `redact.rs` | partly — `/api/changes/<id>/export` serves the treatment beside each path |
| `redaction.{paths, agentWords, failureOutput}` | int | what a redacted copy would leave out, by class; a class with 0 is disabled | export redaction census | partly — `/api/changes/<id>/export` (T-5-4) |
| `readable.{checkedAt, records[]}` | records `{kind:'decision_file'\|'run'\|'transcript', path?, step?, capturedBy?, agent?, files?, readable}` | whether each record the story rests on can be read **now** | export readability | yes — `GET /api/changes/<id>/export` (T-5-4: missing · permission denied · content changed · dangling reference · never written) |

## Computed in `logic.js` (never stored)

- Every sentence on the page: approval line, "It rests on N of M…", the runs paragraph (captured steps joined "26, 46 and 62"), the findings header counts, decision texts, gap sentences, contents counts ("3 ran · 1 no result"), and the export preview lines and file name (`thruline-packet-<change.id>-<version.id>[-redacted].<ext>`).
- Run result words: "F failed, P passed" when anything failed, otherwise "P passed, F failed"; "no result yet" when `passed` is null.
- Timeline glyphs and colours by event kind; orange only on `restsOn` events.

## Markup limits wave 2 must know

- Fixed slots, not loops: the two said-blocks (statement 1 with its two checks, statement 2), the "also said, not checked" line, the five findings in three groups (3 + 1 + 1), the three decision-history rows and the four gaps. Live data with other shapes needs markup loops, which this wave was not allowed to add.
- Each criterion row draws the chain as **all-holding / Proven** statically (dots, colours, ● mark). Only the text holes vary. A criterion in another state needs presentation work. **Corrected at C30 P0-1:** ⑥ So, the five link connectors and the "why" chip now read from `logic.js` (`lineVerdict`). ⑥ So draws the line's own state with the §1.4 tokens, "Proven · version X" only when the record names the version X the line is proven on, and nothing when the record holds no state; the links are lit only on such a line. The numbered discs are still static.
- "The record checks out" is static. `integrity.ok = false` has no drawing yet.
- The approval chip "◆ Approved on …" is static. A Change that is not approved has no drawing yet.

## Wave 2 — the live page (W2-E)

How the page is built:
- **Snapshot.** `index.jsx` calls `data/use-change.js`'s `loadChange(at)`, which produces the ONE snapshot from `data/change.js`.
- **Tracked Change.** It also reads the tracked Change's documents and derived model, through the same `readChangeDocuments` / `deriveChangeModel` pair that use-change.js uses.
- **Selector.** It hands both to `wholeStoryFrom(snapshot, tracked)` in `from-change.js`. That selector chooses and groups; it re-derives no verdict, criterion, readiness or decision.

| Section | Live source |
|---|---|
| Header, version | `snapshot.project`, `change`, `currentVersion`, `versions[]` (commit, `unsavedChanges`) |
| How it went | `edits`, `runs` (tests), `statements` grouped by version. Only the last `MAX_VERSION_BANDS` (2) are drawn; earlier ones are summed into one band. The lit thread is drawn only when an approval stands |
| What counts as done | `snapshot.criteria` plus `tracked.hq.criteria[]` (text, `state`, `proven.version`, `proven.scope`) and `tracked.hq.version.short`. ⑥ So is each line's own state; "Proven" only with the version its proof names; no state, or Proven with no version, draws no verdict (C30 P0-1). Without a tracked Change it says "What counts as done is not accepted yet" |
| What the agent said | `snapshot.brief.statementId` (the statement the Brief leads with), its `proof` (verdict, `stopsAt`, `restsOn` → run, `since` → edit, `recheck`); the latest task-complete claim; one other statement |
| Runs | the last `MAX_RUNS` (12) test runs; the note says how many were left out |
| Findings | `snapshot.findings[]` (typed kinds) plus `tracked.hq.packet.findings[]`. **A loop** |
| Decision history | `tracked.change.accepted_scope.acceptance` plus `tracked.change.decisions.history[]`. **A loop**; with none, "No decision has been recorded for this work." |
| What was not recorded | derived from the snapshot: the time before the first recorded step, test runs with no result, the code at the moment the agent spoke, edits with no line counts, and criteria not accepted. **A loop** |
| Record integrity | not served for a thread, so it reads "Not checked on this page" |
| Export (C30) | `export.js` makes the file from the page's own values (`page`, `bands`, `crit`, `runs`, `words`, `readable`, notes, counting second approvals, objections) and nothing else: Markdown, HTML and JSON are three walks of one model. The preview holds every line of it; Download saves exactly that text (Blob → download, `export-io.js`), named `thruline-packet-<title slug>-<version7>[-redacted].<ext>` by the workspace's `filenameOf`; Copy as Markdown copies the Markdown file's text. A redacted copy is a second file: its label first (each class and its count, the paths rule, `derived_from_sha256` of the full file in the same format), a marker in place of every removal, the counts being the markers made. What a class removes comes from typed fields: paths = the project root, the edit file a statement's proof stopped at, finding files, decision files, proven scopes and the record paths Readable now names; the agent's words = the quotes the page shows. Nothing of a check's printed output is on the page, so that class is 0 and off. The page's times stay local `HH:MM`; the file's technical part gives the approval and the look at the records in UTC with `Z` |
| Readable now (C30) | `GET /api/changes/<id>/export`, read by `index.jsx` for a tracked Change only; each record is drawn with the route's own word (readable, missing, permission denied, content changed, dangling reference, never written, could not be read) and the sentence counts from the route's own `totals`. Untracked work says it applies to a tracked Change only. In the file, each record that cannot be read is also named at the top of the section it belongs to |
| Face (C30) | `this.state.face`, read through `faceOf` (a `face` prop wins, so an app-wide preference can set it; `onFace` hears each change). Technical draws the file's own technical part on the rail (format, ids, the moments in UTC, masking, each record's code) plus this exact file's sha256 and size, and each Readable-now row's record id and code |

Times on the page are this machine's local `HH:MM`, with no offset printed.
