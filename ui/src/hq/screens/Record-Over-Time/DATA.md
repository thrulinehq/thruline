# Record-Over-Time — data contract

`logic.js` reads content only from `const D = this.props.data || DEMO` (`demo.js`). `this.state` holds UI state only: range, agent filter, project filter, chosen proof-chain link, menu, toast, banner dismissal, Codex retry and focus ring. With `props.data` present, `isDemo` is false and the canvas's "Demo records. Illustrative…" banner is not drawn. The template gates it on `v?.isDemo`, and `logic.js` also sets `banner.show = isDemo && …`.

This screen is **history across many Changes and sessions**. Every number on it is an aggregate over **statements** (claims an agent made), counted per window × project × agent. Nothing serves these aggregates today. They are the main engine work this screen needs.

## The aggregates the store must compute

| Aggregate | Grain | Window(s) | Notes |
|---|---|---|---|
| **Tally** (15 counts, below) | per **window** × **project** × **agent** (`cc` = Claude Code, `cx` = Codex) | each of `windows` = 7, 30, 90 days, ending **today** (local date), counted by the day the statement was **made** | Additive. The screen sums across projects and agents for "All projects" and "Both agents", so the store serves only the finest grain |
| **Median minutes from statement to proof** | per window × scope (`all` or a project id) × agent (`all`, `cc`, `cx`) | 7, 30, 90 days | **Not additive**, so every combination the screen offers must be served: (1 + projects) × 3 agents × 3 windows. Absent = no statement proven in that slice; drawn as "—" |
| **Daily series** | per **project** × **agent** × **day** | the longest window: **90 days**, index 0 = 89 days ago, index 89 = today | Two counts per day: `statements` made that day, and `proven` = of those, proven on their version **now**. `null` = that agent was not recorded that day; it is never a zero |
| **Project lifetime** | per project | all time | Used only by the empty states: total statements, statements by agent, the date of the last statement |

Shorter windows read the tail of the 90-day series. Tallies are served per window rather than summed from the series, because a tally counts state **now** (needs a re-check, reopened, re-proven, …), and the series does not carry that.

## Fields

| Field | Type | Meaning | Real source | Store has it? |
|---|---|---|---|---|
| `today` | ISO date | the last day of every window; dates in empty states are counted back from it | server clock, local date | no — trivial to serve |
| `readAt` | 'HH:MM' | when the page was read; also "Tried again at …" after a Codex retry | server read time | no |
| `inbox.yourMove` | int | sidebar badge | Inbox contract (W1-A) | yes, via the Inbox |
| `storage.bytes` | int | drawn as `x.y GB` (1e9) | storage census | yes — `/api/storage` |
| `windows` | int[] | the range buttons, in days | product constant | n/a |
| `projects[]` | `{id, name, recording, changes, dismissedChanges, lastRecordedOn: ISO\|null, lifetime:{statements, byAgent:{cc,cx}, lastStatementOn: ISO\|null}}` | every project on this machine, recorded now or before. The sidebar sub-line is "recording · N Changes", "recording · nothing yet", or "recorded <date> · N dismissed" | HQ project list + Change counts + capture state | partly — `/api/hq/projects` lists projects; `recording`, `dismissedChanges` and `lifetime` are unverified |
| `capture.claudeCode.hookVersion` | string | "Claude Code · hook v0.4" | the installed hook pack version | unknown |
| `capture.runCapture` | bool | "Run capture on" | shim install state | unknown |
| `capture.codex.answering` | bool | whether the Codex app server answers | Codex capture health | unknown. **The markup draws "Codex app server not answering" statically** (Capture panel and By-agent header), so wave 2 must gate those on this field. The chart's not-recorded band is already data-driven (from `null` days) |
| `tallies[w][project][agent]` | Tally | see below; an absent entry = nothing recorded | statement/claim records joined to proof state | **no** — `/api/claims` serves per-session claims, not window aggregates |
| `medianMinutesToProof[w][scope][agent]` | int minutes | see above | claim time → time of the run that proved it | **no** |
| `daily[project][agent].statements` / `.proven` | (int\|null)[90] | see above | claims by day | **no** |

### Tally (one window × project × agent)

| Field | Meaning (canvas letter) |
|---|---|
| `statements` | statements made (n) |
| `proven` | proven on the version they were about (p) |
| `needsRecheck` | proof moved by a later edit; no finished run on the new version yet (r) |
| `noProofFound` | not proven: nothing in the record matches (fa). The proof stops at ② RESTS ON |
| `rerunFailed` | not proven: a re-run on that version failed (fx) |
| `recordedStopsAtRanOn` | recorded only; the run was not watched, so the proof stops at ③ RAN ON (r3) |
| `recordedStopsAtSo` | a "done" claim, which settles as Recorded by rule at ⑥ SO (r6) |
| `reopened` | reopened by a later edit (re = reproven + rerunFailed + needsRecheck) |
| `reproven` | re-proven after re-running (reP) |
| `reruns.{passed, failed, stopped}` | re-runs the person started, by outcome |
| `beforeAnyRun.{total, laterProven, stillNothing}` | claims made before any run was recorded in their session; how many a later run proved; how many still rest on nothing (≤ `noProofFound`) |

