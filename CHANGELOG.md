# Changelog

## [0.2.0] — unreleased

Built from the frozen main line at `688868ba` (code freeze `0992b108`, then three corrections: ⌘↵ inside the Approve sheet records no decision and Approve needs an explicit press, `a01cb3c4`; the Change Brief's Plain / Technical button keeps its visible name, `1a1741c7`; the Approve sheet, What counts as done and Every statement judge "bar set after the work" on recorded moments, not clock times, `9a33c88a`, `0b4d0b97`) plus the version commit and the release documents, on branch `hq-next/v02-release` (`b600eede` before this documentation commit). The release candidate is the commit that carries this text, and the founder confirms the launch commit. Full plain-words notes are in [the release notes](docs/release/v0.2.0-NOTES.md).

- **Licence: FSL-1.1-MIT.** The source is public under the Functional Source License, Version 1.1, MIT Future License (Copyright 2026 RAAS Labs, LLC); the terms are in `LICENSE.md`, and each version converts to the MIT license two years after it is first made available. This supersedes the 2026-10-06 "Licence: proprietary" entry below, which is kept as history. Approving a version still needs a Pro licence.
- HQ Next connects the Change review screens, grounded navigation and keyboard controls. (Commits `449defc9`, `983ad072`, `66c5ee6e`.)
- Decisions records accepted checks separately from proposals and gives Verify explicit evidence states. (Commits `524c8237`, `fdf93c5f`, `dc39e946`.)
- Project capture, pause, path exclusions and run permissions have dedicated controls; notes, objections and second approvals are kept as person acts. (Commits `ff9dc0c1`, `da07c704`, `8dff27cd`, `bda4c06b`, `1a6ac86d`, `81e94757`, `53152dd9`.)
- Whole Story exports Markdown, HTML, JSON and a redacted copy. HQ adds record search, Inbox hide, per-Change integrity reads, open-page notifications and a weekly summary. (Commits `8872c076`, `15975d04`, `95d63b1c`.)
- **Remove records is not in 0.2.** Settings shows the row disabled and says removing is not available in this version; 0.2.0 removes nothing. The work is parked for 0.2.1. The upgrade still creates the empty `project_removals` receipt table (registry order 170); nothing writes to it. (Commits `ab52a93e`, `5e9e17ea`.)
- **Upgrading from 0.1.0 needs `thruline upgrade`.** Rehearsed on 2026-10-12 on a clone of a 2.76 M-event record: 36 s, every existing table's rows unchanged except the upgrade's own schema stamp, 14 new empty tables, and `verify` afterwards gives the same per-project results. Projects whose original session files are gone now exit **2** (unverifiable, chain intact) where 0.1.0 exited 1; same counts, new wording, not damage and not intact.
- Fixed since the first draft (C54): Change Brief "Open in editor" no longer puts the home-folder path in the page (`db17b1cd`); `thruline ui` no longer stops when its reader stops reading output (`9e6a866a`); Settings 13 fixes (`f66eb620`); Inbox refreshes the tracked command when a Change's target changes (`37b9bd9a`); Record's three statement counts open a statements list (`52d1fc3d`); Record Capture names its fault and Try again says "Checking…" until the new read returns (`c3b73bdb`, `60991659`); the Decisions Stop line's read budget may be lengthened (never shortened) with `THRULINE_STOP_ADVISORY_BUDGET_MS`, capped at 2 s (`48426da2`); screen repairs for Repair Loop, What Changed, Change Brief, Command Ask, Inbox, Approve Sheet, Step View, Every Statement and the pull-request receipt check (`a07655e0`, `ebb33ed5`, `de36f51f`, `0c0c1b3f`, `d4fad7c5`, `39edc58b`, `b3b40219`, `b19c8139`, `264bf23c`).
- Fixed since the first draft (census fix waves, at the code freeze): on the Change Brief `.` and `y` press the buttons the panel shows (`79e70782`); a held `?` or `g` no longer undoes itself (`492bbf0e`); the Esc that closes Find, ask, act no longer also goes back (`3ae3ad76`); Inbox row links always name the row's thread (`34263d77`), a finished thread approved for its current version reads Approved (`af1430a1`), and Inbox quotes say HH:MM · step N (`642948ee`); Open in editor works on real Changes, where it was always disabled, and still keeps the home folder out of the page (`8d195b88`); Settings: hooks pointing at a missing program raise the system-state bar (`8289e37b`), "Installed by thruline init" shows only where it is installed (`4cadad55`), the Rules link names and opens a Change (`772749e8`), allowed-folder times say the day (`38f99900`), and the "HQ stopped" bar appears when HQ stops, with Settings checking every 3 s only while open and visible (`e558861e`, `823d6dcc`, `a37160c4`); the top-bar Recording pill follows the Change you are looking at (`e558861e`); a folder ThruLine has never recorded shows its folder name with "path not known until the first recording" instead of a guessed path, and no hooks are installed into an unknown path (`5e3f1b9e`).
- Fixed since the previous draft (midnight order): just after midnight, the Approve sheet, What counts as done and Every statement could misjudge whether the bar was set after the work, because they compared clock times; they now compare the recorded moments (`9a33c88a`, `0b4d0b97`).
- Known limits: Inbox takes about 3.4 s to open on a 2.76 M-event record; a later Capture fault can show an old "Tried again at …" line before Try again is pressed; the upgrade's free-space line is the volume's available capacity (72.5 GiB where `df` said 46 GiB). The release notes' "Census known issues" list names, by id, every census row (39) and every separate census check (31, which are not among the 249 rows) that did not pass in its newest measurement, and the sign-off sheet states the census result at each row's newest measurement (249 rows: 210 PASS, 31 FAIL, 4 BLOCKED, 1 HARNESS, 3 RULED-NOT-YET; the 136 reviewed rows: 133 PASS, 0 FAIL, 3 RULED-NOT-YET) and the runs, on `0992b108`, `61ed03cb`, `688868ba` and earlier commits, behind it.

**2026-10-09 — C48 lead qualification (not a release).**
Settings shows permission per recorded worktree folder, with guarded historical
revocation and masked labels. Shared ledger opens honour a removal hold; CLI/HQ
report the refusal, and held sensor deliveries spill without appending or sealing.
Observational verification leaves metadata unchanged and confines accepted
Codex payload checks to an explicitly supplied mirror. An Inbox request can reuse
an overlapping background index build. Migration 170 adds the append-only removal
receipt, but Remove records remains disabled at two unresolved safety gates.
The full engine merge gate remains red; focused results and clone qualification
are reported in `docs/hq-next/C48-LEAD-STATUS.md`.

**2026-10-08 — C44 Decisions qualification (not a release).**
Verify distinguishes recorded-prefix growth, missing originals, genuine
mismatches and unavailable evidence, with registered JSON and exit 0/1/2.
Complete parameter revisions can be proposed with exact parent/source pins
without recording acceptance. The list API exposes latest proposal pins
separately from an older accepted revision. Unavailable raw receipts retain
exact grouped counts and observed line bounds without per-event duplicate
messages. Claude's acceptance-byte digest needs an independently verified EOF.
Changed agent files retain separate, proved original-record and sealed-copy wording.
Exact AST test declarations and enumerated support references extend narrow
waivers within their existing bounds. A local archive-proof generator uses
an explicit external target; exclusions require matching committed package versions.
The gate harness preserves interpreter, Pillow and
umask mappings, internal runtime storage for the named authored-record suites,
and an explicitly pinned native Git for the speaker fixture. Compiler output
remains external. The macOS compiler census now uses bounded native process
metadata inside the capture sandbox. HQ's existing capture-hold verifier is
preserved through the qualified CLI adapter. The encrypted basis backup and 97-project diagnostic are
retained; nine prefix integrity findings remain unrepaired. Final gates,
actual ship proof, remaining release-script scopes and install qualification
are reported separately in ENGINE 100; no new person act is implied.

**2026-10-08 — Decisions gate fixes (C38).**
Mask display text before truncation, mask visual presentation boundaries, and
refuse credential-shaped review and rule values. Register the Decisions public
JSON envelopes and bounded waits. Make visual test folders private in fresh
checkouts and retain the HTTP child stdout pipe across server startup. These
changes require the unfiltered workspace gate before HQ re-integration.

**2026-10-08 — Decisions engine (v0.2 integration pending).**
Decisions: a thing you said once becomes a check ThruLine runs on every version.
The accepted span, closed check parameters, exact version and findings are
recorded separately from proposals. A person accepts, retires or waives a
decision; a recorded verdict can be replayed against the same tree.
C35 adds exact conflict/supersession and native Edit/Write revival receipts,
deferred decisions with recorded triggers, qualified person work intervals,
tracked-rule proposals and parameter notes, accepted rules in the MCP context
packet, bounded source readers, native review preparation, and typed
policy/source projections for HQ. Checks and standing
authority use source receipts and exact versions. Missing producer facts stay
Unverifiable; enforcement remains dormant until real person acceptance.

**2026-10-06 — Licence: proprietary.**
ThruLine is proprietary software. Copyright (c) 2026 RAAS Labs, LLC. All rights
reserved. The licence terms are in `LICENSE`. This supersedes the earlier
licence entries below, which are kept as history.

**2026-09-23 — P6-MERGE: `p6/ui-shell` merged (R-2026-09-22-6, R-2026-09-23-3).**
The parallel UI session's three cards land on `codex/l0-execution` as one
`--no-ff` merge whose parents are exactly `e10a200b` and `9facd060`: P6-6 (the
true-black visual system — `--ws-app: #000000`, colour by meaning, type scale,
focus, reduced motion), P6-1 (Brief / Explore / Pro with one top bar, sidebar
and keyboard map) and P6-3 (four lanes, the edge legend, the lit thread, the
Plain/Technical face, the scrubber and the Outline twin). One conflict,
`ui/package.json`'s test list, resolved as the union of both sides — no test
from either side was dropped. **Brief derives nothing:** it reads `hq.*` off
the one model `change-model.js` and `coverage-view.js` own, and
`change-agreement.test.js` now renders Brief as a fifth surface and asserts its
one state chip carries the model's own joined line, so a bare word cannot drift
back in. Every row of the branch's `INTERFACE-NEEDS.md` is filed as
`P6-IR-1`…`P6-IR-11` in the new `docs/loop/INTERFACE-REQUESTS.md`, with
successor cards for rows 4, 8 and 9. `ui/dist` is rebuilt and byte-identical to
a fresh build.

**2026-09-23 — T-3-6: the eight qualification cases run on the installed
candidate, and five defects filed rather than fixed.** `Q1`–`Q6`, `Q12` and
`Q13` were each run against `qualification/retry-delivery` on a binary built
from `c59fb05a`, under a fresh HOME per case, with their expected outcomes
copied from `docs/PHASE1-DECISIONS.md` §2.5 into
`qualification/retry-delivery/cases/EXPECTED.md`
(sha256 `029ed2896e1154e3c07d2a6cba59e8b60d6a44dd8c3d35acc56c7cf0560243f8`)
and hashed before the first case ran. Every ruling is made from the Change
screen's DOM text in headless Chrome; the captures are in
`docs/loop/reports/queue-2026-09-22/T-3-6/`. `Q3`, `Q5` and `Q12` came out as
predeclared. `Q1`, `Q2`, `Q4`, `Q6` and `Q13` departed, and are filed as
`docs/loop/tasks/T-3-6-F1` … `T-3-6-F5`. No product code was changed by this
task.

**2026-09-22 — candidate T-S-2 (sprint slice of T-3-3): "proven on the previous
version — needs a re-check."** When a source, test, dependency or
configuration path under an accepted criterion's scope changes after a linked
check run was produced, the criterion now reads *"Proven on the previous
version — needs a re-check"* with the reason class one level down; an
untouched criterion reads *"Still applies"*. Records are append-only history —
an old run keeps its own snapshot and time, never relabelled — and
`GET /api/changes/<id>/applicability` serves them, masked.

* **The store change requires `thruline upgrade`.** Adds one table,
  `change_criterion_freshness`, and alters nothing else.
* **One documented dependency rule.** `evidence_applicability::invalidating_paths`
  decides: a check profile's declared paths, when any exist, are the only
  paths that invalidate; otherwise every changed path under the accepted scope
  does (conservative). No check profile in this schema declares per-check
  paths yet, so every assessment today takes the conservative branch — a
  decision recorded in the T-S-2 coder report, not silently assumed.
* **Criteria and policy changes reassess, never inherit.** Accepting a new
  scope version whose criterion text changed at an index marks the earlier
  version's linked evidence at that index `criteria changed — needs
  reassessment`; an unchanged criterion's records are left alone.
* **Scoped to the sprint slice.** T-3-4/5/6 keep the rest of T-3-3: this
  candidate does not wire an automatic re-run, does not derive changed paths
  from a live git diff (the assessment function takes two whole-tree
  manifests as input), and does not yet surface the sentence on the Change
  screen or in the packet — `docs/loop/tasks/T-S-2.md`'s coder report names
  these as cuts against the sprint's 90-minute budget.

**2026-09-22 — candidate T-3-2b: a check run can reach the Change it was run
for, and a Change says which recorded folders it covers.** This candidate adds
the link from `thruline check --for-change` to the named Change, lets Prepare
derive the recorded files into a proposed allowed scope, and keeps scope
acceptance append-only. First-use shows the same proposed folders before the
person accepts what should count as done; it does not treat that proposal as an
approval.

* **Recorded workspace provenance chooses the root.** Captured transcript
  `cwd`, an already-recorded workspace identity, or a person's explicit
  `change new --root` selection can establish the workspace. A Claude Code
  directory key that merely resembles a path cannot. Conflicting captured roots
  stay unresolved, absolute paths without an established root never widen the
  allowed scope, and an already-bound sibling worktree keeps its own identity.
* **The store change requires `thruline upgrade`.** Existing stores must be
  upgraded before using the candidate behavior; the upgrade adds the Change
  link, allowed-scope, and examination-input records needed to retain the
  history rather than rewrite it.
* **A second checkout is refused for the reason it is actually refused for.** A
  check run from a clean sibling copy of the same commit is still not counted,
  and the sentence now says why: *"This is a second checkout of the same commit.
  ThruLine records one checkout per Change — pick the one you're working in."*
  It used to say the unsaved changes were different when neither copy had any.
  The words "unsaved changes" now appear only when one side genuinely has some.

**2026-09-19 — report provenance and qualification-source reconstruction (T-0-6 candidate).** A privacy-safe index maps 105 historical and takeover source files (102 distinct byte revisions) to locked deposited bytes, preserving FAIL and withdrawn READY dispositions. The generated-file inventory separates committed logical blobs from dirty and ignored material, and the qualification recipe reconstructs the preserved application into a disposable sandbox. This documentation does not qualify product cases or close the three blocked source tasks.

**2026-09-18 — every result now says which version it came from, and one from a
different running copy of the app counts for nothing (T-3-2).** Until this task
a green run from yesterday's server, or from a sibling copy of the project
folder, could sit under a thing that counts as done with nothing to say it was
about something else.

* **An examined-inputs record on every examination.** One table,
  `examination_inputs`, holding *(which examination, what it looked at)* by
  reference and never by copy: the workspace snapshot's own id, the workspace
  identity that already covers uncommitted changes, the starting revision, the
  lockfile digests the snapshot computed, the config and test files named on the
  command line, the tools that ran with their own `--version` answers, and — for
  a browser or API examination — the listening port, the process that owns it,
  that process's program and the database file it has open, read with `lsof` for
  that one port and nothing else. DDL change: `thruline upgrade` is required and
  every other verb exits 3 until it runs. Measured cost: 974 bytes per record.
* **Unknown is a state, never a value.** A probe that did not answer is stored
  as nothing at all, rendered as *"not written down"*, and **never compares
  equal to anything, including another unknown**. The database refuses the
  literal word `unknown` in every column that could carry one, so a caller that
  writes the word instead of the state is refused by the store rather than by a
  convention.
* **One predicate, and its callers are enumerated by a test.** An observation
  applies to a version only when the revision, the unsaved changes, the copy of
  the project folder and — where a running application was examined — the port
  owner and its database all match what the Change names. Every mismatch has a
  sentence: *"This was checked on a different version of the code."*, *"This was
  checked on the same commit, with different unsaved changes."*, *"This was
  checked in a different copy of your project folder."*, *"This came from a
  different running copy of the app."*, *"This came from a copy of the app using
  a different database."*
* **A mismatched result is shown and counts for nothing.** It is never deleted
  to tidy the screen, it does not move the count in the sentence at the top of
  the Change, and a line whose only evidence came from somewhere else reads
  *"Something ran for this, and not on this version."* — which is not a pass and
  not a failure.
* **Prepare says what will be looked at, in one screen.** The project folder,
  the starting point, whether your unsaved changes are included and how many,
  which tools will run and which version each is, which tools are not on this
  machine, and the app on the port this Change is about. The ids, digests and
  pids are one click deeper.
* **New:** `thruline change prepare` writes it down; `thruline check --app-port`
  records which running application a check examined; `GET
  /api/changes/<id>/inputs` serves it.

**2026-09-18 — each thing that counts as done now says what was looked at for
it, and where nothing was (T-3-1).** Until this task the Change screen could say
"two recordings are linked" and could not say whether anything in them bore on
line 3. It can now, and the rule that decides is one function a person can read.

* **One table of links, and no fifth basis.** `change_criterion_evidence` holds
  *(which accepted line, which already-stored piece of evidence, why)* and no
  outcome, no count and no prose copied out of the thing it points at. `why` is
  a closed four-word set — the accepted checks cover this kind of thing
  (`declared`), what ran names what the line is about (`named`), a person said
  so (`manual`), a person said it does not bear (`not-bearing`). **There is no
  timing basis**, in the code or in the database's own CHECK: a run that
  happened in the same session as the work is not evidence about a line, and
  that was the failure this task existed to prevent. DDL change: `thruline
  upgrade` is required and every other verb exits 3 until it runs.
* **A test command that exited 0 settles nothing.** This store records that a
  command ran and how it exited; it does not record how many tests it ran, so
  "exit 0" cannot tell a full suite from zero tests or from every test skipped.
  The run is shown under the line, labelled *"Ran, checked nothing."*, and the
  count in the sentence at the top of the Change does not move for it. A build
  or lint check is different and says so: its evidence is its exit code.
* **Four things a line can say, and none of them is a pass.** Something was
  looked at · *"No test ran for this."* · *"The check this needs isn't available
  here: <tool>."* · *"Something ran for this, and it checked nothing."*
* **Taking a link off never deletes it.** It appends a tombstone with the
  person's name on it, and the link stays on the screen under *"Taken off this
  line, and kept"*.
* **A disabled control names the act that would turn it on.** On a Change whose
  work is confirmed and whose accepted list names no files, "Confirm the work
  that belongs to this Change" was asking a person to do what they had already
  done; the reason is now the link layer's own, and each of its six answers has
  its own enabler.

**2026-09-18 — the number is the folder's, and the last disabled control got
its enabler (T-2-10).** The two findings of the Phase 2 exit check, fixed on
the surfaces they were found on.

* **The storage number is the folder's number.** The Inbox said "ThruLine is
  keeping 1.4 MB of records on this machine" and its one-click-down disclosure
  named the whole folder — while the count was the ledger and its two SQLite
  journals and nothing else. On the store the exit check measured, the folder
  held 3.0 MB: the missing half was mostly the work kept word for word, which
  is the exact growth the store-size sentence exists to warn about. The read now
  walks the folder it names, adds up every regular file (symbolic links are
  skipped, not followed), and serves a breakdown the technical layer shows as
  what it is made of — *"records — 897.0 KB · what was run — 180.0 KB · the work
  kept word for word — 32.3 KB"*. The plain sentence is unchanged in shape, and
  the rounding it does is now stated beside the exact byte count.
* **Every disabled control says what would enable it.** The Define stage's
  "Accept what counts as done" was off with a reason that ended "Nothing on this
  screen enables it" — why, twice, and nowhere to go, inside the ten-second read
  of the first screen a new person opens. The accepting act exists on this
  build, so the reason now names it, with this Change already in it: *"Accept
  them from a terminal, in this project: `thruline change accept <this Change>
  --all --actor "your name"`."* A test over every disabled control on the
  Change, whole-story, Inbox and first-use screens now holds the second half of
  the rule: each rendered reason must name an act from the copy file's own list.

**2026-09-18 — the five defects the real walk found, fixed (T-2-9).** Every one
of `T-2-8-F1`–`F5`, with the test each card asked for, re-run against a copy of
the store T-2-8 built from two real Claude Code sessions.

* **Timestamps as the vendors write them.** The link suggester read one spelling
  of UTC — the 20-byte form this store writes — and filed everything else as
  "we cannot tell when this ran". Claude Code stamps milliseconds, so *every*
  real recording was undated and a Change could never span sessions by timing.
  Fractional seconds and an explicit `+00:00` are now normalised at read
  (truncated to the whole second, nothing written back, no store grows for it);
  a non-zero offset still stays unplaceable rather than guessed at. On the real
  store the second session is now proposed, basis `timing`.
* **What was not considered is said.** With nothing to suggest, the surface
  printed "Nothing in the record looks like it belongs to this Change yet." and
  returned — while the same run's report named both recordings it could not
  place in time. It now always reaches them, and ends with the count in a
  sentence: *"2 pieces of recorded work could not be placed in time, so they
  were not considered."*
* **Whole sentences, or nothing.** The proposer split a request at the `.` in
  `src/lib.rs` and at every soft wrap, and offered the halves for one-click
  acceptance. A dot inside a token, inside an abbreviation or inside backticks
  is no longer a full stop, a line break inside a paragraph is no longer a
  sentence end, and a span that does not read as a whole sentence is never
  proposed as something that counts as done — it is kept as a note with the
  reason.
* **One address watcher.** Three screens each watched the address for the one
  pair its own card had walked, and the pair nobody had walked was broken: Back
  from a Change put `#/inbox` in the address bar and left the Change on the
  glass. One watcher over the routing table, installed once, now owns the
  choice of root for every screen.
* **No absence-of-problem sentence without its scope.** "Nothing in the linked
  work contradicts what was said" and "nothing is waiting on evidence" read as
  an all-clear four lines under the page's own "None has evidence yet". Both now
  say what they are about and what they are not: *"…— nothing here has been
  checked against what counts as done."* The rule is mechanical and the test
  drives the zero-evidence Change through both surfaces.

**2026-09-18 — the first real Change, on real work, on the installed candidate
(T-2-8). No product code changed.** The qualification application
`docs/PHASE1-DECISIONS.md` §2 decided now exists at `qualification/retry-delivery/`
(gitignored, never shipped, never a product): a small outbox service that accepts
a delivery, attempts it against an endpoint that can be told to fail, records
every attempt in SQLite and exposes a retry, with `cargo test` covering the
duplicate-retry criterion, the restart obligation, and a second, independent
reader that opens the store read-only so that a "sent" message and a stored row
stay two different witnesses. One labelled seeded defect lives on the branch
`seeded/duplicate-enqueue` and never on `main`. Two **real** Claude Code sessions
— not replays — implemented and then extended its retry flow under a temporary
`HOME` with the candidate's own hooks installed by `thruline init`, and that work
was walked through the product end to end: first-use screen, proposed Change,
accepted criteria, confirmed sessions, Inbox, Change workspace, packet page. The
two predeclared outcomes held: no screen said "ready for approval", and every
accepted criterion read **"No test ran for this."** Five product defects the walk
found are filed as `docs/loop/tasks/T-2-8-F1`–`F5` and deliberately not fixed
here — among them a proposer that cuts a request into fragments at the `.` in
`src/lib.rs`, and a link suggester that files every real Claude Code session as
undated because the vendor's timestamps carry milliseconds.

**2026-09-18 — the first five minutes: install, pick a project, read one true
thing about your own work (T-2-7).** The documented quickstart used to end
nowhere. `cargo install` → `thruline init` → `thruline ingest ~/.claude/projects`
never opened a browser and, on a machine with nothing wired yet, stopped at
"0 transcript file(s)" with no next step; the first true statement anybody
reached in the measured walkthrough was about the bundled demo data, after 81
seconds. The quickstart is now two commands — `cargo install`, then
**`thruline ui`** — and on a machine with no project picked yet it opens on a
first-use screen instead of an Inbox that can only say it is empty. The screen
lists the repositories this machine has **already recorded work for** (read out
of the session records Claude Code and Codex write, with the repository taken
from the recording's own `cwd`, never guessed from a folder name), with what is
visible about each one from typed fields: how many pieces of recorded work, and
when the most recent one was. Before it writes anything it says what it will
keep and where — *ThruLine will keep its records in one folder on this machine.
Nothing leaves it.*, with the folder one click down. Picking one writes the
project and the recordings already on disk for it, and **nothing else**: no
Change, no criteria, no decision. The next screen is the Change that work would
make, named in the words you used when you asked for it, with what ThruLine
suggests checking and which of those tools this machine does not have — *not
available here: semgrep* — said as a fact about the machine and never as a
finding about your work. One click writes the Change, with the work you saw
**confirmed** and everything else merely suggested, and lands on the Inbox,
where that Change's own reason is the first true statement you read. Accepting
what counts as done is a **second, separate click**, offered and never required;
a test asserts that writing the Change leaves the accepted-scope table empty. A
project with nothing recorded gets the sentence that says exactly that and the
one command that changes it, never a "setup required" with no way out.

**Coming back to the first screen (T-2-7, fix round 1).** Two defects found in
the shipped path, both of them this product telling a person something untrue
about their own machine. First: whether a project was **already picked** was
answered by comparing the repository path to a stored one built by reading every
`-` in the vendor's folder name as a `/` — so for `uploads-api`, `my-app`,
`web-service` and every other ordinary hyphenated repository name it answered
"no" seconds after the person said yes. It is now an equality between two copies
of one vendor string, and the row says which of two different things is true:
*Already picked*, or *Already picked, and 1 Change is being tracked here*.
Second: `#/start` — a bookmark, a typed address, a second tab, the back button —
re-opened "pick a project" on a machine that had already picked one, and each
walk through wrote the same recorded work down as another Change; three visits
put three identical items in one Inbox with nothing on screen to say why there
were three. `#/start` now resolves through the same landing the bare address
does, the one action for a project that is already tracked goes to the Inbox
rather than offering to use it again, and the server refuses to confirm one
piece of recorded work onto a second Change — it answers with the Change that
already holds it, and the screen says *This work is already written down*.

**2026-09-17 — a door in front of the Change screen, and a way to open a Change
(T-2-4, fix round 1).** The screen below shipped with nothing leading to it:
the landing held no link to `#/changes/<id>` and no word about a Change, and
nothing in the product could create one — a fixture for the screen's own
walkthrough needed a program calling the library directly. Both are now
product: **`thruline change new`** opens a Change (one row, a title, who asked,
and nothing else decided), and the landing carries a strip listing what is open,
each row a link, each with the same one sentence the Change's own top says. With
nothing open the strip says what would put something there and gives the exact
command. Three smaller things the walkthrough found are fixed on the screen
itself: a revision written down as zeros is said to name nothing instead of
rendering as `version 0000000`; the linked-work list now sits under Verify,
whose question it answers, instead of under all six stages; and the finding
cards no longer print the record's own ids as chips on the Plain surface, nor a
"Show proof" button with no viewer behind it. Trace's cards are unchanged, chip
for chip.

**2026-09-17 — the thin Change workspace, the loop's first user-facing surface
(T-2-4).** A Change opens at `#/changes/<id>` as one screen whose top answers
"what am I being asked to accept?" in ten seconds: one title, one sentence of
current conclusion in everyday English, one named version — "version a1b2c3d ·
2026-01-02", in the text rather than in a chip beside it. (This line first
shipped naming the version "version 0000000 · 2026-01-02", which was the
synthetic fixture's null revision printed as if it were a version; the entry
above is the fix, and this correction is recorded rather than made quietly.) Under it the six
stages of the brief (Define · Prepare · Build · Verify · Review · Approve) as a
strip of **views**: clicking one changes the view and writes nothing, and a
stage with nothing in it says what would put something there rather than
rendering blank. Verify and Review reuse the existing finding cards, scoped to
the sessions a person has *confirmed* belong to the Change — a suggestion
contributes no card and no node, and the screen says how many it left out. The
focus set is the Trace focus, over the same confirmed work, as a list; the
canvas stays one click down. Approve is present and off, with both halves of
the reason beside it. Nothing on this screen is green, no state word is used as
the Change-level answer, and every string comes from the copy file.

**Sigstore keyless verification is now built** (Track B's crate, vendored at
`crates/sigstore`, DECISIONS #130): the DSSE signature, certificate chain,
embedded SCT, Rekor inclusion proof and signer identity are each verified and
each reported as its own check.

Still gated off, named rather than implied: production Rekor upload is off by
default, TUF refresh is a separate explicit act with the trust root pinned as
data, and RFC 3161 timestamp authorities are unhandled. The local-key path is
untouched — every receipt already issued keeps verifying.

The dated entries below that describe keyless as unbuilt were true when written
and are left exactly as they are; history is not rewritten here to look aligned.

All notable changes to ThruLine. Milestones are from `docs/BUILD-SPEC.md` §17.

## 2026-09-17 — what counts as done: proposed from the record, accepted in one act (T-2-3)

**No DDL.** Nothing about the schema moves; the two tables T-2-1 built —
`change_scope_revisions` and `change_scope_acceptances` — are where an acceptance
lands, and proposals are derived at read rather than stored. A reading that
disagreed with the record would be a bug a stored copy would hide.

Four things exist that did not:

- **A proposer that writes no sentences.** From the Change's purpose, the
  original prompt it points at, and the opening request of every session someone
  has *confirmed* belongs to the Change, it cuts each sentence into `subject ·
  condition · observable outcome` and proposes the ones that carry all three.
  Every part is a contiguous span of the person's own words; the only character
  added is the full stop. "make retries reliable" is never a criterion — it is
  **kept as a note** with the missing part named, and so is a sentence the
  masker cut into, because a half-redacted sentence is nobody's words. Same
  store, same proposals, in the same order.
- **A check profile suggested from the kind of work**, read off the paths the
  confirmed sessions changed: documentation, interface behaviour, backend
  behaviour, authorization-sensitive, data/schema. Each check names a registered
  adapter, and a mandatory check whose tool is not installed here is **still
  listed**, marked "not available here: <tool>" — a dropped check reads exactly
  like a passed one. Installed-but-with-no-reviewed-settings-file is a third
  answer, not a "ready": on this machine the measured count of adapters that
  could actually run is zero, and the product now says so instead of implying
  otherwise. A profile is not evidence, and every reading of it says that.
- **One acceptance act.** `thruline change accept <change> [--all | --criterion
  <n>…] --actor <name> [--rationale <text>] [--edit <n>=<text>]`, and
  `POST /api/changes/<id>/scope/accept`. It appends one scope version and one
  acceptance record with actor, time, version and rationale, carries forward
  everything the previous version said about scope, exclusions and constraints,
  and **is not an approval** — no decision row exists for it to touch, and it
  says so on the same screen. Five refusals, each a sentence that names what
  would fix it: no name; nothing selected; a line nobody proposed; a rewrite of
  a line nobody selected; and the bound of 20 versions per Change, stated on
  every reading before it bites. An empty list is refused: an empty accepted
  scope is not a scope.
- **Never silently weaken.** A second reading of an accepted Change is shown
  *beside* the accepted version, with what it would drop and what says less
  named by two mechanical tests — a dropped negation, and a word set that is a
  proper subset. Reading changes nothing; only accepting does. A line a person
  rewrote before accepting is attributed to the person, and the proposal it
  replaced is still derivable from the record.

`GET /api/changes/<id>/proposals` is read-only and masked. Every string either
verb or route can serve is in the copy file in both registers, and the Plain
register uses "what counts as done", "proposed", "accepted" and "still needs
evidence" and contains no engine noun — checked, not asserted.

## 2026-09-17 — session ↔ Change links: a suggestion, an act, and a refusal (T-2-2)

**This task changes DDL; `thruline upgrade` is required; every other verb exits
3 until it runs.** One table is amended and nothing else: T-2-1's
`change_session_associations` gains `rejected` as a third provenance and a
`reason` column for the words a person gives. The table has never shipped — the
tree this was built on has no Change tables at all — so there is no data to
migrate; a store carrying the earlier shape is **refused by name** on open
rather than opened and silently unable to record a rejection.

Three things exist that did not:

- **A suggester** that proposes sessions for a Change from two mechanical
  bases — the Change's activity window overlapping the session's capture
  window, and the session's changed files falling inside the accepted scope's
  allowed paths. It carries the evidence for each proposal, reports ties,
  undated sessions and scope lines it cannot read rather than resolving them,
  and produces the same list twice from the same store. It can write nothing
  but an unattributed `suggested` row.
- **An explicit act.** `thruline change associate <change> <session>
  --confirm|--reject --actor <name> --reason <text>`, and
  `POST /api/changes/<id>/associations`. Without a name, nothing is written and
  the refusal says what would fix it. Deciding appends beside the suggestion it
  answers and never edits it.
- **The scope refusal (`B3.13`).** One function decides whether a session's
  recorded outcomes may be read under a Change, and it requires both a
  confirmed link *and* changed paths inside the accepted scope. A test
  re-derives every call site of it from the workspace sources and fails naming
  any new one. A session confirmed on two Changes counts on the one whose scope
  its work is inside, and the other says why not, in Sam's words.

`thruline sessions` is untouched — byte-identical output, and its source is
pinned by digest, because unassociated historical work staying discoverable
(`B3.16`) is the quiet half of this. No association act writes a criterion or
an acceptance; a test compares both scope tables before and after every act.

## 2026-09-17 — the Change record, as a versioned migration (T-2-1)

**This task changes DDL; `thruline upgrade` is required; every other verb exits
3 until it runs.** The upgrade is idempotent, it adds five tables and alters no
existing one, and it invents nothing: no historical session gets a Change and no
acceptance criteria are fabricated for old work.

A **Change** — one bounded piece of work someone is being asked to accept — is
now a record in the core ledger, as a child of the project and a sibling of the
session. It carries the request (title, purpose, the original prompt by event
id, requester, source), the accepted scope as a **versioned** record with an
acceptance record that is deliberately not an approval, and an implementation
binding that names **one** repository, base revision and snapshot per binding
version. Re-scoping and re-binding append; the superseded version stays
readable, because an approval is attached to the version it was given.

Session ↔ Change associations carry explicit provenance. A row may only be
`confirmed` when its basis is `user` and it names an actor — timing, filename
overlap and an agent's own declaration can propose and can never confirm — and
the rule is a database CHECK, not a caller's good manners. No association
carries a result: there is no column for one, so a session's successful run
cannot arrive on a Change by inheritance.

Two read-only routes, `GET /api/changes` and `GET /api/changes/<id>`, serve the
records with the human words for them from `crates/core/src/voice.rs`. Every
free-text field is masked before it reaches the socket. There is no write route
and no user-facing surface yet.

Measured, not estimated: **2,216 bytes per Change** (with request prose at the
store's own mean of 914 B) and **410 bytes per association**, on a fixture store
with `PRAGMA page_count × page_size`. `crates/core/tests/change_cost.rs`
re-measures on every run and fails if the cost moves.

`thruline export` gains `--generated-at <TIMESTAMP>`, which stamps the export
with the time you name instead of the moment it ran. It reaches exactly one
field — `generated_at`, the only thing in the export that is not derived from
the record — and the default is unchanged: with no flag, the export still says
when it was taken. It exists because the migration's byte-identity guard
compares two whole exports of the same store, and two runs that straddle a
second differed in that one field and in nothing else; the guard now pins the
stamp and still compares it, rather than excluding it.

## HQ Verdict Panel, checking sequence and Trace focus mode (unreleased)

The HQ right-hand panel is replaced by a Verdict panel: two tabs (Verdict,
Trace), a state word from the five in the register, a headline with counts,
ranked finding cards (Said / Record / Do, receipt chips, Copy prompt), the
things that hold up, a "you clicked" card, four questions answered from the
record, and a two-button footer. The free-text Ask box and the "Record-based
guide" footer are deleted. Plan and audit: `docs/HQ-VERDICT-PANEL-PLAN.md`.

### Step 1 — one copy file
`ui/src/workspace/hq-copy.js` holds every string the Verdict tab renders,
rendered through the deterministic engine in `hq-narrate.js` with a grounding
post-check (refs must exist, numbers only from refs, the 13 banned tokens with
inflections, no paths in the Plain register, unreferenced sentences styled as
inference). Golden test pins all 380 strings; `docs/HQ-COPY.md` is the prose
export for review. No model is involved anywhere on this path.

### Step 2 and 4 — the Verdict tab
`verdict-model.js` is pure: session state (any Not proven → Not proven; else
Loose thread; else Couldn't check; else Proven; Fog when capture is degraded;
an empty session reads Couldn't check), finding-kind labels only where the
engine stores a rule class (Said done no proof · Record says otherwise ·
Command failed then reported done · Tests weakened · Left behind · Couldn't
check · Recording gap), ranking by tier then blast radius then recency, Do
lines and paste-into-your-agent prompts from the copy families. Labels the
brief names but nothing stores are dropped, never faked (`docs/INTERFACE-REQUESTS.md`).

### Step 3 — the checking sequence, an honest instrument
`thruline claims --json --phases` reports six real pipeline phases to stderr
with real counts and measured times; stdout is byte-identical with or without
the flag (contract test). `GET /api/phases` serves the log; the panel plays
exactly those rows, scaled to ≤ 600 ms when cached in the browser, and lands
the verdict with one short fade and a single pulse. Rows are never invented;
because the engine has no streaming transport, the sequence replays the
completed log rather than ticking live (interface request 1). A reader who
leaves the Verdict tab mid-check and returns re-attaches to the same request;
the server's run is never paid twice in one browser tab (interface request 10
covers cancelling it server-side). When the log cannot be loaded, or arrives
with no countable row, the verdict still lands and the reason is said once in
the "Checked:" line's place — no counts, no replay link, nothing cached — with
or without reduced motion.

### Step 5 — Trace focus mode
Focus is the default Trace view: nodes reachable within three derivation hops
(`ref`/`inf` wires) of anything not Proven are shown; everything else merges
into counted group cards that expand in place; a Focus / Everything toggle
restores today's graph unchanged. "Show proof →" enters Focus with the chain
outlined. The pure rule lives in `focus-set.js`.

### Known gaps
The p3 `narrate` crate is still unmerged, so the copy engine lives in the UI;
`proof_needed`, claim-bound integration findings, constraint asks and a
tree-moved field are interface requests; the stranger test (`docs/HQ-STRANGER-TEST.md`)
is prepared, not run.

## M6 — the gate, the demo, the docs, and the packaging (unreleased, partial)

§9's policy engine and its App, §14's sync client, §15's first run, the mdBook,
and every install path. **The App authenticates a delivery and then stops** —
its two fetches and its token minting are not implemented — **and no sync
transport exists**, because §18's answer is that no server ships in v1. §18's
cloud-provider decision is still open, so that backend has no success path.
Every one of those is named in Known gaps rather than approximated.

### The policy engine reads a closed schema, not YAML

`.thruline/policy.yml` decides whether a merge is blocked, so the parser's
failure mode matters more than its coverage. `crates/policy` reads a documented
subset — two-space indentation, `key: value`, nested maps, inline flow lists —
and **refuses everything else by name and line number**: anchors, aliases, merge
keys, block sequences, block scalars, multi-document streams, quoted keys, tabs,
and YAML's implicit booleans (`yes`/`no`/`on`/`off`).

The closure is the security property. An unrecognised key is an error, not a
key that gets ignored:

```yaml
version: 1
require_reciept: true    # under a permissive reader, your gate is now off
```

Under this one it fails to load at line 2. A policy written for a newer
ThruLine fails loudly on an older one instead of quietly enforcing less than its
author wrote.

**No policy key can reference the extractor, a score, a confidence or a
threshold**, and a test walks the schema to keep it that way — BUILD-PLAN C.8.3,
backed by this repository's own measurement that 27 of 30 model-sourced emitted
lines were false accusations. Model-extracted claims are filtered out of
evaluation and **counted in the output**, so a reader can see that an advisory
`unsupported` existed rather than wonder why it never appeared.

### Three conclusions, and the third one is where the work is

§6's `unverifiable` survives to the merge button as **neutral**: nothing
blocked, and the check says what would settle it. Every undetermined input is
neutral rather than a pass — an unmeasured coverage under a `min_changed_line_coverage`
rule, an unparsed skip count under `allow_skips: false`, a human acknowledgement
a local run cannot see. And when the changed-path list itself cannot be
determined, `required_when` rules **still apply**: a gate that switches itself
off when it cannot see the diff switches itself off exactly when something has
gone wrong.

The tempting version of that code fills the gaps with `Vec::new()` and `0`,
which reads as "no changed paths, no skipped tests" and is a silent pass on the
runs that most need attention. Every such field is `Option` and the policy
engine turns `None` into a neutral.

### Added

- **`crates/policy`** — the schema, the parser, the evaluator. Deterministic and
  total: no clock, no environment, no I/O, so a check run is reproducible from
  the receipt and the policy.
- **`thruline policy init|check`** — `--init` writes §9's documented example and
  refuses to overwrite; `check` evaluates the last session. Exit **3** on
  failure, **0** on pass or neutral, **1** when the gate could not decide. A
  broken policy is an error, never a fallback to "gate nothing".
- **`--md` and `--conclusion`** — the check-run table and the one-word
  conclusion, rendered in Rust so the Action parses nothing in shell.
- **`thruline demo`** — §15's first run, and **nothing about it is canned**. It
  builds a real git repository, makes the edits a cheating agent makes, really
  runs the narrowed check through the shim, and prints the verdict §6's
  verifiers computed from those events seconds earlier. Needs a real test runner
  and **says so and stops** when it finds none: a simulated test run is the exact
  thing this product exists to catch.
- **`docs/book`** — the mdBook: quickstart per harness, the gate in CI, policy
  reference, verifier reference, receipts, threat model, benchmark methodology,
  privacy.
- **`packaging/`** — `install.sh`, a Homebrew formula, an `npx` shim, and an
  install step for the Action. Every one verifies a checksum **before** the
  binary is put anywhere, and none of the automated paths resolves a floating
  version.
- **`.github/workflows/release.yml`** and **`deny.toml`** — signed releases via
  a third-party signing action and GitHub's build-provenance attestation, an
  SBOM, and `cargo deny`. Tag-triggered only; this repository has never pushed a
  tag. **This is release-artifact signing in CI and is not the same thing as
  `crates/attest`, where Sigstore keyless verification remains not built** — a
  guard in that crate exists precisely so those two do not get conflated in a
  document, and it caught this entry's first wording.
- **`crates/reexec/src/cloud.rs`** behind `reexec-cloud` — §7's cloud sandbox
  interface **with no success path**, because §18 leaves the provider to the
  founder. A stub that returned a fabricated `ExecResult` would manufacture the
  evidence re-execution exists to demand.

### Changed

- **`thruline demo` now runs §15's scenario by default.** The context/handoff
  walk that used to be the default is `--continuity`; `--recovery` is unchanged.
  Two different demos wanted one name and the spec's took it — recorded here
  rather than done quietly.
- **`thruline claims` and `thruline policy check` share one extraction path.**
  Two would be two answers, and the first time they disagreed nobody would know
  which one the gate had used.
- **The Action gained an install step and an optional policy step.** Its
  no-derive rule is unchanged and its census still passes: the policy step
  consumes the word `--conclusion` printed and parses nothing.

- **`crates/app-core`** — §9's App, minus the sockets: webhook HMAC
  authentication (constant-time, and **the body is not parsed until it
  passes** — the parsed form is only reachable through the authenticating
  constructor), validation of every payload field that could become a path or an
  argument, the three receipt sources ordered strongest-first with their caveats,
  the conclusion combination, the check-run body, and the badge. Names **no HTTP
  or TLS crate**, so its decisions cannot come from anywhere but their arguments;
  `tests/no_network.rs` keeps that true in three ways.
- **`services/github-app`** — the socket shell, **excluded from the root
  workspace** so its dependencies never reach an engine build, with its own CI
  job so the exclusion does not become rot. It refuses to start without a
  webhook secret.

- **`crates/sync`** — §14's client half as a reference implementation v1 does
  not ship (§18). XChaCha20-Poly1305 with a fresh OS-RNG nonce per seal and the
  object id as associated data, under an Argon2id key whose parameters ride in
  the envelope so a second machine re-derives. The transport is a trait; the
  only implementation here is in-memory, for tests. `thruline sync` is behind a
  default-off feature where it **exists and refuses**, because a verb §14 lists
  that is simply absent reads as an oversight.
- **`crates/cli/tests/cli_surface_budget.rs`** — clap's derive builds every
  inline `Command` variant in one generated function, and that frame overflows a
  test thread. It has happened twice, both times aborting an unrelated test with
  no hint of the cause. 51 inline variants overflowed and 50 works, so 50 is the
  budget; `ImportLegacyArgs` is flattened to get back under it.

### Known gaps

- **The App authenticates and then stops.** Fetching the receipt and
  `.thruline/policy.yml` from the repository is not wired, and installation-token
  minting from an App private key is not implemented. It refuses and says so
  rather than posting a check run built from placeholders — such a check run
  would report a neutral conclusion on every pull request, which reads as
  "ThruLine looked and could not tell" when nothing looked. The service has no
  outbound HTTP client at all, because it makes no outbound call; the client
  returns in the same commit as the fetch that needs it.
- **No sync transport exists.** The client half is built and tested; §18's
  answer is that no server ships in v1, so the only `Transport` implementation
  here is the in-memory one the tests drive. `thruline sync push` has never moved
  a byte anywhere.
- **M6's done-when is unproven.** "A new user on a fresh machine gets from
  install to a PR check in under ten minutes following only the docs" needs a
  fresh machine and a new user. What is tested is the failure mode that would
  make it impossible: three censuses assert that every command the docs name
  exists, that the policy reference and the schema agree in **both** directions,
  and that every finding the verifier reference names is one a verifier emits.
- **`cargo install thruline` does not work.** The binary crate is
  `thruline-cli`; §15's line assumes a package named `thruline` on crates.io and
  nothing is published.
- **`thruline demo` needs a runner on `PATH`** (pytest today). §15 wants value
  visible before the user runs anything, and on a machine with no runner the
  demo refuses instead.
- **`policy check` cannot see a skip count.** §6's parsed run summary is not
  carried on the verdict record, so `allow_skips: false` yields a neutral rather
  than a pass or a failure. Named rather than defaulted to zero.
- **`cargo deny` and the SBOM have never run.** They are configured and
  tag-gated; no tag exists.

## Fixed — the classification queue could wait forever (unreleased)

`crates/core/src/annotate.rs` paused the language classification queue under
memory pressure with `while Paused { wait; resample }` — **no deadline, no
iteration bound, no escape.** If available memory never rose above
`resume_above_available_mb` it never returned. Measured: a full workspace test
run blocked there for **13.5 minutes on 7.7 seconds of CPU** while a second
process built the same workspace, which is an ordinary CI condition. In CI it
presents as a hung runner rather than a failed one.

The shape is the sentinel family one step further along: not a fallback standing
in for a fact, but **a fallback that never resolves.** The pause was right;
the absence of an end was the defect.

Fixed the way this project already fixes waits — the Stop gate's 5 s watchdog,
the hooks' 3.5 s self-timeout, `reexec/docker.rs`'s container deadline:

- **`language.memory_pause_timeout_secs`** (default 120 s), validated non-zero
  because a zero deadline is the same unbounded wait wearing a bound's name.
- On expiry the candidate becomes **`Uncertain` with a stated reason** — not a
  new state: `LanguageConfig` already documents that annotation continues when
  the language tier is unavailable, and `parse_stored_event` already accepts
  `attempts: 0` with `Uncertain` and a reason. It cannot become a guessed
  category, and §0.1 is untouched.
- **`classification_memory_timeouts`** in the report, with a `tracing::error!`
  naming what to change. A degraded run nobody can see is the failure this
  project refuses — and the unbounded wait was invisible the same way.

Validated both directions: bounded, **the test** finishes in 1.17 s (its file's
29 tests take 3.34 s); with the deadline check replaced by `if false`, the same
test was still running at 45 s, and the verifier recorded it never returning in
240 s. The earlier wording said "the suite finishes in 1.17 s" — that is one
test, and it broke the figure-names-its-command rule inside the entry recording
that rule.

**The fix would otherwise have created a flake.**
`language_tier_acceptance.rs` used the production entry point, which installs the
*system* memory probe, so its result depended on ambient free RAM. Unbounded, that
was a hang; bounded, it would have been red on a busy machine and green on an idle
one. It now injects an always-ready probe, as `language_pipeline.rs` already did.
**No test in the workspace depends on ambient free memory any more.**

Swept the rest of the workspace by hand and got one wrong. `cli/src/main.rs`'s
self-test fixture loops `for _ in 0..2 { listener.accept()? }` on a blocking
listener, joined synchronously by its caller — so a child that opened fewer than
two connections hung `thruline` with no diagnosis. I classified it as "the accept
loops are servers"; it is a two-request fixture. It now polls a non-blocking
listener against `SELF_TEST_ACCEPT_TIMEOUT` and fails with a named reason.

**`crates/cli/tests/blocking_wait_census.rs`** is the option I argued against: an
allowlist census, where the walk *detects* and a human *judges* once in writing.
It found two more bounded sites the hand sweep never listed, its own positive
control caught its walk truncating each file at the first `#[cfg(test)]`, and its
first version did not bind — a second sleep in a listed file was invisible until
the key carried a count, and a named-but-unchecked deadline passed until the
test asserted the comparison.

## RULE 7: no codex hook adapter, and a glibc mismatch (unreleased, $0 spent)

Found by validating in real containers before any spend.

- **RULE 7 CONFLICT.** The operator authorised **Codex CLI only**, and
  `thruline hook-install --harness codex` answers *"only the claude-code harness
  is implemented; codex and gemini are M4"*. A codex pilot would report
  `hook_events 0` for every task, and that zero would describe **ThruLine's
  missing adapter**, not the plumbing — the exact failure the Terminus refusal
  exists to prevent, through a different door. `Harness::Codex.provides_hook_events()`
  returned `true` until the container disproved it. Three options are set out in
  `REPORT.md`; none is chosen here. The shim half **does** work: `shim install`
  wrote 11 wrappers in-container including `pytest`, the runner all 241 tasks
  use, so codex can still measure (b) and (c).
- **glibc: the builder was newer than its target.** `rust:1.97` tracks the
  newest Debian (glibc **2.41**); the oldest target is **2.36**; glibc is
  backward compatible, not forward, so the binary died with `libc.so.6: version
  'GLIBC_2.39' not found`. Fixed by building on `rust:1.97-bookworm` — one
  binary that runs on both 2.36 and 2.39. **Not** the Rule 7 conflict that was
  flagged as possible; a one-word fix found by building rather than reasoning.
- **The builder must not pull ONNX.** `thruline-cli` has `default = ["ort"]`,
  which downloads ONNX Runtime from a CDN at build time.
  `--no-default-features` removes a network dependency from 241 image builds for
  a model none of them runs.
- **Verified live**: `thruline 0.1.0` runs in the task container; 10 hooks
  installed for claude-code; git 2.39.5 installed by the layer; tree hash
  recorded, 40 chars, clean. `tb run --agent oracle` reproduced at exit 0, 1
  resolved, 100%, zero model calls.
- **Container-side capture is unbuilt, not infeasible**: the task compose file
  already bind-mounts two host paths, so the spool is a third entry — on its own
  mount, never sharing a directory with the harness's own account.

## Multi-stage build, and the precondition amended (unreleased, nothing run)

- **The linux binary comes from a multi-stage build**, not cross-compilation —
  nothing outside Docker to provision. Builder is `rust:1.97` matching
  `rust-toolchain.toml` (a different compiler is a different binary) and
  **Debian-based, not Alpine**: every task base image is glibc, and a musl
  binary would fail as a container that starts and does nothing.
- **Amendment to the precondition: `git init` is not enough.** Measured in a
  real container — `git init` alone leaves `is-inside-work-tree` **true** while
  `HEAD^{tree}` exits **128**. A naive check passes; there is no tree; every
  bound check would be `unverifiable`, which is what the precondition exists to
  prevent. The requirement is `init` **+ an initial commit** (`--allow-empty`,
  so an empty workspace does not take the precondition with it). Verified: git
  2.39.5 installed by the layer, tree `6709938c…` recorded, 40 characters,
  status clean.
- **`tb` is installed from the repository's own pinned archive**, not PyPI, so
  the harness version and the task set come from the same commit. That belongs
  in the protocol's `parameters`.
- **`REPORT.md` gains "A family: guards that searched a space adjacent to the
  one they meant"** — five instances, the fifth committed inside the test
  written to guard the fourth.

## The harness driver and the capture route (unreleased, nothing run)

The pilot could not have measured anything: `~/.thruline/exec.db` absent, `tb
run` invoked 0 times in `crates/`, no `docker cp`/`exec` anywhere, `PilotTask` a
struct nothing filled. A paid run would have reported 0% coverage against the
90% gate. All verified independently before building.

- **`crates/bench/src/driver.rs`** — step 6, the missing one. `TbRun` builds the
  invocation; `TaskEvidence` fills `PilotTask`.
- **§0.2's ruling as a type.** `TaskEvidence` holds `container:
  Option<ContainerCapture>` and `corroboration: Vec<HarnessArtifact>`, and **no
  quantity of the second produces any of the first**. Terminal-Bench's
  `agent-logs`, `commands.txt`, `panes` and `sessions` are post-hoc harness
  output — a hint, not the execution record — so they are recorded with their
  provenance and can never be the sole basis for a bound check. Letting them
  count turns two tests red.
- **`CaptureLayer`** generates the per-image Dockerfile fragment: install `git`
  (conditionally), copy `thruline`, install hooks and the `PATH` shim **inside**
  the container with the spool on a mounted volume, then `git init` **and an
  initial commit** so a tree hash exists before the agent's first action.
  Ordering asserted.
- **`git` is ABSENT from both dominant base images** — `python-3-13:20250620`
  and `ubuntu-24-04:20250624`, 185 of 241 tasks — with `apt-get` present in
  both. Measured directly. The precondition needs an install step before `git
  init` is possible.
- **The `oracle` agent validates the chain at $0** — no model calls.
  `TbRun::oracle` cannot name a model and `costs_money()` is false.

### Known gaps

- **`tb` is not installed here**, so the oracle validation is built and not yet
  run.
- **`COPY thruline` needs a binary for the container's platform**;
  cross-compilation from macOS is not arranged.

## The pilot's budget guard, model pin and cost measurement (unreleased, nothing run)

The operator authorised ~$8 of their own credit, Codex CLI only, 10 tune tasks,
cheapest capable model, report and stop. Built for approval; **nothing run**.

- **`gpt-5.6-luna` pinned** at $0.20 in / $1.20 out per 1M tokens — verified
  against live pricing and Codex CLI's own supported list on 2026-09-03, not
  guessed. Cheapest model the CLI documents; `gpt-5-nano` is cheaper but its
  compatibility with Codex CLI is unconfirmed and the authorisation is too small
  to spend finding out. The adapter requires the model and passes `--model`, so
  nothing is defaulted.
- **Budget guard in integer micro-dollars** — no float touches a spend — with
  cost rounded **up**. Per-task cap $0.50 checked *before* the charge lands, run
  total cap $3.00 checked between tasks, both below the $8 authorisation with
  room for one re-pilot. A halt reports what was spent and how far it got.
- **Usage is retrievable, and the caveat is handled.** `turn.completed.usage`
  omits reasoning tokens, which bill as output, so a figure from it is a floor;
  `UsageSource` is carried per task and `any_may_understate` travels on the
  ledger. The stock adapter does not pass `--json` anyway, so usage comes from
  the session rollout file, which is where reasoning tokens are recorded.
- **Auth is `OPENAI_API_KEY`, not `codex login`** — and the setup script writes
  the key to `$HOME/.codex/auth.json` **inside the container**, so a
  credential-bearing artifact exists by design. The scan covers that path.

## The pilot path (unreleased, nothing run)

The operator exports a key; agents pilot ~20 tasks per corpus to prove the
pipeline and measure per-task cost; the full run is approved with a real figure.
**Built, and nothing has been run.**

- **`crates/bench/src/pilot.rs`** — the pilot is its own protocol with its own
  commitment and ledger entry, so the real protocol stays uncommitted and the
  holdout unspent. Pilot ids must be a subset of the **tune split**, checked as a
  refusal on the protocol however it was built; a re-pilot must use **new** tune
  tasks. The sample is derived from the base protocol's hash, so nobody chooses
  it.
- **Terminus is refused as a pilot harness.** Hook events only exist under the
  real harnesses, and a Terminus pilot would exercise the shim while saying
  nothing about the hook adapter — producing a hook-less corpus whose coverage
  number reads as a property of the corpus. Terminal-Bench at the pinned commit
  ships `claude_code` and `codex` adapters invoking the real CLIs, so the
  refusal has somewhere to go and this is **not** a Rule 7 conflict.
- **Three coverage measures, reported separately and gated at 90%**: hook
  events, shim exec events, and checks bound to a git tree. The gate names which
  failed. If only the third fails, the `git init` precondition is not holding in
  a live container — the failure the 0-of-241 measurement predicts, found for
  twenty tasks instead of 241.
- **Cost measured, never estimated.** Per-task means are over tasks that
  reported a cost; `cost_coverage` travels with the projection; `projected_for`
  returns `None` from zero samples.
- **The credential never enters anything.** `require_credential` checks presence
  and returns `()`; an absent variable fails by name and never falls back to an
  unauthenticated path. `scan_artifacts` uses §4.6's own predicate — the
  `claude_code` adapter really does inject `ANTHROPIC_API_KEY` into the container.

Incidental finding from reading the adapters: **codex is invoked with
`--skip-git-repo-check`** — Terminal-Bench itself expects these workspaces not
to be git repositories, corroborating the 0-of-241 measurement from the
harness's own side.

## M5 (revised) — Terminal-Bench and ImpossibleBench under class 2 (unreleased)

The operator ruled the Rule 7 conflict: **Terminal-Bench under class 2,
pre-registered** — not a tool-call verifier, because a verdict from tool calls
without an execution record is closer to reading the transcript than reading the
record. Then an addendum split the measurement in two and added the precondition
that saves the run.

### Two corpora, two numbers, never pooled

`run-tests.sh` answers *did this task pass*, so Terminal-Bench measures
**false-success detection** and cannot label a cheat. ImpossibleBench's tasks are
impossible by construction, so a pass **is** a cheat and the label is the task's
own — **cheat detection**. Two detectors over two corpora; the `Corpus` type
carries `measures()` so a headline cannot be written without saying which, and
tests assert neither oracle can produce the other's label.

This is also the first correct use of ImpossibleBench here: it could not be class
1 because it publishes no trajectories, but as a task source under a committed
protocol its impossibility is the label.

### The precondition, measured rather than assumed

§6.1 binds every check to a git tree hash. Over the pinned Terminal-Bench
archive: **241 task directories, 241 with an oracle, 241 `parser_name: pytest`,
and 0 carrying a `.git` tree.** Without `git init` in the workspace before the
agent starts, all 241 tasks would have produced `unverifiable` — a flawless,
fully pre-registered run whose every result was uncountable, discovered only
after the holdout was spent.

So it lives in the commitment: `Preconditions` is hashed, `commit()` refuses a
protocol that does not require it, and the ingest checks the tree itself rather
than believing the harness's terminal state.

### Added

- **`crates/bench/src/class_two.rs`** — the class-2 ingest for both corpora. The
  loop is over the **commitment's** task list, so a task that OOMed, timed out or
  never started becomes a published trajectory with a terminal state and the
  outcome count equals the declared task count whatever the run did. Undeclared
  results are surfaced, not dropped. `terminal_bench_tasks` derives the
  accounting denominator from the pinned archive and counts a task only when its
  oracle is present.
- **`Preconditions`** on `Protocol`, hashed, with `commit()` refusing a protocol
  that does not require a bindable workspace.
- **`Protocol::derive_holdout`** — the holdout from the protocol's own hash, in
  two stages because the literal reading is circular: hash everything except the
  holdout, use it as the salt, then commit the whole thing. Nested, so a fraction
  raised after results cannot become a different sample under the same rule.
- **`fixtures/bench/download.sh`** fetches Terminal-Bench pinned to a **commit**,
  not a branch — a task set that moves under a committed protocol is a protocol
  that committed to nothing.
- **`crates/bench/tests/tau2_extractor.rs`** — τ2 scoped to the claim extractor,
  appendix only, with its scope stated beside its result.
- **`Outcome::executed_checks` and `Measurement::evidence_coverage`** — the
  ruling that τ2 never reaches the headline, made structural. `overclaim_index()`
  refuses **before computing any rate** when no scored trajectory carries an
  executed check bound to a git tree. Measured by deleting the check: τ2 then
  produces a §12 headline with **AUROC 1.000** over 4,993 tool calls and zero
  executed checks. Every headline now carries `evidence_coverage`, so a
  partially bindable class-2 run cannot read as a fully bindable one.

### Changed

- **τ2-bench moves to the appendix and stays recorded.** Its coverage of zero for
  §6 verifiers is the evidence for why the corpus moved, so it is kept rather
  than deleted.

### Known gaps

- **The class-2 runs have not been executed.** The path is built and tested; the
  verifier takes the run. No number exists for either corpus yet. **`thruline
  bench` has no class-2 verb** — the path is a library API, so `REPORT.md` gives
  the eight-step sequence and the refusal each step buys rather than leaving the
  verifier to infer it.
- **τ2 cannot produce extractor precision and recall either.** Measured: 0
  completion claims over 200 final assistant messages, with the positive control
  firing on *"All tests pass"* in the same test, and 78 of 200 finals being the
  harness's `TRANSFERRED TO A HUMAN AGENT` boilerplate. The corpus's claims are
  domain assertions, not software-completion claims — the wrong vocabulary, the
  same shape of finding as the wrong evidence class. What it does measure is
  **specificity against out-of-domain prose**, reported as that.

## M5 — τ2-bench is class 1, and it cannot produce a §12 number (unreleased)

§12's harness, its metrics, the Overclaim Index report type, and the three
provenance classes the operator's amendment defines. **No §12 target is
measured**, and the reason changed between drafts of this entry — which is the
part worth reading.

### The correction

The first version of this entry said the labelled trajectories §12 asks for
"are not published anywhere". That was five sources, all of them harness-only,
generalised into a claim about the field. **τ2-bench publishes them and it
downloads today**: `sierra-research/tau2-bench` ships
`data/tau2/results/final/` — 27 result files of runs Sierra performed, each
simulation carrying the benchmark's own `reward_info`. Third-party
trajectories, third-party oracle. Four files are fetched and digest-pinned by
`fixtures/bench/download.sh`; the corpus itself is gitignored.

### And the number it yields is a zero about coverage, not an AUROC

Every §6 verifier decides from **executed code checks bound to a git tree**.
Measured over the two domains fetched: 656 trajectories, 4,993 tool calls, **0
code-check executions**. The tools are `get_reservation_details`,
`toggle_airplane_mode`, `grant_app_permission`. An AUROC over this corpus would
be a number about detectors that never ran. §12 lists τ2-bench among
false-success labelled trajectories and it is exactly that; ThruLine reads a
different evidence class, and that conflict is the operator's to rule on.

The census caught its own author first: matching tool names by substring
counted `run_speed_test` 53 times as a code-check execution. It matches whole
names now and **records the near misses** (`run_speed_test ×160`) so a reader
auditing "zero" can judge the exclusion.

### Added

- **`crates/bench/src/protocol.rs`** — the three provenance classes, each
  **carrying the evidence its claim implies**: an `ExternalSource` (publisher,
  dataset, version, and the named field the labels came from) for class 1, a
  `Commitment` obtainable only from `Protocol::commit()` for class 2, nothing
  for class 3. The protocol hash covers harness, model, task ids, holdout,
  parameters **and the detector configuration** — a test mutates each and
  asserts the hash moves. Ordering is not content, so honest re-runs are not
  refused.
- **`crates/bench/src/tau2.rs`** — the class-1 loader. Refuses bytes whose
  sha256 is not the one recorded at fetch; labels come from
  `reward_info.reward` and nowhere else; the holdout is derived from the
  artifact's own digest and is nested, so it cannot be widened after results
  and called the same rule. `evidence_census()` reports code-check executions
  by exact tool name, with near misses recorded.
- **`HoldoutLedger`** — an append-only record of which runs consumed which
  holdout, so `RunManifest::prior_runs` is derived from a file rather than
  asserted by a caller. A holdout spent twice is a refusal.
- **`OracleReading`** — a label is now a *reading*: the field it came from, the
  raw value, and the label derived from them, with private fields and one
  constructor. For class 1 the field must be the one the provenance declared.
- **Per-trajectory protocol binding** — every `Outcome` carries the protocol
  hash in force when it was produced. A corpus assembled from two runs is
  refused; a trajectory carrying no hash is refused rather than defaulted.
- **`crates/bench/tests/class_two.rs`** — the pre-registration ladder, walked.
  Every class-2 refusal existed and none was exercised; nine tests now climb it,
  ending at a correctly pre-registered run that **reports** and a crashed
  trajectory recorded as unlabelled that still reports. A scheme where nothing
  can pass is not a safeguard, and one that punishes recording a crash teaches
  you to omit it.
- **`crates/bench/tests/construction_census.rs`** — two source walks with
  negative controls: provenance and labels may only be constructed where an
  artifact is read, and the ingest loop may contain no `continue`,
  `filter_map`, `flatten`, `if let Ok` or `.ok()`.
- **`crates/cli/tests/m5_bench_cli.rs`** — including one test whose whole job is
  that the retracted finding is gone from the command's output.
- **`crates/bench`** — AUROC (rank estimator, ties at half credit), catch rate,
  coverage, per-shape false positives, nearest-rank latency, detector
  ablation. Every metric tested against a hand-computed value.
- **`thruline bench`** — exits **3** and prints which corpus is missing and
  what a probe of it found.
- **`normalize::Normalizer`** — the shape a loader must satisfy. No loaders:
  writing one for a format nobody has seen means guessing at field names and
  then testing the guess against fixtures written from the same guess.

### §12's four ways of lying, made unrepresentable

- **No pooled false-positive accessor exists**, and `meets_targets()` judges
  the worst shape. A test builds 4% pooled with one family at 20% and asserts
  the targets are missed — this build's own history, where a family measured
  80% while the average looked fine.
- **`Headline` carries `coverage`**, not optionally, so an abstaining detector
  cannot score well quietly.
- **Catch rate and false positives come from one `Measurement`**; there is no
  way to pair them across runs.
- **`overclaim_index()` refuses a `SelfAuthored` corpus.** The refusal is on
  the headline, not the data, so a self-measurement can still be shown as one.

Plus: fewer than five honest-work shapes is a refusal rather than an average,
and the HTML page will not render without a fixture version and a tool version.

### The pipeline, on real verdicts

`bench_pipeline.rs` drives the real `tests_pass` verifier and the real tamper
detectors over five honest shapes and three cheats: coverage 1.0, catch rate
1.0, AUROC 1.0, zero false positives. **Marked `SelfAuthored`, so the code
refuses to call it an Overclaim Index**, and the test asserts that refusal.

### Changed

- **`thruline bench` measures instead of asserting.** It used to print, to the
  user, that §12's sources "publish task environments, not the
  false-success-labelled trajectories §12 asks for" — the generalisation
  τ2-bench refutes. It now loads whatever corpus is present and prints counts it
  computed: trajectories, oracle-labelled, tool calls, code-check executions,
  and the near misses. It still exits **3**, because no §12 number exists. The
  digest it prints is computed here and explicitly **not** pinned — the command
  has no fetch record, and passing a file's own digest in would be an identity
  wearing a verification's name.

## M4 — the flight recorder, the other two harnesses, and a real terminal (unreleased)

§17's M4: the ratatui TUI, the Codex and Gemini adapters, and `thruline run`
under a pseudoterminal. **The done-when is met** — the same scripted session
through each harness yields the same claims and the same verdicts — and it is
tested with a negative arm, because "three harnesses agree" is satisfiable by
three harnesses that are equally blind.

### Fixed — the most serious defect this build has carried

- **A re-executed check had no identity, and §6.3's narrowing detector went
  blind on it.** `session_view` looked a check's position and command up by
  `exec_event_id`, which a re-execution deliberately does not have. The miss
  invented `argv = []` and `seq = u64::MAX`.
  - `argv = []` carries no narrowing flag, so a re-executed `pytest -k`
    presented as a clean full-suite pass and **supported a `scope: all`
    claim, beside `reexecuted: true` in the receipt** — the strongest evidence
    class the product produces, least checked.
  - `seq = u64::MAX` sorts after every edit, so §6.1 rule 2's observed half
    skipped the entire edit record. Both halves then missed an Edit-tool write
    to a gitignored, behaviour-changing path, which needs no `.gitattributes`
    and so is not caught by `capture_degraded` either.
  - Both facts were already on the `Check` row. `CheckView.seq` and `.argv`
    are `Option` now, so the absence is representable: `bind` refuses, §6.3
    raises `narrowing_unchecked` and withholds `supported` for a `scope: all`
    claim, and two runs with unrecorded commands are not compared for
    flakiness. Every guard was validated by reintroducing the sentinel.
- **Three more stand-ins for recorded facts**, swept: `capture_degraded` was
  `unwrap_or(false)`; `Comparison::of` was handed a literal `"pipe"` for the
  agent's side, which made §7's capture-shape branch unreachable; `bind`'s
  `final_seq` fell back to `u64::MAX`.
- **A document guard that fired on deletion and not on contradiction.** The
  keyless guard asserted two phrases anywhere in a 1,300-line file; changing
  the subject's sentence to its opposite left it green, because another
  sentence still carried the predicate. Subject and denial must meet on the
  same line now, and a control mutates the real documents both ways.
- **Two spools folding one session silently lost deliveries.** `seq` is a
  line's ordinal in its own spool and the fold resumes at the session's chain
  tail, so the second spool folded hit the resume guard and its lines were
  counted as already present. `shim install --harness` keeps one session in one
  spool; the fold names any session it finds in two and exits non-zero.

### Added

- **`thruline run -- <agent command>`** — §4.4's harness-agnostic path. Spawns
  the agent on a pseudoterminal, puts the exec shim on `PATH`, folds the
  result, and exits with the child's own status. `--report <path>` writes the
  JSON report to a file, which is the only unambiguous way to read it: this is
  a pass-through and the child owns stdout.
- **`capture::pty`** — a *safe* `openpty` binding (`rustix-openpty`), because
  this workspace forbids `unsafe`. The recorder's `stdio` is **measured** now
  rather than the constant `"pipe"`, which is what made §7's capture-shape
  branch unreachable.
- **`thruline tui`** — §10's flight recorder: timeline, detail, claims and
  promises panes, search, jump-to-stop. Read-only; there is no key that writes.
  Every decision is a pure method on a tested model, and a census asserts the
  crate cannot reach a verifier, so its claims pane cannot disagree with
  `thruline claims`.
- **`thruline hook codex` and `thruline hook gemini`**, behind a harness
  descriptor (BUILD-PLAN B.4) that carries each vendor's differences as data.
  The harness is recorded **on the spool line**, not read off the file name.
- **A Codex rollout reader** for the two record types §3.1 has kinds for and
  the legacy corpus left unmapped: `patch_apply_end` (56,273 rows) →
  `fs_change`, `context_compacted` (2,791) → a compaction boundary.

### The vendor contracts, re-checked today

- **[CONFLICT-9, new] Gemini's `AfterTool` is not success-only**, and the
  failure is a *field* (`tool_response.error`) rather than a second event. An
  envelope shape the adapter has not seen is refused, not coerced — the only
  value a coercion could pick is `true`.
- **Gemini has no `PostCompress`**, so a compaction boundary there has no
  observable end. Recorded as a capability gap on the event itself.
- **[CONFLICT-10, new] Gemini's `SessionEnd` is best-effort** — "the CLI will
  not wait for this hook to complete" — so nothing may depend on it running.
- **Codex's `PostToolUse` fires on a non-zero Bash exit and its documented
  payload names no outcome field**, so `ok` is not derivable: the fold records
  `ok_basis: "not_stated"` rather than guessing in either direction.
- **[CONFLICT-5 confirmed] Codex sessions are at
  `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`**, not §4.2's flat glob.

### Known gaps

Sigstore keyless signing and verification remain **not built** — unchanged from
M2, and enforced by `crates/attest/tests/no_network.rs`. `thruline watch` and
the Claude Code `FileChanged` / `TaskCreated` / `PermissionDenied` enrichment
are not built. The Gemini adapter has never seen a Gemini: its fixtures are
transcribed from the vendor document. The full list is in `REPORT.md`.

## M3 — Re-execution and the Stop gate (unreleased)

§7's `Sandbox` trait with a Docker backend, materialisation that cannot
launder, and §4.1's opt-in Stop gate. **Both of M3's done-whens are met**;
the container half is `#[ignore]`d because a Docker daemon is not a suite
dependency, and its result is recorded in `REPORT.md` with the image digest
it produced.

### Added

- **`thruline-reexec`** — §7's `Sandbox` trait (`prepare` / `run` /
  `destroy`), the Docker backend, image selection and the lockfile-keyed
  dependency layer.
  - **Materialisation writes blobs through `git cat-file` and never
    `git checkout`** (BUILD-PLAN C.7/4). A checkout applies the *smudge* half
    of any `.gitattributes` filter pair, so its content is not a function of
    the tree hash and can hold bytes that exist in no tree. The test does not
    assert this: it installs a smudge filter, runs a real checkout as the
    control, and shows the two answers differ. A source-walking walker makes
    `checkout`, `checkout-index`, `read-tree`, `restore`, `archive` and
    `worktree` unreachable from the module, with a negative control that
    `cat-file` and `ls-tree` are used.
  - A **submodule** is a hole: its content is not in the parent tree (C.7 row
    8), so a materialised tree carrying one is not `complete()` and re-exec
    refuses. Content that is not in the tree is never invented.
  - `--network none` unless `.thruline/reexec.yml` asks for `host`, pinned at
    the source level because the default is one word from inverting and no
    test output would look different.
  - §7's image order — `.thruline/reexec.yml` → `devcontainer.json` →
    `Dockerfile` → ecosystem default — with **which source answered** recorded
    on the run. Each step is tested with the later steps also present, so the
    tests show precedence. A repository that names nothing is `Undecidable`,
    never guessed at.
  - The dependency layer is keyed by lockfile bytes **plus the install
    command**: a source edit does not invalidate it, a lockfile edit does, and
    so does a changed `install:` line.
- **`thruline reexec [--mode claimed|full] [--backend docker]`**, exit **0**
  only on agreement, **2** when it ran and could not be compared, **3** when
  the gate refused or a re-run disagreed or failed at the final tree, and
  **1** when it could not run at all. The middle code is the point: without it
  a re-execution whose output could not be parsed exits 0, and a CI gate reads
  0 as a pass.
- **`thruline gate --hook stop`** — §4.1's Stop gate, installed beside the
  capture pack by `thruline hook-install --gate`. Two `Stop` entries by
  design: the capture hook must stay silent inside 50 ms, the gate must be
  able to speak and take seconds.

### The three rulings this milestone is mostly about

- **`reexecuted: true` is derived, never asserted.** A re-execution records
  §8.1's `container_image_digest` on the `Check` row; `thruline receipt`
  derives the boolean from it and `verify` refuses a receipt whose
  `reexecuted: true` has no `sha256:`-shaped digest behind it — plus the
  inverse, so a digest paired with `reexecuted: false` is a record disagreeing
  with itself.
- **The gate refuses; it does not annotate.** A degraded session makes
  `reexec` exit 3 with `tree_fidelity_broken` and run nothing, so
  `reexecuted: true` is unreachable for it. Re-running honest code to confirm
  a claim about cheating code *is* the laundering.
- **The Stop gate blocks on `unsupported` and only on `unsupported`.**
  Widening to `!Supported` would block on `unverifiable`, which accuses the
  agent where ThruLine could not observe enough to decide — §6 read
  backwards. Three guards: the narrow predicate, a source census against the
  widening, and a behavioural control that the two predicates genuinely
  disagree on `unverifiable`.

### Known gaps

- **`--mode full` detects the canonical command; it does not read
  `.thruline/policy.yml`.** §7 names the policy file and §9's policy engine is
  M6.
- **The cloud backend is not built.** §7 puts it behind a feature flag;
  naming it fails rather than falling back to Docker.
- **`--no-object-write` does not exist**, so C.7's second refusal reason is
  unreachable. The snapshot engine has no such mode yet.
- A re-executed check is compared on **exit code and parsed tally**, never on
  output digests. `Comparison::of` refuses across a `pipe`/`tty` mismatch and
  says so; a PTY is §4.4's `thruline run` (M4.5), so the mismatch branch is
  unit-tested rather than exercised end to end.
- **A re-execution appends nothing to the session's chain**, and a test asserts
  the head is unchanged. The first version did append, and it ate hook
  deliveries: the fold resumes at `seq <= last_seq`, so a foreign writer that
  advances the chain tail makes the next spool line count as already present —
  the defect `SPOOL_SEQ_STRIDE` was introduced to fix, re-entered through a new
  writer. The cost is that the digest lives on the derived `checks` table
  rather than in the append-only chain, which is the standing every other
  `Check` in a receipt already has.
- The block budget lives in a file under `$THRULINE_HOME/gate/`, not in the
  ledger: `events` is append-only and a block is loop-guard bookkeeping, not
  something that happened to the code. The harness's own cap of 8 consecutive
  blocks is the backstop if the file is removed.

## M2 — Receipt, offline verification, and the Action (unreleased)

in-toto Statement in a DSSE envelope, local-key signing, fully offline
verification, the commit trailer, and §9's GitHub Action, check run, PR table
and badge. **Sigstore keyless is not built** — see Known gaps.

### Added

- **`attest::dsse`** — DSSE envelopes and the PAE encoding, with its own test
  vector rather than only a round trip: a verifier that reconstructs PAE
  differently from the signer fails as "signature invalid", which reads as
  tampering.
- **`attest::statement`** — the in-toto Statement and §8.1's predicate. The
  predicate URI is **provisional** (§18's default) and lives in exactly one
  constant, so changing it is a one-line edit.
- **`attest::key`** — PKCS#8 PEM ed25519, `0600`, refusing to overwrite.
- **`attest::verify`** — offline verification whose result is **a list of named
  checks, not a boolean**. `Pass` / `Fail` / `Skipped` / `NotApplicable`, and
  `ok()` requires every check to be `Pass` or `NotApplicable`. `NotApplicable`
  is reachable **only when the receipt asserts nothing** on that axis, never
  when the machine is missing something.
- **`attest::trailer`** — the `ThruLine-Receipt:` line. This module writes
  nothing and has no path that could amend a commit.
- **`attest::report`** — §9's three renderings of one conclusion: the GitHub
  check run, the PR comment table and the `verified-by-thruline` badge.
  `conclusion()` is the only producer of `"success"`, and it produces it only
  for a fully verified receipt.
- **`packaging/action/`** — `thruline/action`, a composite action that posts a
  check run and a PR comment table. It **consumes** the conclusion and never
  derives one; `crates/cli/tests/m2_action.rs` walks its running lines and
  fails on `.checks`, `.outcome`, `$?` or any `outcome !=` test.
- **`spec::git_env`** — C.7's S3 and §8.1's `capture.git_env_digest`: a digest
  over the attribute sources and their content hashes, every `filter.*`
  *value*, twelve `core.*` keys, `.git/info/exclude`, the `.gitmodules` set,
  and whether the clone is shallow or sparse. Four of C.7's fourteen rows live
  outside the tree, so no diff-based detector can reach them; this is how they
  reach a receipt.
- **CLI**: `thruline receipt`, `thruline verify-receipt
  [--format text|json|md|check-run|badge] [--head-sha]`. Exit **0** when every
  check passed, **1** when one failed, **2** when nothing failed and something
  could not be checked here.

### Fixed

- **The `verified` field name invited the bug it was built to prevent.** It
  was a tri-state *string*, and `"partial"` and `"no"` are both truthy in
  Python and JavaScript — so `if r["verified"]:`, which is exactly what that
  name suggests, read a partially verified **and a failed** receipt as
  success. It is now a real boolean beside `status`
  (`verified` / `partial` / `failed`), both written from one call so they
  cannot disagree, with a test that reads every conclusion the way a script
  would.
- **A shallow clone made honest receipts fail.** `actions/checkout` defaults
  to `fetch-depth: 1`, and `cat-file --batch-check` truthfully answers
  `missing` for objects a shallow clone lacks — so the Action as commonly
  configured would have posted a red check run on entirely honest work. A
  missing object in a shallow clone is now `Skipped` with a reason naming
  `fetch-depth: 0`, not `Fail`. Not a loosening: `Skipped` still blocks
  `ok()`, so nothing reaches "verified" that would not have. Proven with a
  real `--depth 1` clone plus a full-clone control.
- **An agent's own sentence could put markup in the PR comment.** The claim
  text is whatever the agent wrote and the comment is rendered markdown, so
  `![](…)` rendered as a request made by whoever opened the pull request —
  telling a third party who read it and when, which is verbatim §0.8's reason
  for `explain --html` fetching nothing. Every cell now goes through one
  function that makes it an inline code span; six hostile shapes are unit-tested
  and one is driven end to end through the binary.
- **The key accuser decoded nothing.** `contains_accusable_key_block` asked a
  shape question — base64 alphabet, ≥32 characters, mixed case and digits —
  and three constructed non-keys still accused. The body is now
  base64-decoded and the bytes are asked what they are: `openssh-key-v1\0`,
  an OpenPGP packet header, or a DER `SEQUENCE` whose first element is one a
  private key structure begins with. The masker is untouched; its over-mask is
  fail-safe and every fixture asserts it still fires.

### Known gaps

- **Sigstore keyless is not built.** §8.1 hedges the `sigstore` crate's
  verification completeness itself and BUILD-PLAN M2.4 calls it a spike. Local
  keys ship first by design, and they are what M2's done-when needs. Nothing
  in the workspace contacts Fulcio, Rekor or a TUF root, and no code path
  could.
- **Rekor is not contacted, and nothing in the verify path can.** Upload stays
  opt-in per §0.8 when it exists.
- **The predicate carries no `materials`.** The subject already names the
  tree, and a materials list that restated it would be decoration; the
  meaningful entries (base tree, container image) arrive with §7's records and
  are not wired into `materials`.
- **The badge endpoint is a route function, not a server.** §9's hosted badge
  belongs to the GitHub App, which is M6. `badge_route` exists and is tested;
  nothing mounts it.

## M1 — Claims, verifiers, Explain (unreleased)

Rules-first claim and promise extraction, §6.1 final-tree binding, the
`tests_pass` verifier, tree-sitter tampering detectors for five languages, and
`thruline explain`.

### Added

- **`thruline-claims`** — §5.2's ordered pattern table in editable YAML, with
  negation handled at extraction (ruling C.9) and clause-scoped so a cue cannot
  reach across `and` / `but` / `;` and drop a real claim. Claims carry their
  exact span. Verifiers receive `VerifiableClaim { id, kind, args }`, which
  serialises to exactly three keys — no `text`, no `span`, no `extractor`, no
  `confidence` (ruling R3).
- **`verify::binding`** — §6.1, recording **which** rule let a check count. A
  test-only edit after a check relaxes binding and is counted rather than
  hidden.
- **`verify::tests_pass`** — §6.3: narrowing from argv, flakiness, skips,
  lucky-pass ordering, a failing run outranking a passing one, and degraded
  capture that can never reach `supported`.
- **`verify::tamper`** — §6.7 over tree-sitter for Python, Rust, JavaScript,
  TypeScript and Go.
- **`verify::verdict`** — `Verdict` with **no `Default`**, because a defaulted
  verdict is `Supported` under any derive that picks the first variant, and
  `worst()` never moves toward `supported`.
- **`thruline-explain`** — §10's six questions, the stop-reason classifier and
  the compaction-loss report. §6.9 is narrative only, enforced by the dependency
  direction: `verify` depends on no ThruLine crate at all.
- **CLI**: `thruline claims`, `thruline explain` (text / `--json` / `--md` /
  `--full`). Both exit **3** on an unsupported claim, so CI can tell a refuted
  claim from a broken command.
- **`fs_change` events**, from worktree observation and never from a tree diff
  (C.7/S1). A `Bash` tool names no path, so none is invented for it.

### Fixed

- **Every fold-derived snapshot in a session carried the same tree hash.** The
  fold runs after the session, so it snapshots *now* while labelling the event
  with a *past* `seq`; §6.7 compared the final state against itself and found
  nothing. Snapshots are now stamped `taken_at` (`fold` / `exec`) and §6.7 takes
  its base from `head_commit`.
- A comment reading `# we used to @pytest.mark.skip this` was reported as a skip
  marker; decorators now come from the tree, and line counting had also reached
  into the previous function's body.
- `cargo test`'s subcommand was read as a named test path, so every full cargo
  run looked narrowed.
- A check whose output could not be parsed was filtered out before it could be
  reported, so it read as "the agent ran nothing".
- `pytest -q`'s bannerless tally is recognised (carried over from M0.8).

### Added, finishing M1

- **§6.4** `build_ok` / `lint_ok` / `typecheck_ok`, **§6.5**
  `behavior_unchanged`, **§6.6** the diff-evidence claims, **§6.8**
  special-casing heuristics, **§6.3's** coverage clause, **§6.7's** mock-target
  detector, and §6.2 parsers for go test, jest/vitest, mocha and JUnit XML.
- `thruline promises` (§14), with cross-session fulfilment.

### Known gaps

- `task_complete` has no verifier: it needs the harness's TaskCreated /
  TaskCompleted events, which this build does not subscribe to (M4).
- `go test`'s fixtures are shapes written from documented formats — there is no
  Go toolchain on this machine. pytest, cargo, jest, mocha and JUnit XML are
  captured from real runs.
- The coverage floor is hardcoded at 70% until §9's policy engine exists;
  `.coverage` and tarpaulin's native format are not read.
- §6.5's two-tree requirement is rarely met until §7's re-execution (M3).
- §6.7's literal-changed-while-source-changed combination is not wired.
- `explain --html` renders one self-contained file, with nothing fetched and
  everything the agent wrote escaped.

## M0 — Foundation (unreleased)

Event model, hash chain, canonicalisation, snapshot engine, ledger, redaction,
the Claude Code hook adapter, the spool fold, the PATH-shim exec recorder, two
runner parsers, and `doctor`.

### Added

- **`spec::event`** — the §3.1 event, with one construction gate every path
  passes (`TryFrom<EventWire>`), private fields, RFC 8785 canonical bytes, and a
  hash preimage derived by deleting one key from the real serialisation rather
  than mirroring the field list.
- **`spec::chain`** — chain verification returning a head, plus `ChainWalker`
  for constant-space verification of a session of any length.
- **`spec::canonical`** — RFC 8785 via `serde_jcs`, verified against the
  reference vectors as bytes and differentially against a second implementation.
- **`spec::snapshot`** — the §4.4 engine: `git add -A` into a temporary
  `GIT_INDEX_FILE`, `write-tree`, honouring `.gitignore` and `.thrulineignore`.
  Reports content filters that break tree binding, and entries git cannot
  represent.
- **`thruline-ledger`** — the §11 store at `~/.thruline/exec.db`, WAL, per-repo
  namespaces, append-only events by trigger, `PRAGMA user_version` migrations,
  and a streaming legacy importer.
- **`thruline-capture`** — §4.6 redaction wrapping the existing masker, the
  §4.1 Claude Code hook adapter, and the spool fold.
- **`capture::shim`** — the §4.4 PATH shim directory: eleven POSIX `sh`
  wrappers for `pytest`, `cargo`, `go`, `npm`, `pnpm`, `yarn`, `npx`, `make`,
  `mvn`, `gradle` and `dotnet`, byte-deterministic in their inputs and hashed in
  a manifest so `doctor` can name a wrapper that was edited, removed, left off
  `PATH`, or shadowed by an earlier `PATH` entry. Each wrapper **fails open**:
  with the recorder missing it strips the shim directory out of `PATH` and runs
  the real program rather than breaking the user's shell.
- **`capture::exec`** — the recorder the wrappers invoke. Transparent to exit
  codes and signals, streams stdout/stderr through unmodified, records the §3.1
  `exec` payload with a git snapshot before and after, and refuses any candidate
  carrying the wrapper marker so it can never re-enter itself.
- **`capture::stream`** — redact, then hash, then bound, over an unbounded
  stream in bounded memory. Blocks end only at a newline and never inside a
  `-----BEGIN … -----END` region, because a boundary in the wrong place unmasks
  a secret. Gated by a **byte-equivalence** test against the whole 1,958-row
  redaction corpus at one-byte chunks.
- **`doctor`'s `shim-bypass` finding** — §13's *"PATH shims bypassed"*, made
  checkable by M0.8: a session whose tool input records a recognised test runner
  and whose chain holds no `exec` event. The commands are parsed through
  `verify::shell`, so a mention is not a hit, and the transcript is used only to
  mark the record incomplete.
- **`verify::shell`** — §13's `sh -c` payload parser. Refuses every construct it
  cannot expand rather than producing a best-effort argv.
- **`verify::check`** — the §3.3 `Check` draft from a captured exec, with no
  verdict and no defaulted `parsed` block.
- **`thruline-verify`** — §6.2 parsers for pytest and cargo test.
- **CLI**: `thruline hook`, `thruline shim install|uninstall|status`,
  `thruline fold`, `thruline import-legacy`, `thruline doctor`,
  `thruline hook-install`.

### Deviations from the spec, each recorded in `docs/BUILD-PLAN.md`

- **D1** — the §11 store is `~/.thruline/exec.db`, not `ledger.db`. The existing
  6.9 GB transcript ledger holds a different evidentiary tier and its schema
  version is a hash of its DDL, so adding tables would force a two-million-row
  migration for tables the old schema has nothing to do with.
- **D2** — hashed event payloads carry integers only, bounded to ±(2^53−1).
  Beyond that RFC 8785 rounds through an IEEE-754 double and distinct events
  share a hash preimage. Measured: `383151015300511101` and `…102` both
  canonicalise to `…100`.
- **D3** — the hook path takes no snapshot and opens no database. §4.1's 50 ms
  p95 cannot survive `git add -A` plus a WAL open. Snapshots moved to the fold.
- **D4** — migrations use `PRAGMA user_version` and ordered embedded statements.
  `refinery` caps `rusqlite` at ≤0.39 against this tree's pinned 0.40.1 with
  `links = "sqlite3"`; `sqlx migrate` needs an async runtime, which `CLAUDE.md`
  forbids before the watcher milestone.
- **N2** — event text is normalised to NFC at construction. RFC 8785 does not
  normalise, and macOS hands back decomposed filenames where Linux does not, so
  the same file captured on two machines would otherwise fork the chain.

### Fixed after verification

- **A third-party PATH shim could make the recorder re-enter itself forever.**
  `pyenv`, `asdf`, `rbenv` and `ccache` install shims that carry neither our
  content marker nor our path, so the recorder resolved *to* one, and it
  re-dispatched the bare program name through `PATH` — back into our wrapper.
  Measured as a 25 s hang with the program never running: the exact outcome the
  fail-open design exists to prevent. `THRULINE_SHIM_ACTIVE` now makes a wrapper
  fail open when its own program is already being recorded, checked **in the
  wrapper** — the recorder's own escape hatch ran the resolved program, which in
  this cycle is the third-party shim, so it looped at an unchanged depth. Keyed
  on the program rather than on depth so `make` → `pytest` is still captured.
- **`spool-latest` was every default install's only route, and misbinds under
  concurrency.** `$CLAUDE_CODE_SESSION_ID` is now read first after the explicit
  override — verified byte-equal to the name of a real
  `~/.claude/projects/*/<session_id>.jsonl` transcript, which is the id the hook
  payload carries. Measured misbind before the fix: 49.7% with two equally
  active sessions.
- **A captured run hid every bypassed one after it.** `shim-bypass` asked
  whether a session had *no* execs; it now compares counts per session, so
  `pytest` (green, captured) followed by `/usr/bin/pytest --ignore=failing`
  (invisible) is caught.
- **A here-document's body was parsed as commands** by `verify::shell`; a body
  containing `pytest -q` would have produced a command that never ran. Refused
  now, like every other construct the parser cannot expand.
- **An argv flag now names the element after it.** `--password s3cr3t-opaque`
  survived as two elements while the identical text on one line was masked —
  same secret, two answers, decided by whether a shell split it. Uses
  `key_implies_credential`, the same predicate the JSON walk and the `.env` rule
  ask.
- **The exec event records `stdio: "pipe"`.** A TTY/non-TTY difference changes
  the bytes, and §7's re-execution comparison would otherwise read it as a
  digest mismatch — tampering where there is none.

### Changed

- Licensed **Apache-2.0** (was MIT), per §15 and the operator's ruling.
- **Spool `seq` is the line's ordinal times a stride of 4.** A fold-derived
  snapshot was numbered `line_seq + 1`, which is the next line's own ordinal;
  that line then hit the resume guard and was counted as *already present*. A
  real hook delivery vanished and the report called it idempotence.
- **`is_env_style_key` accepts lower-case and camelCase keys, and the text rule
  now calls `key_implies_credential`** — the same predicate the JSON walk uses,
  asked the same way. `aws_secret_access_key=…`, the spelling
  `~/.aws/credentials` uses, was reaching stored output in plaintext because the
  predicate said "credential" and this caller never asked it.
- **`parse_pytest` recognises the bannerless `-q` tally.** Real `pytest -q`
  prints `2 passed in 0.00s` with no `=====` banner; the `-q` fixtures in the
  parser's own tests carried one, so the first output this system captured
  itself came back `unverifiable`.
- **`FoldReport` counts three things it previously assumed**: snapshot events of
  every origin (not only fold-derived ones), a quarantine file that could not be
  written, and checks whose captured output could not be read.

### Deviation recorded here because it is a measurement, not a choice

- **BUILD-PLAN M0.8(d)'s "< 30 ms added to a trivial wrapped command" is not
  met, and cannot be while §4.4 stands.** Release build, ten runs each: bare
  14 ms, through the shim **166 ms** on a five-file repository and **208 ms** on
  this 602-file one. **36 ms** of that is the wrapper, recorder and child spawn
  measured outside any git repository; the rest is the two snapshots §4.4
  mandates — `git add -A` + `write-tree` alone costs 27 ms and 61 ms on those
  two trees. Surfaced rather than blended (Rule 7): the spec wins on *what*, and
  the plan's figure was a target rather than a measurement. What the 200 ms
  debounce suppresses is a duplicate snapshot **event**, never the snapshot
  itself — debouncing the measurement would bind a check to a tree it did not
  run on.

### Known gaps

- **M0 computes no verdicts.** `exec` events and parsed §3.3 checks now exist,
  so `tests_pass` has evidence to be checked against — and nothing that checks
  it. §6.3 is M1.
- A command interrupted with Ctrl+C loses its `exec` event: `SIGINT` reaches the
  whole process group. What survives is the `exec_begin` note and the
  pre-snapshot, so the gap is counted rather than silent. Closing it needs
  `unsafe`, which the workspace forbids.
- The wrapper exits `128 + signal` rather than dying of the signal; `$?` is
  identical, `WIFSIGNALED` is not.
- The child's streams are pipes, not a TTY, so TTY-gated output is absent from
  what is stored. A PTY is §4.4's `thruline run` (M4.5).
- §6.3–§6.9 verifiers, §7 re-execution, §8 receipts and the §12 bench are
  scaffold only.
