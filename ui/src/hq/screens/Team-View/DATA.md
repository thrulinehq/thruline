# Team-View — data contract

`logic.js` reads content only from `const D = this.props.data || DEMO` (`demo.js`). `this.state` holds UI state only: the Plain/Technical face, the set-aside filter, whether a note was copied, and **local, uncommitted edits** (`seat`, `rule`) — overrides keyed by id, where an absent key means "as recorded on the base branch". With `props.data` present, `isDemo` is false and the canvas's "Roadmap … People and records here are illustrative" banner is not drawn.

Store column: **yes** = served today (route/table cited) · **no** = nothing serves it · **unknown** = not verified in this session.

**Headline: nothing on this screen is served by the store today.** Team features are roadmap (the canvas banner says so). Seats, groups, per-path approval rules and shared templates have no table and no route (second approvals do: see "Second approvals are live" below). Wave 2 needs a team model before this screen can be live. The exceptions are the storage figure, the inbox count, and the set-aside rows (via `change_obligation_waivers`).

| Field | Type | Meaning | Real source | Store has it? |
|---|---|---|---|---|
| `project.name` | string | the project the team view is for | project record | yes — `/api/hq/projects` |
| `project.baseBranch` | string | branch rules and decision files are read from ("main") | repository default branch / policy config | unknown — `thruline policy` resolves a base ref (`crates/cli/src/policy_cmd.rs`); no route serves it |
| `recording.projects` | int | how many projects are recording now ("Recording · 2 projects") | capture state per project | partly — `/api/hq/projects`; no per-project "recording" flag verified |
| `readAt` | 'HH:MM' | when this view was read | the server read time | no — the client can stamp it |
| `inbox.yourMove` | int | Changes whose move is yours (sidebar badge) | Inbox grouping | yes, via the Inbox's own contract (W1-A) |
| `plan.name`, `plan.seats` | string, int | licensed plan and seat count ("Team · 5 seats") | licence | no — `/api/licence` exists, but no seat field was found in `crates/licence` |
| `plan.licensee` | string | organisation licensed ("Acme Ltd") | licence | unknown — `/api/licence` |
| `storage.bytes` | int | bytes ThruLine keeps on this machine; drawn as `x.y GB` (1e9) | storage census | yes — `/api/storage` |
| `people[]` | `{id, name, isYou, email, key, seated}` | everyone the team knows: masked e-mail (Plain), signing key or `null` (Technical, drawn as "no key yet"), and whether they hold a seat on main | team roster + signing keys | **no** |
| `groups[]` | `{name, members: id[]}` | groups named in rules (`payments-owners`) | team policy | **no** |
| `rules[]` | `{id, paths \| null, group \| null, twoApprovals}` | per-path approval rules, most specific first; `paths: null` = everything else, `group: null` = anyone with a seat | `.thruline/policy.yml` on the base branch | **no** — the policy file exists for `thruline policy`, but no route serves rules in this shape |
| `waiting[]` | `{change:{id,title}, version, commit, cleanTree, decisionFile, rulesMatched: rule-id[], approvals:[{by, at}], next}` | decisions with one approval recorded that wait on a second; `next` = the person id who is waited on | `change_decisions` (one row per approval) + rule matching on changed paths | partly — `change_decisions` holds single approvals (T-S-4); second approvals, `rulesMatched` and `next` do not exist |
| `changedAfterApproval[]` | `{title}` | Changes edited after approval, across the team ("none") | version moved past the approved version | unknown — derivable from `change_decisions` plus the version history; no route |
| `templates[]` | `{name, version, lines, adoptedBy, adoptedOn}` | shared "What counts as done" templates adopted for the repo | template registry | **no** |
| `setAside[]` | `{kind: 'waived'\|'not_applicable', line, version, change, by, authority, at, why}` | lines set aside under team authority | `change_obligation_waivers` (rows 5 and 6 of CLAUDE.md's write-address table) | partly — the table exists; no cross-Change listing route; `authority` comes from the accepted profile |
| `activity.monthDeciders` | int | people who approved or set lines aside this month | `change_decisions` + `change_obligation_waivers`, distinct actors, calendar month | derivable — no route |

## Computed in `logic.js` (never stored)

- Seat use = people named in a rule's group, plus people with `seated`, under local edits. Segment bar = `plan.seats`.
- "N decision(s) waiting on you · M is waiting on X" = `waiting[]` grouped by `next`. "N lines set aside" = `setAside.length`.
- Filter counts = `setAside` by kind.
- "With your edit: N would wait" = `waiting[]` whose `rulesMatched` hits a rule that is on after local edits; "main today" = the same, under the recorded rules.
- Initials = the first letter of the name; "you" stands in for the person with `isYou`.

## Markup limits wave 2 must know

- The template draws exactly **one group row**, **two rule rows**, **two waiting rows** (`waiting[0]` drawn first as "b", `waiting[1]` second as "a"; only the second has the copy-note button) and **three template rows**. They are fixed slots, not loops. Live data with other counts needs loops added to the markup, which this wave was not allowed to do.
- "Anyone with a seat", "Everything else" and the Team plan description are kept as product language.
- Dates and times (`at`, `adoptedOn`) are display strings as the canvas shows them ("today 15:20", "12 Sep"). The real source is a timestamp that wave 2 formats.

## Wave 2 — what the live page reads (W2-E)

Live data is `teamFromStore(...)` (`from-store.js`) over four GETs: `/api/hq/projects`, `/api/changes`, `/api/storage` and `/api/licence`. `logic.js` draws it through `liveVals`.

| Live field | From | Notes |
|---|---|---|
| `decisions[]` | `/api/changes` → each ChangeView whose `project_id` is one of the HQ project's `members[].ledger_project_id` → `decisions.history[]` with `act: approved` | title = `request.title`; version = first 7 characters of `target_revision`; `withdrawn` = a later `withdrawn` row names it |
| `setAside[]` | the same rows' `exceptions[]` | `line` = `accepted_scope.criteria[n-1]` for `criterion:n`, otherwise "A line of what counts as done" |
| `plan` | `/api/licence` | By the ONE plan rule, `ui/src/hq/data/plan.js` (C30 P0-6), which Settings and Approve call too: **Pro** when `status: valid` AND the server's own `entitlements.approve_version.allowed` is true; otherwise **Free**; `null` ("Plan not read") when the answer is missing or malformed. (This row used to say `status: active`, a status the server never serves, so Team said Free for every licence.) The `trial` block is never read (CTO C21-b) |
| `storage.bytes` | `/api/storage` | |
| `inbox.yourMove` | the sum of every project's `whose_move.you` | |

These sections are drawn as their headings plus a plain reason (`LIVE_REASONS` in `logic.js`), and no control in them works:
- groups;
- approval rules;
- shared templates;
- seats.

**Second approvals are live (C30 R-LEDGER).** Each approved version (`decisions[]`) also carries `standing` (the approval that stands now), `full` (its whole version, sent as `seen_version`), `replaced` (a later approval took its place; the server's `superseded_by`) and `seconds[]`: `{id, who, at}` for each row of the served Change's `seconds[]` that is `act: approved` and `counting` (the server derives `counting`; `data/seconds.js` `secondsOf`). The row says "Seconded · <name> <time>", "No second approval yet", "Approval taken back, so no second counts" or "A later approval replaced this one" (`secondSlot` in `logic.js`); it never says a version is waiting, because no approval rule (`.thruline/policy.yml`, a later card) says how many approvals are needed. On the approval that stands, `seconds.jsx` offers "Second this version" and, where one stands, "Take a second back"; they post to the two addresses in `actions.js` (write-address rows 22 and 23, free) and show a refusal as the server served it.

The banner says "Team arrives in a later version." and links "See plans" to `https://thrulinehq.com/pricing`. "Paddle" appears only in the gallery. The live plan line says Stripe, and only for Pro with a licensee.