Invariant the demo keeps, and the store must keep too: `statements = proven + needsRecheck + noProofFound + rerunFailed + recordedStopsAtRanOn + recordedStopsAtSo`. The headline sentence is built on it ("41 of 64 … 10 need a re-check, 6 have no proof, 7 are recorded").

## Computed in `logic.js` (never stored)

- Sums across projects and agents for "All" and "Both".
- The headline sentence, the KPI cards, the per-agent bars (scaled to the larger agent), and "where the proof stops" (counts at ②, ③, ⑤ = needsRecheck + rerunFailed, ⑥; the default lit link is the largest).
- The chart: y-axis rounded up to an even number; a day counts as not recorded only when every chosen agent is `null` that day; the not-recorded band shows if any chosen agent is `null` on any day in the window.
- Empty states come from project facts: a project with no statements ever; an agent with none in the project; a window that ends before the last statement, with a button to the smallest window that reaches it.

## What the demo cannot tell you

- The demo stores checkout-web's `lifetime` as its 90-day totals (lower bounds). The demo project is "demo", with one Claude Code statement on 5 Sep (21 days before `today` = 2026-09-26).
- The canvas staged the counts so that they agree. The live store must make them agree by construction (same claim set for the tallies, the series and the medians), not by staging.

## Wave 2 — the live page (W2-E), and why it counts nothing yet

Every aggregate above needs each claim with its time, its agent (from its session's `source`) and its proof verdict. The only document that serves those is the per-thread `GET /api/hq/projects/<p>/threads/<t>`. Measured on the scratch store on 2026-10-07 (144 threads), the costs are:

| Window | Threads | Thread documents |
|---|---|---|
| all | 144 | 167 MB, 175 s cold, read one after another |
| 30 days | 102 | 145 MB |
| 7 days | 30 | 33 MB |

A page cannot pay that. So the live page (`recordFromProjects`, `from-projects.js`) reads only `/api/hq/projects` (0.5 s, 196 KB per project view) and `/api/storage`. It shows:
- the projects, as a loop built from the design's own row markup;
- the totals the summaries type: `claims` and `threads`;
- the sources;
- storage.

It does not show tallies, medians or a daily series, and it does not draw them as zero. The empty card says "Not available in this version." with the reason. The 7/30/90 range buttons, the Projects menu and Export CSV are rendered `disabled`. The Codex "not answering" panels are not drawn, because the capture health of each agent is not served.

**Requested engine route** (proposal, not built), `GET /api/hq/projects/<id>/statements`:
- one row per claim: `{id, ts, source, kind, has_verifier, verdict, stops_at, recheck, proven_at}`;
- one row per session span: `{source, started_at, last_at}`.

That is roughly 6,075 claims × about 150 B ≈ 1 MB. `tallyKey` and `dailySeries` in `from-projects.js` are the derivations that would consume it, and they are tested now:
- the verdict comes from the Change model's `proof`;
- a day with no recorded session for an agent is `null`, never 0.


## C30 — the counts that need more than a verdict, and Export CSV (`metrics.js`)

The statements route (`GET /api/hq/projects/<id>/statements`) now serves three facts beside each verdict (`proven_at`, `run_before`, `proof_moved`: `docs/hq-next/API.md`). `recordFromStatements` keeps the rows it counted from (`data.rows`: `{pid, a, ago, key, c}`), and `metrics.js` makes these from them, per selection (range × agent × project), at render:

| On the page | Made of | Where the record cannot give it |
|---|---|---|
| Median time from claim to proof | the minutes from `ts` to `proven_at` of each **proven** statement whose proving run was recorded **after** it was said; the median of those (mean of the middle two when even) | `proven_at` not served → "Not counted: proof times not recorded". Statements already backed by an earlier run are counted apart ("N more were already backed…") and are **not** in the median as 0 minutes. A statement with no parseable time on either side is `unmeasured` and left out, said in the tooltip. No waiting statement → "—" and why, never 0 |
| Needed a re-check after a later edit (and "re-proven") | `proof_moved`: total; `open` = moved and still not proven (exactly the headline's "need a re-check"); `reproven` = moved and now proven; `failed` = moved and contradicted. They add up | `proof_moved` not served → the card falls back to "Need a re-check now" with the number the headline gives |
| Claims made before any run | `has_verifier`, `run_before === false`, in a thread: of them, proven afterwards / a run afterwards failed / still nothing | `run_before` not served → "Not counted" |
| Checks you re-ran | nothing: a person's re-run is recorded per Change (`change_run_executions`) and no route reads it across Changes | always "Not counted", with the reason |

**Export CSV** is built here in the browser from the same rows (`recordCsv`) and handed to `Whole-Story/export-io.js` `IO.save` (Blob → download); nothing is posted. One header row, then one row per project and agent that made a statement in the selection (an empty record is not a row of zeros): `project, agent, range_days, counted_through, statements, proven, need_a_recheck, nothing_found, a_run_failed, recorded_settles_nothing, proof_moved_by_a_later_edit, reproven_after_a_later_edit, said_before_any_run, median_minutes_to_proof`. CRLF line endings, RFC 4180 quoting, a project name that looks like a folder path keeps its last segment only. An empty cell is a count that is not counted. File name `thruline-record-<project|all-projects>-<range>d[-claude-code|-codex].csv`.

When the statements cannot be read at all, the page draws "The statements could not be read just now." and a reload button, with every counting control off.
