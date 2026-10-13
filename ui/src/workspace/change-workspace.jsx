import React from 'react'
import { createPortal } from 'react-dom'
import { HQ_COPY } from './hq-copy.js'
import { fill, SlotError } from './hq-narrate.js'
import { API, CHANGE, CHANGE_APPLICABILITY, CHANGE_COVERAGE, CHANGE_FINDINGS, CHANGE_INPUTS, CHANGE_PROPOSALS, readProjection, writeAction } from './data.js'
import { adaptSession, autoLayout } from './adapter.js'
import { buildVerdict } from './verdict-model.js'
import { focusView } from './focus-set.js'
import { Card } from './verdict-panel.jsx'
import { Blocked } from './blocked.jsx'
import { buildChangeView, nothingCheckedYet, projectNameOf, STAGES, workReadCount } from './change-view.js'
import { deriveChangeModel, movedSentences, versionSentence, foldList, INSPECTOR_FOLD_AFTER, showsDecision, linkedWorkLine, claimCaptureOf } from './change-model.js'
import { buildCoverageView, evidenceEntries, runCheckCommand, servedUnlinkedLines, unlinkedCompletedRun } from './coverage-view.js'
import { buildPrepareView } from './prepare-view.js'
import { CriterionCoverage, DECISION_BLOCK, FailureBlock, WhatBrokeDoor } from './criterion-links.jsx'
import { ShellSidebar, ShellTopBar } from './shell-chrome.jsx'
import { AskPanel } from './ask-hq.jsx'
import { EVIDENCE_ANCHORS, openDoor } from './ask-hq.js'
import { doorFromSearch, searchWithoutDoor } from './door-request.js'
import { initialAltitude, rememberAltitude } from './altitude.js'
import { RUN_SHEET, RunDoor, RunSheetHost } from './run-sheet.jsx'
import { FindingStandings, ReviewSection, ReviewSummary } from './change-review.jsx'
import { buildChangeReviews, buildFindingStandings } from './review-model.js'
import { readStamp } from './read-stamp.js'
import { cardsTold, tellDraftOf } from './tell-draft.js'
import { goToInbox } from '../routes.js'
import { licenceView, LICENCE_HASH, PLANS_URL } from './licence-view.js'
import './workspace.css'

/* The Change workspace — the first user-facing surface of the delivery loop
 * (T-2-4; `B8.5`–`B8.7`, `B4.1`–`B4.3`, `B8.10`–`B8.12`, `B7.1`–`B7.4`,
 * `B8.14`).
 *
 * WHAT THIS SCREEN IS. One Change, opened at `#/changes/<id>`. The top answers
 * "what am I being asked to accept?" in ten seconds: one title, one sentence,
 * one named version, in that order, in everyday English, with nothing between
 * them. Under it, the six stages as a strip of VIEWS. Everything technical —
 * ids, digests, revisions, how a link was made — is inside one `<details>` at
 * the bottom, which is one click, never hidden.
 *
 * WHAT IT MAY DO. Read, and — since T-S-4 — record exactly one decision.
 * `readProjection` is the only read I/O and every address it is given is a GET
 * (`data.js` `API.changes`, `CHANGE`, `CHANGE_PROPOSALS`, `API.explain`,
 * `API.claims`, and — since T-DIST-4 — `API.licence`, which says whether this
 * machine may approve; the server refuses an unlicensed approval on its own,
 * this read only lets the button say so before it is clicked).
 *
 * THIS FILE USED TO SAY "there is no POST in this file", and that was true
 * until T-S-4. It is corrected here rather than left standing, in the same way
 * CLAUDE.md records the widening of the read layer's address map. There are now
 * exactly TWO write addresses, both declared beside the `Decision` component:
 * `POST /api/changes/<id>/decisions` and `.../decisions/withdraw`. Neither is
 * reachable by navigating: moving between stages still writes nothing, which is
 * asserted by test both ways (no write address in the read map, and zero
 * further requests after a stage click), and a decision is only ever written by
 * a person clicking the one primary control with their name in the box.
 *
 * P6-5 widens it once more, recorded here rather than silently: the run sheet
 * (`run-sheet.jsx`, mounted here through `RunDoor`) writes to exactly two more
 * addresses — `POST /api/changes/<id>/run` and `.../run/permission`, both
 * declared beside the sheet's one control and both carrying this launch's
 * token — and its Cancel reaches the T-3-4 stop address declared below
 * (`stopRun`), not a copy of it. Opening the sheet writes nothing; only a
 * press of its one primary control does. `change-workspace.test.js` enumerates
 * all five.
 *
 * WHAT IT MAY SAY. Every user-visible string comes from `HQ_COPY.change`, with
 * two disclosed exceptions, both of which are the RECORD speaking rather than
 * this product: the Change's own title and request text, and the finding cards,
 * which render through `verdict-model.js` and its own `HQ_COPY` families
 * unchanged. A literal sentence in this file would fail the fence test.
 *
 * WHAT IT REFUSES TO SAY. No state word at the top. No percentage. No colour
 * carrying meaning on its own — every state on this screen has a word beside
 * it. Nothing green, ever: "Ready for approval" is the strongest sentence this
 * product has and this build cannot yet say it. A stage with nothing in it
 * says what would put something there. Every disabled control says why and
 * what would enable it.
 */

const C = HQ_COPY.change

/* ── reading ──────────────────────────────────────────────────────────── */

/**
 * The documents behind one Change, read once, in one place.
 *
 * The Change first, because without it there is no screen; then the proposals
 * and the confirmed work in parallel, because each is optional and a screen
 * that refuses to open because an optional read failed is a worse screen. A
 * failed optional read is SAID, never swallowed (Rule 12): `notes` carries the
 * refusal text for anything that did not arrive.
 *
 * This was the body of `readChange` until T-2-5, which needs the same
 * documents for `packet.jsx`. It is split out rather than copied: two readers
 * over one set of routes are two places for the same fact to differ, which is
 * the rule `data.js` states at the bottom of its own file. `readChange` below
 * is what it always was, minus these lines.
 *
 * @param {string} id
 * @returns {Promise<{status: string, change?: object, proposals?: object|null,
 *   work?: object[], refusal?: string, notes: string[]}>}
 */
export async function readChangeDocuments(id, options) {
  const notes = []
  const head = await readProjection(CHANGE(id))
  if (head.status === 'refused') return { status: 'refused', refusal: head.refusal, notes }
  if (!head.doc || !head.doc.change) return { status: 'not_found', notes }
  const change = head.doc.change
  /* Where each confirmed conversation's own log holds the agent's final claim,
   * served BESIDE the Change on the same route (`agent_log`,
   * crates/core/src/agent_log.rs). A file and a line, and nothing else is read
   * from an entry: the words are never served, and a field this build does not
   * name is not carried. Keyed by session; a conversation the server could not
   * place has no entry here, and `unrecordedWorkOf` uses a place only for work
   * whose words were not recorded. */
  const placed = new Map()
  const agentLog = head.doc.agent_log
  for (const entry of (agentLog && Array.isArray(agentLog.sessions) ? agentLog.sessions : [])) {
    const r = entry && entry.receipt
    if (r && typeof r === 'object' && entry.session_id) placed.set(String(entry.session_id), { file: r.file, line: r.line })
  }

  const proposalsRead = await readProjection(CHANGE_PROPOSALS(id))
  if (proposalsRead.status === 'refused') notes.push(proposalsRead.refusal)
  const proposals = proposalsRead.doc ? proposalsRead.doc.proposals : null

  /* What bears on each accepted line (T-3-1). A failed read is SAID, and the
   * screen then shows every line as having nothing behind it — which is what
   * an unreadable record honestly means, and is never a pass. */
  const coverageRead = await readProjection(CHANGE_COVERAGE(id))
  if (coverageRead.status === 'refused') notes.push(coverageRead.refusal)
  /* T-3-5 — the readiness answer travels WITH the coverage it was computed
   * from: one object, so the obligations on screen and the answer above them
   * can never come from two different reads of the store. */
  /* T5/T6 — the typed test counts of each check's latest run live on the run
   * sheet's document (`GET /api/changes/<id>/run`, `checks[].last`), not on
   * the coverage document. The Change screen and the packet ask for it, so
   * the message for the agent and the repair history can name a failing run's
   * own counts; the Inbox does not, and pays nothing. A failed read is SAID,
   * and every count is then absent — never zero, never a pass. It travels
   * WITH the coverage, like readiness, so the one item derivation joins it. */
  let runChecks = null
  if (options && options.runs) {
    const runRead = await readProjection(RUN_SHEET(id))
    if (runRead.status === 'refused') notes.push(runRead.refusal)
    runChecks = runRead.doc && Array.isArray(runRead.doc.checks) ? runRead.doc.checks : null
  }
  const coverage = coverageRead.doc
    ? { ...coverageRead.doc.coverage, readiness: coverageRead.doc.readiness || null, review_findings: coverageRead.doc.review_findings || null, ai_review: coverageRead.doc.ai_review === true, run_checks: runChecks }
    : null

  /* What exactly will be examined (T-3-2). A failed read is SAID, and the
   * Prepare view then says nothing is written down — which is what an
   * unreadable record honestly means, and never a claim about a version. */
  const inputsRead = await readProjection(CHANGE_INPUTS(id))
  if (inputsRead.status === 'refused') notes.push(inputsRead.refusal)
  const inputs = inputsRead.doc || null

  /* Whether each piece of linked evidence still applies (T-S-2). A failed read
   * is SAID, and the model then holds no freshness at all — which is what an
   * unreadable record honestly means, and is never "still applies". */
  const applicabilityRead = await readProjection(CHANGE_APPLICABILITY(id))
  if (applicabilityRead.status === 'refused') notes.push(applicabilityRead.refusal)
  const applicability = applicabilityRead.doc || null

  /* Where each thing a review found stands (T-4-2). A failed read is SAID,
   * and the section is then not drawn at all — never "nothing was raised". */
  const findingsRead = await readProjection(CHANGE_FINDINGS(id))
  if (findingsRead.status === 'refused') notes.push(findingsRead.refusal)
  const findings = findingsRead.doc || null

  /* Only confirmed work is read at all. A session that is merely suggested
   * costs no request, which is the cheapest possible way of guaranteeing it
   * contributes no card and no node. */
  const view = buildChangeView({ change, proposals, work: [], evidence: [] })
  const work = []
  /* An execution is not a conversation (T-3-2b). A session `thruline check`
   * wrote holds one captured command and no prompts, so asking the transcript
   * route for its story answers `invalid_session` — and the screen printed
   * that refusal under the covered folders, which is an engine word in front
   * of a person for no reason. The served list says which linked sessions are
   * executions; what they showed is already on the criterion above. */
  const executions = new Set((coverage && Array.isArray(coverage.execution_sessions) ? coverage.execution_sessions : []).map(String))
  for (const sessionId of view.scope.confirmed) {
    if (executions.has(String(sessionId))) continue
    const [explain, claims] = await Promise.all([
      readProjection(API.explain + '?session=' + encodeURIComponent(sessionId)),
      readProjection(API.claims + '?session=' + encodeURIComponent(sessionId)),
    ])
    if (explain.status === 'refused') { notes.push(explain.refusal); continue }
    if (claims.status === 'refused') notes.push(claims.refusal)
    let session
    try {
      session = adaptSession({ explain: explain.doc, claims: claims.doc || null, key: sessionId })
    } catch (e) {
      notes.push(e && e.message ? e.message : String(e))
      continue
    }
    let verdict = null
    try { verdict = buildVerdict(session) } catch (e) { notes.push(e && e.message ? e.message : String(e)) }
    let focus = null
    try {
      const laid = autoLayout(session)
      focus = focusView(laid.nodes || [], laid.wires || [], { degraded: Boolean(session.capture && session.capture.degraded) })
      focus = { ...focus, labels: labelsOf(laid.nodes || [], focus.shown), groups: focus.groups }
    } catch (e) { notes.push(e && e.message ? e.message : String(e)) }
    /* `steps` is the reading's own count of what the record holds for this
     * piece of work — a typed field of the verdict, never an estimate. The
     * page that states what a Change's record is made of (rule 13) sums these
     * and nothing else. */
    const steps = verdict && verdict.checked && typeof verdict.checked.events === 'number' ? verdict.checked.events : null
    /* UNRECORDED-WORK-NOT-SILENT-R2 — whether this conversation's end was
     * recorded, so that a claim would have been captured, and how many were:
     * from the served documents themselves, never from `steps`. A refused
     * claims read is unknown (null), never zero. */
    const { ended, claims: claimCount } = claimCaptureOf(explain.doc, claims.status === 'refused' ? null : claims.doc || null)
    work.push({ id: sessionId, findings: verdict ? verdict.findings : [], verdict, focus, steps, ended, claims: claimCount, log: placed.get(String(sessionId)) || null })
  }

  /* Optional reads remain visible on the Change screen as a partial record.
   * The Inbox also needs to know that this was not a full reading before it
   * can decide whether the Change needs attention. Storage is read elsewhere
   * and never enters this completeness decision. */
  return { status: 'ready', change, proposals, coverage, inputs, applicability, findings, work, notes, incomplete: notes.length > 0 }
}

/**
 * Everything the Change screen needs, read once.
 *
 * @param {string} id
 * @returns {Promise<{status: string, model?: object, refusal?: string, notes: string[]}>}
 */
export async function readChange(id) {
  const read = await readChangeDocuments(id, { runs: true })
  if (read.status !== 'ready') return read
  const model = buildChangeView({
    change: read.change,
    proposals: read.proposals,
    work: read.work,
    /* The top sentence counts THROUGH the mapping (T-3-1): one entry per
     * standing link, carrying whether it bears. It is no longer a count of
     * pieces of linked work wearing a criterion's name. */
    evidence: evidenceEntries(read.coverage),
    unlinkedRun: unlinkedCompletedRun(read.coverage),
    unlinkedLines: servedUnlinkedLines(read.coverage).length,
  })
  model.coverage = buildCoverageView(read.coverage)
  model.examined = buildPrepareView(read.inputs)
  /* T-S-3 — the ONE derived view-model. Every state word, version string and
   * count this screen renders comes off `model.hq`, and the Inbox row and the
   * packet page read the same derivation from the same documents. */
  model.hq = deriveChangeModel(read.change, read.coverage, read.applicability, read.proposals, read.work)
  /* NEVER-ASSESSED-ON-SCREEN — the top sentence of the Explore view is the
   * model's: the readiness-aware conclusion, not a second reading of the counts. */
  model.conclusion = model.hq.conclusion
  model.reviews = buildChangeReviews(read.change)
  model.standings = read.findings ? buildFindingStandings(read.findings, read.change.id) : null
  /* T-DIST-4 — whether this machine may approve. A read that did not arrive
   * is `unknown`, and Approve then stays off and says it could not find out;
   * it is never read as "licensed". */
  const licenceRead = await readProjection(API.licence)
  model.licence = licenceView(licenceRead.status === 'ready' ? licenceRead.doc : null)
  return { status: 'ready', model, notes: read.notes }
}

/** The shown nodes' own words, so the focus set reads as a list and not as ids. */
function labelsOf(nodes, shown) {
  const byId = new Map(nodes.map((n) => [String(n.id), n]))
  return shown.map((id) => {
    const n = byId.get(String(id))
    const raw = n && (n.title || n.sub) ? String(n.title || n.sub) : String(id)
    /* A node whose own words are a detector id — `test_run_failed` — reaches
     * this list as an internal name. The copy file already holds the plain
     * phrase for every detector the engine emits, and the baseline's H2T-5 was
     * exactly this: internal shorthand sitting on the surface labelled Plain.
     * An id with no entry keeps its own words rather than being dropped. */
    const text = HQ_COPY.detectors[raw] || raw
    return { id: String(id), text }
  })
}

/* ── the pieces ───────────────────────────────────────────────────────── */

/** The one sentence. It is a `<p>`, in the same text block as the title and the version. */
export function Conclusion({ conclusion }) {
  return <p className="ws-change-conclusion">{fill(C.conclusion[conclusion.key], conclusion.values)}</p>
}

/**
 * The lines the list under the sentence holds: the ones the served answer says
 * still need evidence (`readiness.tally.needing`, the one count), and never the
 * identity or a review (`readiness.also` says those) or a line set aside (the
 * set-aside list says those). With no tally, the answer's own missing list.
 */
function shownLines(readiness) {
  return readiness.tally ? list(readiness.tally.needing) : list(readiness.missing)
}

/**
 * The one line that names the version, wherever it is rendered.
 *
 * Four shapes, and the third is the one the walkthrough found: a record can
 * hold a revision of nothing but zeros, and `version 0000000` is a placeholder
 * printed as if it were a fact. The model decides which shape (`targetVersion`);
 * this only renders it, so the top of the screen and the Approve view can never
 * name the version differently.
 */
/* T-S-3: the branching used to live here, which made the Change screen the
 * owner of a sentence the Inbox and the packet also print. It is
 * `versionSentence` in `change-model.js` now — one derivation, three screens —
 * and this is the re-export every existing import already reaches for. */
export const versionLine = versionSentence

/** The named version, in the Plain text rather than in a chip beside it (H2T-4). */
export function TargetVersion({ version }) {
  return <p className="ws-change-version" aria-label={C.version.label}>{versionLine(version)}</p>
}

/* A control that is off, with both halves of the reason beside it. Moved to
 * `blocked.jsx` at T-3-1 because the criterion list needs the same rule and a
 * second copy of it is a rule that can drift; re-exported here so every
 * existing import of `Blocked` from this file keeps working. */
export { Blocked }

/** The six stages, as a strip of views. Clicking one changes the view and nothing else. */
export function StageStrip({ stages, active, onGo }) {
  /* Roving tabindex plus arrow keys: one Tab stop for the strip, and the six
   * reachable from the keyboard alone (`B8.27`). Home and End are there
   * because a strip of six with no way back to the first is a strip that
   * punishes the keyboard for being a keyboard. */

  /* The six buttons, kept by id, so a key press can put DOM FOCUS on the one
   * it just selected. Moving `aria-selected` and `tabindex` without moving
   * focus is the defect the round-2 walkthrough physically drove out: the
   * panel changed under every arrow press while `document.activeElement` sat
   * on the tab the person started from, an element that by its own attributes
   * was no longer selected and no longer a tab stop. A screen reader follows
   * focus and a keyboard user follows the focus ring, so both were told the
   * wrong stage was open. Roving tabindex is only half a pattern without
   * this: the one tab with `tabindex=0` must also be the one holding focus. */
  const tabs = React.useRef({})
  const keep = (id) => (el) => {
    if (el) tabs.current[id] = el
    else delete tabs.current[id]
  }

  const onKeyDown = (e) => {
    const i = stages.findIndex((s) => s.id === active)
    const last = stages.length - 1
    const to = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? Math.min(i + 1, last)
      : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? Math.max(i - 1, 0)
        : e.key === 'Home' ? 0
          : e.key === 'End' ? last
            : -1
    if (to < 0 || to === i) return
    e.preventDefault()
    const next = stages[to].id
    onGo(next)
    /* Focus moves with the selection, in the same handler, on the same node
     * React will re-render as the active tab (the buttons are keyed by stage
     * id, so the element survives the render). Focusing it before the render
     * is safe: `focus()` does not care about `tabindex=-1`, and the render
     * that follows flips it to `0` under the focus that is already there. */
    const el = tabs.current[next]
    if (el && typeof el.focus === 'function') el.focus()
  }
  return <nav className="ws-change-stages" role="tablist" aria-label={C.stages.strip_label} onKeyDown={onKeyDown}>
    {stages.map((s) => <button
      key={s.id}
      ref={keep(s.id)}
      type="button"
      role="tab"
      id={'ws-change-tab-' + s.id}
      aria-selected={active === s.id}
      aria-controls={'ws-change-panel-' + s.id}
      tabIndex={active === s.id ? 0 : -1}
      className={'ws-change-stage' + (active === s.id ? ' ws-change-stage-on' : '') + (s.filled ? '' : ' ws-change-stage-thin')}
      onClick={() => onGo(s.id)}
    >
      <span className="ws-change-stage-name">{s.name}</span>
      <span className="ws-change-stage-question">{s.question}</span>
    </button>)}
  </nav>
}

/** What would put something in a stage that holds nothing (03-USER-LENS.md rule 12). */
/* The absence-of-problem sentences, each with its scope in it.
 *
 * `T-2-8-F5`: these two read as an all-clear on a Change where nothing has
 * been checked — four lines under the page's own "None has evidence yet". The
 * scope goes in the sentence, not in the heading above it, because Sam reads
 * from the middle where the headings are. `absenceWithoutItsScope` in
 * `change-view.js` is the rule these two satisfy, and it is driven over the
 * whole rendered screen rather than over these two call sites.
 */
/* The sentence itself is `change-model.js` `linkedWorkLine`, shared with the
 * page: the unrecorded-work sentence where the record does not hold what the
 * agent said (UNRECORDED-WORK-NOT-SILENT), otherwise `change-view.js`
 * `nothingWrongLine`, which counts the pieces of work actually read (PW-F13). */

function traceNoneLine(model, folded) {
  const unchecked = nothingCheckedYet(model)
  if (folded) return unchecked ? C.trace.none_shown_unchecked : C.trace.none_shown
  return unchecked ? C.trace.none_shown_nothing_folded_unchecked : C.trace.none_shown_nothing_folded
}

function Empty({ text }) {
  return <p className="ws-change-empty">{text}</p>
}

/* ── the six views ────────────────────────────────────────────────────── */

function Define({ model }) {
  const accepted = model.accepted
  const proposals = model.proposals
  const action = accepted.criteria.length
    ? <Blocked id="ws-change-why-define" label={C.actions.accept_criteria} pair={HQ_COPY.change.blocked.already_accepted} primary />
    : proposals.length
      /* T-2-3 owns the accepting act and serves it at
       * `POST /api/changes/<id>/scope/accept`. This card is read-only, so the
       * control is here, primary, and OFF, with the honest reason that nothing
       * on this screen turns it on. A button that copied text while calling
       * itself "Accept" would be the kind of lie this product exists to find.
       *
       * T-2-10, finding 2: off is not the same as nowhere to go. The reason
       * now carries the act that DOES write the acceptance on this build —
       * `thruline change accept`, with this Change's own name in it, so the
       * command reads as something to run rather than something to look up. */
      ? <Blocked id="ws-change-why-define" label={C.actions.accept_criteria} pair={HQ_COPY.change.blocked.accept_not_wired} values={{ change: model.id || '' }} primary />
      : <Blocked id="ws-change-why-define" label={C.actions.accept_criteria} pair={HQ_COPY.change.blocked.nothing_to_accept} primary />
  return <>
    <h2>{C.define.request_heading}</h2>
    <p className="ws-change-request">{model.request.purpose || model.request.title}</p>
    {model.request.requester ? <p className="ws-change-muted">{fill(C.define.requester, { who: model.request.requester })}</p> : null}
    {accepted.criteria.length ? <>
      <h3>{C.define.accepted_heading}</h3>
      <ol className="ws-change-criteria">{accepted.criteria.map((t, i) => <li key={i}>{t}</li>)}</ol>
      <p className="ws-change-muted">{fill(C.define.accepted_note, { list: accepted.version == null ? 0 : accepted.version })}</p>
      {/* QUAL-F2-R2: accepted after the work was done — said only when the
        * record shows it, never an earlier agreement it does not hold. */}
      {model.retrospective ? <p className="ws-change-muted">{fill(C.define.accepted_after_work, { date: model.retrospective.acceptedOn })}</p> : null}
    </> : proposals.length ? <>
      <h3>{C.define.proposed_heading}</h3>
      <p className="ws-change-muted">{C.define.proposed_note}</p>
      <ol className="ws-change-criteria">{proposals.map((p, i) => <li key={i}>{p.text}</li>)}</ol>
    </> : <Empty text={C.stages.define.empty} />}
    <Covers covers={model.covers} />
    {action}
  </>
}

/* What this Change covers (T-3-2b).
 *
 * The folders, in the record's own spelling — the record speaking, not this
 * product. Accepted folders are what the scope check uses; proposed ones are
 * marked as not yet accepted, and neither is ever shown as a wildcard. A
 * Change that covers nothing says so, because "nothing is counted for this
 * Change" is otherwise a silence. */
function Covers({ covers }) {
  const c = covers || { paths: [], accepted: false }
  return <section className="ws-change-covers">
    <h3>{C.define.covers_heading}</h3>
    {c.paths.length
      ? <>
        <ul className="ws-change-covers-list">{c.paths.map((p, i) => <li key={i}><code>{p}</code></li>)}</ul>
        <p className="ws-change-muted">{c.accepted ? C.define.covers_accepted_note : C.define.covers_proposed_note}</p>
      </>
      : <Empty text={C.define.covers_none} />}
  </section>
}

/* What exactly will be examined (T-3-2).
 *
 * The sentences come first and they are the whole answer in Sam's words: the
 * folder, the starting point, whether the unsaved edits on this disk are
 * included, which tools will run and which version each is, and which running
 * copy of the app this Change is about. The ids, digests and pids are one
 * click deeper, under a summary a person has to open.
 *
 * A Change with nothing written down yet is not an empty box: it says what
 * would put something in it, which is the one act that would. */
function Prepare({ model, onGo }) {
  const examined = model.examined || { recorded: false, lines: [], technical: [], records: null, note: C.prepare.isolation_note }
  const p = model.prepare
  const bound = p && (p.repository || p.base)
  return <>
    <h2>{C.prepare.heading}</h2>
    {examined.lines.length
      ? <ul className="ws-prepare-lines">{examined.lines.map((line, i) => <li key={line.slug + i}>{line.text}</li>)}</ul>
      : <Empty text={C.stages.prepare.empty} />}
    {bound
      ? <dl className="ws-change-facts">{[
        [C.prepare.repository, p.repository],
        [C.prepare.base, p.base],
        [C.prepare.snapshot, p.snapshot],
        [C.prepare.workspace, p.workspace],
      ].map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v || C.prepare.none_recorded}</dd></div>)}</dl>
      : null}
    {examined.technical.length
      ? <details className="ws-prepare-technical">
        <summary>{C.prepare.technical_heading}</summary>
        <dl className="ws-change-facts">{examined.technical.map((row) => <div key={row.label}><dt>{row.label}</dt><dd>{row.value}</dd></div>)}</dl>
        {examined.records ? <p className="ws-change-muted">{examined.records}</p> : null}
      </details>
      : null}
    <p className="ws-change-muted">{examined.note || C.prepare.isolation_note}</p>
    <button type="button" className="ws-change-primary" onClick={() => onGo('build')}>{C.actions.see_what_changed}</button>
  </>
}

function Build({ model, onGo }) {
  return <>
    <h2>{C.build.heading}</h2>
    {model.build.length
      ? <ul className="ws-change-diff">{model.build.map((d, i) => <li key={i}>{d}</li>)}</ul>
      : <Empty text={C.build.none} />}
    <button type="button" className="ws-change-primary" onClick={() => onGo('verify')}>{C.actions.see_what_we_checked}</button>
  </>
}

/* ── the one derived view-model, rendered (T-S-3) ──────────────────────── */

/**
 * One accepted line, as the model decided it.
 *
 * Every word here is `model.hq.criteria[i]`: the state word, the
 * `Proven: … · version … · scope` line, and the freshness sentence. This
 * component computes NOTHING — it cannot, because a second derivation of a
 * state word on a screen is the defect class the card exists to close. The
 * reason paths are inside the Technical drawer, one level down, where paths
 * are allowed to be paths.
 */
export function CriterionLine({ criterion }) {
  const f = criterion.freshness
  /* P6-4: the anchor an Ask answer's door opens on. `ask-hq.js` builds the
   * same id from the criterion's own number and nothing else. */
  return <li className="ws-cov-line" id={'ws-cov-line-' + criterion.n} data-state={criterion.state}>
    <p className="ws-cov-line-text">{criterion.text}</p>
    <p className="ws-cov-line-state">{criterion.line}</p>
    {f ? <p className={'ws-cov-line-fresh' + (f.fresh ? '' : ' ws-cov-line-stale')}>{f.sentence}</p> : null}
    {f && f.reason ? <p className="ws-change-muted">{f.reason}</p> : null}
    {/* T9 — every line with a freshness record has a Technical drawer, the
      * still-applies case included: it names the one rule, then what that rule
      * compared (carried forward) or, on a stale line, why and its class. */}
    {f
      ? <details className="ws-change-technical">
        <summary>{HQ_COPY.change.model.inspector.technical}</summary>
        {f.rule ? <p>{f.rule}</p> : null}
        {f.fresh
          ? (f.carriedTechnical ? <p>{f.carriedTechnical}</p> : null)
          : <>
            <p>{f.reasonTechnical || f.technical}</p>
            {f.reasonClass ? <p><code>{f.reasonClass}</code></p> : null}
            {f.paths.length ? <ul>{f.paths.map((p) => <li key={p}><code>{p}</code></li>)}</ul> : null}
          </>}
      </details>
      : null}
  </li>
}

/**
 * The Inspector: the story of one thing, in the design record's four blocks
 * (§3.4), in that order, every time.
 *
 * Block 2 is the existing evidence and finding cards — the record showing what
 * it holds — with the criterion's freshness sentence beside them. Block 4 is
 * EXACTLY ONE primary button, chosen by state, and while every state this
 * build can reach leaves it off, it carries both halves of why (T-2-10).
 */
/**
 * T-3-5 (`B7.6`-`B7.12`, `Q1`, lens rule 5) — is this ready, and what does
 * that rest on?
 *
 * Read the two shapes this renders, because they are the whole rule:
 *
 *  - READY. One sentence naming the version, then THE LIST of what it rests
 *    on, immediately under it. The sentence ends in a colon and the list
 *    follows it in the same block: there is no arrangement of this component
 *    that shows the claim without what it stands on.
 *  - NOT READY. The count, then each line that still needs evidence with the
 *    record's own reason beside it. Missing evidence reads as missing.
 *
 * There is no badge, no colour, no tick and no percentage. A waived or
 * not-applicable line is printed with the person, the authority and the
 * reason, in the not-ready list's own shape, and never in the rests-on list —
 * the engine builds that from satisfied obligations only.
 */
export function Readiness({ readiness }) {
  /* No answer is not an answer: with no readiness block on the document this
   * says nothing, rather than printing a sentence about a record it could not
   * read. The door stays shut either way. */
  if (!readiness || readiness.absent || !readiness.sentence) return null
  /* P6-4: the anchor "Why can't I approve?" opens on. */
  return <section className="ws-change-readiness" id="ws-change-readiness" aria-label={readiness.heading}>
    <p className={readiness.ready ? 'ws-readiness-sentence ws-readiness-ready' : 'ws-readiness-sentence'}>{readiness.sentence}</p>
    {readiness.ready
      ? <ul className="ws-readiness-rests-on">
        {list(readiness.restsOn).map((r) => <li key={r.id}>{r.line}</li>)}
      </ul>
      : shownLines(readiness).length
        ? <ul className="ws-readiness-missing">
          {/* HTT-TEXT-RUN-TOGETHER — the line and its reason are two inline
            * spans; with nothing between them they read as one run ("…shows
            * zero.No test ran for this."). The same dash the whole-story page
            * puts between them. */}
          {shownLines(readiness).map((m, at) => <li key={m.id || 'at' + at}>
            {m.text ? <span className="ws-readiness-what">{m.text}</span> : null}
            {m.text && m.reason ? ' — ' : null}
            <span className="ws-readiness-why">{m.reason}</span>
          </li>)}
        </ul>
        : null}
    {/* BRIEF-COUNT-AGREES-R2 — what is missing that is not one of the lines
      * (the identity, a review), on its own: it is never counted among "the
      * things that count as done", so it is not in the list above and not in
      * its count. */}
    {readiness.ready ? null : list(readiness.also).map((w, i) => <p key={'also' + i} className="ws-readiness-also">{w}</p>)}
    {/* T-5-3 (`B7.18`, `B7.20`) — the set-aside lines sit UNDER their own
      * heading, below what the answer rests on and never inside it. A person
      * reading the list of what this rests on must not find, in the same
      * list, a line nobody checked. */}
    {list(readiness.waivedLines).length
      ? <p className="ws-readiness-set-aside-heading">{readiness.setAsideHeading}</p>
      : null}
    {list(readiness.waivedLines).length
      ? <ul className="ws-readiness-waived">
        {list(readiness.waivedLines).map((d) => <li key={d.id}>
          {d.text ? <span className="ws-readiness-what">{d.text}</span> : null}
          {d.text && d.waiver ? ' — ' : null}
          <span className="ws-readiness-why">{d.waiver}</span>
        </li>)}
      </ul>
      : null}
  </section>
}

/**
 * STEP-7-LIMITS (PW-F12) — what the record cannot say about the code, from the
 * one derived field (`change-model.js` `codeLimitOf`). This renders it and
 * chooses nothing; it reads nothing from git. STEP-7-GIT-LINE: when the
 * served Change says the code moved, that line is first in the same field,
 * and its receipt is rendered one level down.
 */
export function CodeLimit({ limit, receipt = true }) {
  const lines = limit && Array.isArray(limit.lines) ? limit.lines : []
  const moved = limit && limit.moved ? limit.moved : null
  const out = lines.map((line, n) => <p key={n} className="ws-change-muted ws-change-code-limit">{line}</p>)
  /* STEP-7-GIT-LINE — the receipt, one level down: the commit recorded with
   * the version, the commit the folder was at when this page was read, and
   * that edits not yet committed were not compared. Only where the move is
   * said at the top; beside Approve the line alone is enough. */
  if (moved && receipt) {
    out.push(<details key="receipt" className="ws-change-technical ws-change-code-receipt">
      <summary>{moved.heading}</summary>
      <p>{moved.receipt}</p>
      <p>{moved.uncommitted}</p>
    </details>)
  }
  return out
}

export function Inspector({ hq, cards, onCopy, onDecided, onRecheck, onDoor, onLink, licence }) {
  /* PATH-UI-2-R3 — two kinds of door start here. `onDoor` looks at a line (what
   * broke); `onLink` goes to the control that links a run to it. Without an
   * `onLink` the second is the first, as it was. */
  const linkDoor = onLink || onDoor
  const footer = React.useRef(null)
  /* A3-F4 (WCAG 2.4.11): the page keeps a focused item clear of the sticky
   * footer by its real height, read off the footer each render. */
  React.useEffect(() => {
    const el = footer.current
    const root = typeof document === 'undefined' ? null : document.documentElement
    if (!el || !root || !root.style) return undefined
    root.style.setProperty('--inspector-footer-h', Math.ceil(el.getBoundingClientRect().height || 0) + 'px')
    return () => root.style.removeProperty('--inspector-footer-h')
  })
  if (!hq) return null
  const i = hq.inspector
  const I = HQ_COPY.change.model.inspector
  const foldAfter = typeof i.shows.foldAfter === 'number' ? i.shows.foldAfter : INSPECTOR_FOLD_AFTER
  const cardFold = foldList(cards, foldAfter)
  const itemFold = foldList(i.shows.items, foldAfter)
  const inspectorItem = (it) => <li key={it.id}>
    <span className="ws-inspector-item-name">{it.name || it.evidenceId}</span>
    <FailureBlock item={it} />
  </li>
  /* T-6-4: each of the four blocks is a region named by its own heading, so a
   * screen reader can move block to block; the whole is named for what it is. */
  return <section className="ws-change-inspector" role="region" aria-label={HQ_COPY.a11y.inspector_label}>
    <section className="ws-inspector-block" role="region" aria-labelledby="ws-inspector-h-what">
      <h3 id="ws-inspector-h-what">{i.what.heading}</h3>
      {i.what.quote ? <blockquote className="ws-inspector-quote">{i.what.quote}</blockquote> : null}
      <p className="ws-inspector-summary">{i.what.summary}</p>
    </section>

    <section className="ws-inspector-block" role="region" aria-labelledby="ws-inspector-h-shows">
      <h3 id="ws-inspector-h-shows">{i.shows.heading}</h3>
      {i.shows.line ? <p className="ws-cov-line-state">{i.shows.line}</p> : null}
      {i.shows.freshness ? <p className="ws-cov-line-fresh">{i.shows.freshness.sentence}</p> : null}
      {/* A3-F12 — never longer than about two viewports: the leading card in
        * full, the rest behind one disclosure that says how many it holds.
        * Folded, not removed; opening it draws every card. */}
      {list(cards).length
        ? <div className="ws-change-cards">{cardFold.shown.map((card, n) => <Card
          key={card.sessionId + ':' + card.id}
          card={card}
          primary={n === 0}
          register="manager"
          receipts="off"
          onCopy={onCopy}
        />)}</div>
        : null}
      {cardFold.folded.length
        ? <details className="ws-inspector-fold">
          <summary>{cardFold.folded.length === 1 ? I.fold_cards_one : fill(I.fold_cards_many, { n: cardFold.folded.length })}</summary>
          <div className="ws-change-cards">{cardFold.folded.map((card) => <Card
            key={card.sessionId + ':' + card.id}
            card={card}
            primary={false}
            register="manager"
            receipts="off"
            onCopy={onCopy}
          />)}</div>
        </details>
        : null}
      {/* T-3-6-F6-BRIEF (`Q2`, `B16.10`). WHAT BROKE IS HERE, on the screen a
        * fresh install opens. F3 attached the failure's own words and the
        * evidence-class sentence to the item derivation and rendered them on
        * the Verify tree; P6 then made Brief the component the Change route
        * renders for every stage segment, and this block listed the same items
        * by name alone — so "Where this finding came from" reached only the
        * packet, and a seeded demonstration read as a real regression on the
        * primary surface. The same `FailureBlock`, the same `item.failure`
        * fields, no second copy of a string. The list is shown whether or not
        * there are finding cards above it: a name with nothing under it was
        * the whole defect. */}
      {/* RUN-LINK-INSPECTOR — the line's own unlinked-run sentence, the one its
        * row prints, where "Nothing in the record bears on this line yet."
        * stood while a passing run sat in the record linked to nothing. */}
      {i.shows.unlinked ? <p className="ws-inspector-unlinked">{i.shows.unlinked.sentence}</p> : null}
      {list(i.shows.items).length
        ? <ul className="ws-inspector-items">{itemFold.shown.map(inspectorItem)}</ul>
        : list(cards).length || i.shows.unlinked ? null : <Empty text={i.shows.empty} />}
      {itemFold.folded.length
        ? <details className="ws-inspector-fold">
          <summary>{itemFold.folded.length === 1 ? I.fold_items_one : fill(I.fold_items_many, { n: itemFold.folded.length })}</summary>
          <ul className="ws-inspector-items">{itemFold.folded.map(inspectorItem)}</ul>
        </details>
        : null}
    </section>

    <section className="ws-inspector-block" role="region" aria-labelledby="ws-inspector-h-settle">
      <h3 id="ws-inspector-h-settle">{i.settle.heading}</h3>
      <p className="ws-inspector-settle">{i.settle.sentence}</p>
    </section>

    {/* The one decision, named so the criteria node's primary can be a door to
      * it rather than a second copy of it (P6-DESIGN-FIXES-13). */}
    {/* A3-F4: the action block is the Inspector's sticky footer, so the one
      * primary is on the first screen however many cards are above it. */}
    <section ref={footer} className="ws-inspector-block ws-inspector-footer" id={DECISION_BLOCK} role="region" aria-labelledby="ws-inspector-h-action">
      <h3 id="ws-inspector-h-action">{i.action.heading}</h3>
      {/* Exactly one primary. In the `proven` state that one is the DECISION
        * (T-S-4) and it is live; in every other state it is the off control
        * with both halves of why, because a disabled control that does not say
        * what would turn it on is the paper cut T-2-10 closed. */}
      {/* T-3-5: the Approve door opens on READINESS and on nothing else. A
        * withdrawal stays reachable whatever readiness says — taking an
        * approval back is never gated on the record being in good order. */}
      {showsDecision(hq)
        ? <Decision decision={hq.decision} changeId={hq.id} onDecided={onDecided} onRecheck={onRecheck} licence={licence} />
        /* RUN-LINK-INSPECTOR — after a passing run no line is linked to, the
          * one primary is the run sheet's own door to that line's link
          * control: same words, same `onDoor`, same target. */
        : inspectorDoor(hq)
          ? <WhatBrokeDoor action={{ label: i.action.link.label, door: i.action.link.n }} onDoor={linkDoor} />
          /* P6-5 — "Run the check" is an act this build CAN do now, so in the
            * three states whose one primary is that act, the button is the run
            * sheet's door rather than an off control explaining that ThruLine
            * cannot start it. Label and state are still the model's. */
          : runsFromHere(hq)
            ? <RunDoor changeId={hq.id} version={hq.version} unlinked={hq.readiness ? hq.readiness.unlinked : null} onDoor={linkDoor} label={i.action.label} primary onCancel={stopRun(hq.id)} onRan={onDecided} />
            /* A3-F4 — the model's live door ("Look at what broke") is drawn
              * live: it opens the line that broke. It reads; it writes nothing.
              * A3-F10 — it is the one door component, `WhatBrokeDoor`, the same
              * button the criteria summary draws; here it is handed the
              * Inspector's `onDoor`, so from Brief and from Verify it goes to
              * the line that broke. */
            : i.action.enabled && i.action.door != null
              ? <WhatBrokeDoor action={i.action} onDoor={onDoor} />
              : <Blocked id="ws-inspector-why" label={i.action.offLabel || i.action.label} pair={{ reason: i.action.reason, enabler: i.action.enabler }} primary />}
      {/* STEP-7-LIMITS — beside Approve, on or off: what is approved is the
        * version last recorded, and a later commit has not been looked at. */}
      <CodeLimit limit={hq.codeLimit} receipt={false} />
      <p className="ws-change-muted ws-inspector-numbers">{HQ_COPY.change.model.inspector.numbers}</p>
    </section>
  </section>
}

/** A3-F4 — where the door to one line lands: the Verify stage, on that line. */
export const criterionDoor = (n) => ({ stage: 'verify', anchor: EVIDENCE_ANCHORS.criterion(n) })

/* PATH-UI-2-R3 — where a door to a line's link control ends, and why it is not
 * here any more.
 *
 * R2's `landOnLine` did the landing on the document: scroll the line's summary
 * entry into view, then `focus()` the name box. The two live in different
 * lists about three thousand pixels apart, and the browser's own scroll for
 * `focus()` put the view on the name box, so from the fifth line of a Change
 * up, the named line's control was below the fold and door 12 landed where door
 * 1 did. The landing is now the coverage section's own (`CriterionCoverage`,
 * `landOnControls`): the line a door names is pinned first in its list, marked
 * current and named beside the name box, and the view is placed once, on the
 * name box, with the row under it. This file's part is only to carry the
 * request there: `ChangeShell` holds it, moves to the Verify stage the way
 * every door to evidence does, and spends it once the stage is on the glass.
 */

/** Take a spent door's request off the address, so a reload does not repeat it. A sandboxed document that refuses address writes keeps it, harmlessly. */
function spendDoorAddress(win) {
  try {
    if (!win || !win.history || typeof win.history.replaceState !== 'function') return
    const next = searchWithoutDoor(win.location.search)
    if (next === win.location.search) return
    win.history.replaceState(win.history.state, '', win.location.pathname + next + win.location.hash)
  } catch (_refused) {
    /* nothing else to do */
  }
}

/**
 * T5 (report F13) — the message for the agent, from the Change's own record.
 *
 * `tell` is `tellDraftOf(model.hq)`, composed at render from the documents
 * the last read returned; nothing here stores it. The draft is shown in full
 * before it is copied, because a person pastes it into their agent and should
 * read what they paste. Copy only: nothing is sent, and the screen says so.
 */
export function tellOf(hq) {
  /* A sentence whose slot the record cannot fill is not rendered with a hole
   * (`tell-draft.js` throws); the screen says which value was missing and
   * offers no message, rather than failing the whole Change screen. */
  try {
    return tellDraftOf(hq)
  } catch (e) {
    if (!(e instanceof SlotError)) throw e
    return { kind: 'unbuilt', draft: null, covered: fill(HQ_COPY.change.tell.unbuilt, { slot: e.slot }), lines: [] }
  }
}

export function TellAgent({ tell, onCopy }) {
  if (!tell) return null
  const T = HQ_COPY.change.tell
  return <section className="ws-tell" aria-label={T.heading} data-kind={tell.kind}>
    <h3 className="ws-tell-heading">{T.heading}</h3>
    {tell.draft
      ? <>
        <p className="ws-change-muted">{T.lead}</p>
        <pre className="ws-tell-draft">{tell.draft}</pre>
        <button type="button" className="ws-change-secondary" onClick={() => onCopy && onCopy(tell.draft, 'prompt')}>{C.actions.copy_prompt}</button>
        <p className="ws-change-muted ws-tell-not-sent">{T.not_sent}</p>
        {/* BUILD-16-R2: how the record was masked is the person's to read,
          * beside the copy control; it is not part of what is copied. */}
        {tell.masking ? <p className="ws-change-muted ws-tell-masking">{tell.masking}</p> : null}
      </>
      : <p className="ws-change-muted">{tell.covered}</p>}
  </section>
}

/**
 * T6 (report line 155) — after a line was raised: the ladder, with each rung
 * the record can speak to drawn from typed fields and the one it cannot
 * drawn off, with its reason.
 *
 *  - Told the agent: OFF. ThruLine does not record that the agent was told,
 *    and a press of a copy button is not that record.
 *  - What changed since: the version sentence only; which files changed since
 *    the raise is not in the record.
 *  - What the check says now: one sentence per accepted line, of three kinds,
 *    from `hq.repairs` — the rows the packet page and the file also carry.
 *
 * Read only: no control, no colour carrying the kind, no receipt on an absence.
 */
export function RepairHistory({ hq }) {
  const rows = list(hq && hq.repairs)
  if (!rows.length) return null
  const R = HQ_COPY.change.repairs
  return <section className="ws-repairs" aria-label={R.heading}>
    <h3 className="ws-repairs-heading">{R.heading}</h3>
    <ol className="ws-repairs-ladder">
      <li className="ws-repairs-rung ws-repairs-rung-off" data-off="true">
        <span className="ws-repairs-rung-name">{R.told}</span>{' '}
        <span className="ws-change-muted ws-repairs-off-why">{R.told_off}</span>
      </li>
      <li className="ws-repairs-rung">
        <span className="ws-repairs-rung-name">{R.changed}</span>{' '}
        <span>{hq.version.text}</span>
      </li>
      <li className="ws-repairs-rung">
        <span className="ws-repairs-rung-name">{R.reran}</span>
        <ul className="ws-repairs-rows">{rows.map((r) => <li key={r.n} data-kind={r.kind}>
          <span className="ws-repairs-what">{r.text}</span>{' '}
          <span className="ws-repairs-sentence">{r.sentence}</span>
        </li>)}</ul>
      </li>
    </ol>
    <p className="ws-change-muted">{R.closing}</p>
  </section>
}

/* ── the runs that need an answer (T-3-4) ─────────────────────────────── */

/** P6-5 — the states whose one primary act is running the check. */
const RUN_STATES = ['needs_recheck', 'recorded', 'in_progress']

/** Is the Inspector's one primary the Decision (approve, or take it back)?
 * The one rule lives in `change-model.js` (RUN-LINK-TRUTH-R3) so "What we
 * checked" reads the same function; it is re-exported here for its callers. */
export { showsDecision }

/** Is the Inspector's one primary the run sheet's door? Only when its state
 * asks for a run AND the Decision is not what the Inspector is showing — a
 * Change can read "needs a re-check" and be ready to approve at once, and
 * then the Inspector shows Approve, so the Brief must carry the door. */
export const runsFromHere = (hq) => Boolean(hq && hq.id && RUN_STATES.includes(hq.state) && !showsDecision(hq) && !inspectorDoor(hq))

/** RUN-LINK-INSPECTOR — is the Inspector's one primary the door to the line
 * where a passing run nobody linked is linked? Then it is not the run sheet's
 * opener, and the story carries that as a secondary. */
const inspectorDoor = (hq) => Boolean(hq && hq.inspector && hq.inspector.action && hq.inspector.action.link)

/** The one address this screen stops a check at, and there is only one. */
export const CHANGE_CANCEL_RUN = (id, run) =>
  '/api/changes/' + encodeURIComponent(id) + '/runs/' + encodeURIComponent(run) + '/cancel'

/** P6-5 — the same stop, handed to the run sheet's Cancel, so the address is
 * declared once, here, beside the control that first used it. */
export const stopRun = (changeId) => (run, actor) => writeAction(CHANGE_CANCEL_RUN(changeId, run), { actor })

/**
 * T-3-4 (`B7.1`, `B7.6`, `B12.6`, `Q12`) — the runs a person has to answer:
 * the ones still going, and the ones that were cut short.
 *
 * Two sentences per card, never one badge (design §3.8): what became of the
 * run, then what it settles. **No state word appears here at all** — a check
 * that did not reach the end observed nothing to be right or wrong about, and
 * the model sets `word` to null for every one of them.
 *
 * Exactly one primary control per card, and the model chooses which: a run
 * still going can be stopped, a run that was stopped can be run again. There
 * is no "resume", because a re-run is a NEW examination with a new
 * examined-inputs record and nothing about the old run is edited (Rule 13).
 *
 * Stopping needs a name, for the same reason approving does: the record says
 * who asked, and an unattributed act is one the store refuses.
 */
export function RunCards({ hq, changeId, onCopy, onReload }) {
  const [actor, setActor] = React.useState('')
  const [busy, setBusy] = React.useState(null)
  const [refusal, setRefusal] = React.useState(null)
  const X = HQ_COPY.change.model.execution
  const D = HQ_COPY.change.model.decision
  const runs = list(hq && hq.actionableRuns)
  if (!runs.length) return null
  const named = actor.trim().length > 0
  const stop = async (run) => {
    setBusy(run)
    setRefusal(null)
    const answer = await writeAction(CHANGE_CANCEL_RUN(changeId, run), { actor })
    setBusy(null)
    if (answer.status !== 'ready') { setRefusal(answer.refusal); return }
    if (onReload) onReload()
  }
  return <section className="ws-change-runs" aria-label={hq.executionHeading}>
    <h3 className="ws-change-runs-heading">{hq.executionHeading}</h3>
    <ul className="ws-change-runs-list">
      {runs.map((run) => <li key={run.runKey} className={'ws-change-run ' + (run.running ? 'ws-change-run-going' : 'ws-change-run-stopped')}>
        <p className="ws-change-run-name">{run.name}</p>
        {/* The execution half. */}
        <p className="ws-change-run-execution">{run.executionLine}</p>
        {/* The observed half, its own sentence. */}
        {run.observedLine ? <p className="ws-change-run-observed">{run.observedLine}</p> : null}
        {/* `B12.6`: what was verified about what it left behind. */}
        <p className="ws-change-run-cleanup">{run.cleanupLine}</p>
        {run.actionNote ? <p className="ws-change-muted">{run.actionNote}</p> : null}
        {/* P6-5: "Run it again" runs it again — the run sheet, not a
          * command to copy (design record §3.7, zero terminal). */}
        {run.act === 'rerun'
          ? <RunDoor changeId={changeId} version={hq.version} label={run.action} primary onCancel={stopRun(changeId)} onRan={onReload} />
          : named
            ? <button type="button" className="ws-change-primary" disabled={busy === run.runKey} onClick={() => stop(run.runKey)}>{busy === run.runKey ? D.working : run.action}</button>
            : <Blocked id={'ws-run-why-' + run.runKey} label={run.action} pair={{ reason: D.actor_missing, enabler: D.actor_enabler }} primary />}
      </li>)}
    </ul>
    {runs.some((r) => r.act === 'cancel')
      ? <label className="ws-cov-actor">
        <span>{D.actor_label}</span>
        <input type="text" value={actor} onChange={(e) => setActor(e.target.value)} />
      </label>
      : null}
    {refusal ? <p className="ws-change-notes" role="alert">{refusal}</p> : null}
  </section>
}

/* ── the decision (T-S-4) ─────────────────────────────────────────────── */

/** Where the packet lives. One place, so the two doors cannot drift apart. */
export const packetHref = (id) => '#/changes/' + encodeURIComponent(id) + '/packet'

/** The two addresses this screen writes a decision to, and there are only two. */
export const CHANGE_DECIDE = (id) => '/api/changes/' + encodeURIComponent(id) + '/decisions'
export const CHANGE_WITHDRAW = (id) => '/api/changes/' + encodeURIComponent(id) + '/decisions/withdraw'

/**
 * Approve exactly one version, or take that approval back.
 *
 * Every word comes from `HQ_COPY.change.model.decision`, and every state word,
 * version and headline comes from `change-model.js` — this component chooses
 * nothing. What it does own is the one sentence that has to be beside the
 * button whenever approval is on offer or has been given: *it does not merge,
 * deploy or tell the agent anything.* That sentence is the reason a person
 * clicks, and it is rendered in both states rather than in a help page.
 *
 * A refused act is SHOWN, not swallowed: when the code moved while the screen
 * was open the server answers with what changed, and the files are listed here
 * under the refusal.
 */
export function Decision({ decision, changeId, onDecided, onRecheck, licence }) {
  const [actor, setActor] = React.useState('')
  const [busy, setBusy] = React.useState(false)
  const [moved, setMoved] = React.useState(null)
  const [refusal, setRefusal] = React.useState(null)
  /* A3-F8a: Approve opens a confirming sheet; only the sheet records. */
  const [confirming, setConfirming] = React.useState(false)
  const approveRef = React.useRef(null)
  const wasConfirming = React.useRef(false)
  React.useEffect(() => {
    if (confirming) { wasConfirming.current = true; return }
    if (!wasConfirming.current) return
    wasConfirming.current = false
    if (approveRef.current && approveRef.current.isConnected && typeof approveRef.current.focus === 'function') approveRef.current.focus()
  }, [confirming])
  const D = HQ_COPY.change.model.decision
  const act = async (url) => {
    setBusy(true)
    setMoved(null)
    setRefusal(null)
    const answer = await writeAction(url, { actor, seen_version: (url === CHANGE_WITHDRAW(changeId) ? decision.approvedFull : decision.seen) || '' })
    setBusy(false)
    if (answer.status !== 'ready') { setRefusal(answer.refusal); return }
    const acted = answer.doc && answer.doc.acted
    if (acted && acted.outcome === 'moved') { setMoved(movedSentences(acted)); return }
    if (onDecided) onDecided()
  }
  const named = actor.trim().length > 0
  /* T-DIST-4 — approving is the one paid act. With no licence that works it is
   * off, saying why and what turns it on, with the door to the licence page.
   * Withdrawing is never gated. No licence read at all is `unknown`, never a
   * pass: the button stays off and says it could not find out. */
  const licenceRead = licence || licenceView(null)
  const entitled = licenceRead.approve
  /* The dist merge: the chord below opens the same sheet the button does, so
   * it is off whenever the button is — the licence gate included. */
  const approvable = !moved && !decision.canWithdraw && entitled.allowed && named && !busy
  /* ⌘↵ / Ctrl+↵ from inside the decision (the name box included) OPENS the
   * sheet; it never records. The sheet stops every key it sees, so the same
   * chord pressed inside it confirms there and does not arrive here. */
  const openKey = (e) => {
    if (confirming || !approvable || e.key !== 'Enter' || !(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return
    e.preventDefault()
    setConfirming(true)
  }
  const confirm = () => {
    setConfirming(false)
    act(CHANGE_DECIDE(changeId))
  }
  /* `data-no-key-primary`: no key ever presses a live control in here
   * (`altitude.js` `pressPrimary`); `.` says this sentence instead. */
  return <div className="ws-decision" data-no-key-primary={HQ_COPY.shell.notes.approve_by_hand} onKeyDown={openKey}>
    {decision.approved
      ? <>
        <p className="ws-decision-headline">{decision.headline}</p>
        {/* T-5-2: which version the approval names, and which one the record
          * stands at now — beside the headline, not only after a refusal. */}
        {decision.movedLine ? <p className="ws-change-changed-since">{decision.movedLine}</p> : null}
        {/* T7 — the list it was given against, when a later one is accepted. */}
        {decision.listMovedLine ? <p className="ws-change-changed-since">{decision.listMovedLine}</p> : null}
        {decision.by ? <p className="ws-change-muted">{decision.by}</p> : null}
        {/* T-5-3 (`B7.20`, `B10.9`) — what this approval was given under,
          * beside the approval itself and for as long as it stands. Each line
          * names who set it aside, whose authority they had and why, and ends
          * by saying nobody checked it. */}
        {list(decision.exceptions).length
          ? <div className="ws-decision-exceptions">
            <p className="ws-decision-exceptions-heading">{decision.exceptionsHeading}</p>
            <ul>{list(decision.exceptions).map((e) => <li key={e.obligationId}>{e.line}</li>)}</ul>
          </div>
          : null}
      </>
      : <p className="ws-change-muted">{decision.none}</p>}
    {/* The sentence, in both states. */}
    <p className="ws-decision-not-an-action">{decision.sentence}</p>
    {/* T-S-5 (design §8 S2) — the packet's second real door, here because this
      * is where a person is asked to approve a version, and "what am I
      * approving" is answered by the thing they take away. */}
    <p className="ws-decision-packet"><a className="ws-change-secondary" href={packetHref(changeId)}>{HQ_COPY.change.packet.open_from_decision}</a></p>
    <label className="ws-cov-actor">
      <span>{D.actor_label}</span>
      <input type="text" value={actor} onChange={(e) => setActor(e.target.value)} />
    </label>
    {/* T-6-6 (rule 6): every way this control is off says why and what turns
      * it on. Withdraw used to be off with no name in the box and say nothing;
      * a control busy writing says it will come back by itself. */}
    {/* T-5-2 (`Q13`). Once the act has been refused because the code moved,
      * the one primary here is NOT a second press of Approve — the same press
      * would be refused for the same reason. It is the door to the lines that
      * were checked, which is where "what moved" is read. Approve comes back
      * when the screen has been re-read. */}
    {moved
      ? null
      : decision.canWithdraw
        ? named
          ? <button type="button" className="ws-change-secondary" disabled={busy} aria-describedby={busy ? 'ws-decision-busy' : undefined} onClick={() => act(CHANGE_WITHDRAW(changeId))}>{busy ? D.working : decision.withdrawLabel}</button>
          : <Blocked id="ws-decision-why" label={decision.withdrawLabel} pair={{ reason: D.withdraw_actor_missing, enabler: D.actor_enabler }} />
        : !entitled.allowed
          ? <>
            <Blocked id="ws-decision-why" label={decision.approveLabel} pair={{ reason: entitled.reason, enabler: entitled.enabler }} primary />
            <p className="ws-decision-licence"><a className="ws-change-secondary" href={LICENCE_HASH}>{HQ_COPY.licence.approve.link}</a></p>
            {/* PRICING-TRUTHS-IN-BINARY: shut for a licence reason, so the
              * plans page is one click away — a link the person presses, which
              * the browser opens; nothing is fetched to draw it, and
              * `noreferrer` sends the page nothing about where it came from. */}
            {entitled.plans
              ? <p className="ws-decision-licence"><a className="ws-change-secondary" href={PLANS_URL} target="_blank" rel="noopener noreferrer" aria-describedby="ws-decision-plans-opens">{HQ_COPY.licence.approve.plans}</a> <span id="ws-decision-plans-opens" className="ws-change-muted">{HQ_COPY.licence.approve.plans_opens}</span></p>
              : null}
          </>
          : named
            ? <button ref={approveRef} type="button" className="ws-change-primary" disabled={busy} aria-describedby={busy ? 'ws-decision-busy' : undefined} aria-haspopup="dialog" onClick={() => setConfirming(true)}>{busy ? D.working : decision.approveLabel}</button>
            : <Blocked id="ws-decision-why" label={decision.approveLabel} pair={{ reason: D.actor_missing, enabler: D.actor_enabler }} primary />}
    {busy ? <p className="ws-change-muted" id="ws-decision-busy" role="status">{D.working_why.reason} {D.working_why.enabler}</p> : null}
    {confirming && approvable
      ? <DecisionSheet version={decision.nowVersion} who={actor.trim()} sentence={decision.sentence} onConfirm={confirm} onClose={() => setConfirming(false)} />
      : null}
    {moved
      ? <MovedRefusal moved={moved} onRecheck={() => { setMoved(null); if (onRecheck) onRecheck(); else if (onDecided) onDecided() }} />
      : null}
    {refusal ? <p className="ws-change-notes" role="alert">{refusal}</p> : null}
    {decision.history.length
      ? <details className="ws-decision-history">
        <summary>{decision.heading}</summary>
        <ul>{decision.history.map((h, n) => <li key={n}>{h.line}</li>)}</ul>
      </details>
      : null}
  </div>
}

const SHEET_FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"])'

/**
 * The confirming act (A3-F8a). Approve opens this and records nothing; the
 * approval is recorded only by this sheet's own button or by ⌘↵ / Ctrl+↵
 * pressed inside it. Plain Enter, Space, `.` and every key of the shell's map
 * do nothing here: the sheet is modal and stops each key it sees, so nothing
 * behind it answers either. Esc, the scrim and "Not now" close it, record
 * nothing, and give focus back to Approve.
 *
 * Every line is the decision's own: the version the record stands at, the
 * name typed in the box, what gets written, and the sentence that says what
 * approving does not do.
 */
export function DecisionSheet({ version, who, sentence, onConfirm, onClose }) {
  const D = HQ_COPY.change.model.decision
  const C = D.confirm
  const box = React.useRef(null)
  React.useEffect(() => {
    if (box.current && typeof box.current.focus === 'function') box.current.focus()
  }, [])
  const onKey = (e) => {
    e.stopPropagation()
    if (e.key === 'Escape') { e.preventDefault(); onClose(); return }
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey) { e.preventDefault(); onConfirm(); return }
    if (e.key !== 'Tab' || !box.current) return
    const all = [...box.current.querySelectorAll(SHEET_FOCUSABLE)]
    if (!all.length) return
    const first = all[0]
    const last = all[all.length - 1]
    const at = typeof document === 'undefined' ? null : document.activeElement
    const inside = at && at !== box.current && box.current.contains(at)
    if (e.shiftKey && (at === first || !inside)) { e.preventDefault(); last.focus() }
    else if (!e.shiftKey && (at === last || !inside)) { e.preventDefault(); first.focus() }
  }
  /* Portalled to the body: the Decision lives in the Inspector's sticky
   * footer (A3-F4), and a sticky or fixed box is its own stacking context, so
   * a scrim drawn inside it could not cover the rest of the screen. */
  const sheet = <div className="ws-shell-sheet-scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose() }}>
    <section className="ws-shell-dialog ws-decision-sheet" role="dialog" aria-modal="true" aria-labelledby="ws-decision-sheet-title" tabIndex={-1} ref={box} onKeyDown={onKey}>
      <h2 id="ws-decision-sheet-title">{C.heading}</h2>
      {version ? <p>{fill(C.version, { version })}</p> : null}
      <p>{fill(C.actor, { who })}</p>
      <p>{C.recorded}</p>
      <p className="ws-decision-not-an-action">{sentence}</p>
      <p className="ws-change-muted">{C.keys}</p>
      <div className="ws-decision-sheet-acts">
        <button type="button" className="ws-change-primary" onClick={onConfirm}>{C.act}</button>
        <button type="button" className="ws-change-secondary" onClick={onClose}>{C.cancel}</button>
      </div>
    </section>
  </div>
  return typeof document !== 'undefined' && document.body ? createPortal(sheet, document.body) : sheet
}

/**
 * What a person is told when the act was refused because the code moved
 * (T-5-2, `Q13`).
 *
 * Its own component because it is the whole of the answer to Sam's step 7 and
 * has to be renderable — and therefore testable — without a click: the two
 * sentences, the real paths that changed, what to do, and the ONE primary,
 * which is a door to what was checked rather than a second press of Approve.
 * Every path in the list is a file a person can open; nothing internal to the
 * engine has ever a place here (T-F-1-F6).
 */
export function MovedRefusal({ moved, onRecheck }) {
  const D = HQ_COPY.change.model.decision
  return <div className="ws-decision-moved" role="alert">
    {moved.lines.map((line, n) => <p key={n} className="ws-decision-moved-line">{line}</p>)}
    {moved.files.length ? <ul className="ws-decision-moved-files">{moved.files.map((f) => <li key={f}><code>{f}</code></li>)}</ul> : null}
    <p className="ws-change-muted">{moved.enabler}</p>
    <button type="button" className="ws-change-primary" onClick={onRecheck}>{D.moved_recheck}</button>
  </div>
}

const list = (v) => (Array.isArray(v) ? v : [])

function Verify({ model, onCopy, onReload, register, onGo, landing, onLand }) {
  const hq = model.hq || null
  /* T5 — the hand-off is the Change's, rebuilt from this read. */
  const tell = hq ? tellOf(hq) : null
  const cards = cardsTold(model.findings, tell)
  const left = model.scope.suggested.length
  const read = workReadCount(model)
  /* The criterion list comes FIRST, above the cards. Sam's second question is
   * asked per thing-that-counts-as-done, and until T-3-1 this view answered a
   * different question — "what does the record hold about the linked work" —
   * and let a person infer the answer to the one they asked. */
  const coverage = model.coverage || { rows: [], candidates: [], rowsLine: '' }
  return <>
    {/* What moved since these were checked, in Sam's words, above the lines it
      * explains — one truth per screen, from the one derived model (§3.8). */}
    {hq && hq.changedSince.any ? <p className="ws-change-changed-since">{hq.changedSince.text}</p> : null}
    {/* The accepted lines, as the model decided them: one state word each, and
      * "Proven" only ever with its scope and its version beside it. This
      * replaces the per-criterion state the coverage list used to render on its
      * own, so the Change screen, the Inbox row and the packet page cannot
      * disagree about one line. */}
    {hq && hq.criteria.length
      ? <ul className="ws-cov-lines">{hq.criteria.map((c) => <CriterionLine key={c.n} criterion={c} />)}</ul>
      : null}
    {/* T-3-4 (`B7.1`, `B7.6`, `Q12`) — the runs that did NOT reach the end,
      * above the Inspector because they are the thing a person has to act on
      * before anything below them means what it says.
      *
      * Two sentences, never one badge (design §3.8): what became of the run,
      * then what it settles. No state word appears on one of these cards —
      * a check that was cut short observed nothing to be right or wrong
      * about. One primary action, "Run it again", and no second control:
      * a re-run is a new examination, so there is nothing to resume. */}
    <RunCards hq={hq} changeId={model.id || ''} onCopy={onCopy} onReload={onReload} />
    <Inspector hq={hq} cards={cards} onCopy={onCopy} onDecided={onReload} onRecheck={() => { if (onGo) onGo('verify'); if (onReload) onReload() }} onDoor={(n) => openDoor(criterionDoor(n), onGo)} onLink={onLand || undefined} licence={model.licence} />
    <TellAgent tell={tell} onCopy={onCopy} />
    {/* Why nothing below says which version it was checked on, and the one act
      * that changes it (T-3-2b). Above the lines, because it is about all of
      * them; a prepared command, never a run. */}
    {coverage.nothingBoundSlug
      ? <section className="ws-change-unbound">
        <p className="ws-change-muted">{C.coverage.nothing_bound}</p>
        <p className="ws-inbox-command"><code>{fill(C.coverage.bind_command, { change: model.id || '' })}</code></p>
        <button type="button" className="ws-change-secondary" onClick={() => onCopy && onCopy(fill(C.coverage.bind_command, { change: model.id || '' }))}>{C.coverage.nothing_bound_action}</button>
      </section>
      : null}
    {coverage.rows.length
      /* The one derived model goes IN: this section says what the headline
        * says, because it renders the headline's own fields (design §8 item 1). */
      ? <CriterionCoverage changeId={model.id || ''} view={coverage} hq={hq} onChanged={onReload} landing={landing} />
      : null}
    {/* T6 — what happened after each line was raised, under the coverage it
      * is about, from the same rows the take-away carries. */}
    <RepairHistory hq={hq} />
    {/* The same prepared command the first screen offers, built by the same
      * function, so what a person copies here and what they copied there are
      * one text (`coverage-view.js` `runCheckCommand`). */}
    {/* T-6-6: and when no work is linked at all, because running a check for
      * this Change is the one thing that links some (`empty.change_unlinked`),
      * and the sentence below points up at this command. */}
    {coverage.rows.length || !model.scope.confirmed.length
      ? <section className="ws-change-run-check">
        <h3>{C.coverage.run_check_heading}</h3>
        {/* P6-5 — the check runs from here, with no terminal. The command a
          * person could type instead is one level down, under Technical. */}
        <p className="ws-change-muted">{HQ_COPY.run.door_note}</p>
        <RunDoor changeId={model.id || ''} version={hq ? hq.version : null} unlinked={hq && hq.readiness ? hq.readiness.unlinked : null} onDoor={onLand || ((n) => openDoor(criterionDoor(n), onGo))} onCancel={stopRun(model.id || '')} onRan={onReload} />
        <details className="ws-run-technical">
          <summary>{HQ_COPY.run.technical.heading}</summary>
          <p className="ws-inbox-command"><code>{runCheckCommand(model.id || '')}</code></p>
          <p className="ws-change-muted">{C.coverage.run_check_note}</p>
          <button type="button" className="ws-change-secondary" onClick={() => onCopy && onCopy(runCheckCommand(model.id || ''))}>{C.coverage.run_check_action}</button>
        </details>
      </section>
      : null}
    <h2>{C.verify.heading}</h2>
    <p className="ws-change-muted">
      {C.verify.scope_note}
      {/* STEP-7-LIMITS-R2 — the same number the nothing-wrong line below
        * counts: the conversations read, never a check run linked beside
        * them. With none read there is no count; the line below says so. */}
      {read ? ' ' + (read === 1 ? C.verify.sessions_one : fill(C.verify.sessions_many, { n: read })) + (left ? '.' : '') : null}
      {left ? ' ' + (left === 1 ? C.verify.suggested_excluded_one : fill(C.verify.suggested_excluded_many, { n: left })) : null}
    </p>
    {cards.length
      ? <div className="ws-change-cards" id="ws-change-cards">{cards.map((card, i) => <Card
          key={card.sessionId + ':' + card.id}
          card={card}
          primary={i === 0}
          /* P6-1: Plain / Technical in the top bar is this register; Plain by default. */
          register={register === 'operator' ? 'operator' : 'manager'}
          /* The card's receipts are its own ids — `clm`, `f_test_run_failed`.
           * On Trace, a technical surface, they are the grounding a reader
           * clicks. Here they sat on the surface labelled Plain as undefined
           * shorthand, which is the baseline's H2T-5 inherited whole (T24-2),
           * and with no proof viewer on this screen they were also buttons
           * that did nothing. Off here, on everywhere else; the proof path is
           * the Show proof control below, which opens the same step in Trace. */
          receipts="off"
          onCopy={onCopy}
        />)}</div>
      : <>
        <Empty text={model.scope.confirmed.length ? linkedWorkLine(model) : C.stages.verify.empty} />
        {/* Which reason depends on what is true: with work linked, asking a
          * person to link work is asking for what they have already done.
          * T5: when the Change's own draft exists, the message is right above
          * with its copy control, and an off control here would contradict it. */}
        {tell && tell.draft ? null : <Blocked
          id="ws-change-why-verify"
          label={C.actions.copy_prompt}
          pair={model.scope.confirmed.length
            ? (nothingCheckedYet(model) ? HQ_COPY.change.blocked.nothing_to_say_unchecked : HQ_COPY.change.blocked.nothing_to_say)
            : HQ_COPY.change.blocked.nothing_checked}
          primary
        />}
      </>}
    {/* The focus set belongs to this stage's question — "what happened when we
      * checked it?" — and to no other. It used to render under every stage,
      * where a reader had to decide each time whether it belonged to the stage
      * above it or to the Change (T24-4). */}
    <FocusList model={model} />
  </>
}

/* T-4-1a-FIX (R-2026-09-28-5 (a)) — where the accepted policy expects a
 * review and none can be had on this machine, say so plainly and why, where
 * the review would show. The engine's own reason is one click deeper. Never
 * "no findings": this line is about findings that cannot exist yet. */
function ReviewUnavailable({ said }) {
  if (!said) return null
  return <div className="ws-review-unavailable" role="status">
    <p className="ws-change-request">{said.sentence}</p>
    {said.technical
      ? <details className="ws-review-technical">
        <summary>{C.review.unavailable_now.technical}</summary>
        <p className="ws-change-muted">{said.technical}</p>
      </details>
      : null}
  </div>
}

function Review({ model, onReload, onGo }) {
  return <>
    <h2>{C.review.heading}</h2>
    <ReviewUnavailable said={model.hq && model.hq.readiness ? model.hq.readiness.reviewUnavailable : null} />
    <ReviewSection reviews={model.reviews} empty={<Empty text={C.stages.review.empty} />} />
    <FindingStandings standings={model.standings} changeId={model.id} onSaved={onReload} />
    {/* AI-REVIEW-OUT-OF-LAUNCH (R-2026-09-28-20): a build without AI review
      * offers none here — not a disabled control either. */}
    {model.hq && model.hq.aiReview === true
      ? <Blocked id="ws-change-why-review" label={C.actions.ask_for_review} pair={HQ_COPY.change.blocked.review_from_terminal} primary />
      : <button type="button" className="ws-change-primary" onClick={() => onGo('approve')}>{C.actions.see_what_you_would_approve}</button>}
  </>
}

function Approve({ model, onGo }) {
  const r = model.hq ? model.hq.readiness : null
  return <>
    <h2>{C.approve.heading}</h2>
    {model.accepted.criteria.length && model.version.bound
      ? <>
        <p className="ws-change-request">{versionLine(model.version)}</p>
        <ol className="ws-change-criteria">{model.accepted.criteria.map((t, i) => <li key={i}>{t}</li>)}</ol>
      </>
      : <Empty text={C.stages.approve.empty} />}
    <p className="ws-change-muted">{C.approve.never_green}</p>
    {/* T-3-5 — what is being approved, and whether the record is ready for it,
      * from the one computation. The stage and the Inspector give the same
      * answer because they read the same field. */}
    {model.hq ? <Readiness readiness={model.hq.readiness} /> : null}
    {/* RUN-LINK-TRUTH-R6 — the stage explains approval from the served
      * readiness answer and from nothing else. READY (with or without
      * exceptions): no blocked line; it says approval is available, in the
      * served `available` pair, and its one primary is a live door to the one
      * Decision (the Inspector's, on What we checked) — the decision itself
      * stays in one place (T-S-4). It used to fall back to `approveBlocker`
      * here, which counted a line a named person had set aside as "no
      * evidence yet" and told them to run the check nobody was going to run.
      * NOT READY or NO ANSWER: the served blocker. `approveBlocker` speaks only
      * when there is no derived model at all. */}
    {r && r.ready
      ? <>
        {model.hq.decision && model.hq.decision.approved
          ? <p className="ws-decision-headline" id="ws-change-approve-available">{model.hq.decision.headline}</p>
          : <p className="ws-change-approve-available" id="ws-change-approve-available">{r.available.reason} {r.available.enabler}</p>}
        {showsDecision(model.hq)
          ? <button type="button" className="ws-change-primary ws-approve-decide" aria-describedby="ws-change-approve-available" onClick={() => openDoor({ stage: 'verify', anchor: DECISION_BLOCK }, onGo)}>{HQ_COPY.change.coverage.decide}</button>
          : null}
      </>
      : r
        ? <Blocked id="ws-change-why-approve" label={C.approve.action} pair={r.blocker} primary />
        : <Blocked id="ws-change-why-approve" label={C.approve.action} pair={model.blocker} values={model.blocker.values} primary /> /* NO-READINESS: no derived model at all */}
    {model.hq ? <CodeLimit limit={model.hq.codeLimit} receipt={false} /> : null}
  </>
}

/**
 * The focus set, scoped exactly as the cards are: the confirmed work only.
 *
 * It is a LIST, not a canvas. The canvas is one click down, on Trace, where it
 * has always been — `B8.27` asks for a non-canvas representation of the same
 * relationships and this is it, reachable by keyboard, readable aloud, and
 * built by the same `focusView` the Trace canvas uses, over the same nodes.
 */
export function FocusList({ model }) {
  const shown = []
  const groups = []
  let total = 0
  let merged = 0
  for (const w of model.work) {
    if (!w.focus) continue
    total += w.focus.summary.total
    merged += w.focus.summary.merged
    for (const l of w.focus.labels) shown.push(l)
    for (const g of w.focus.groups || []) groups.push(g)
  }
  if (!model.work.length || !total) return <section className="ws-change-trace"><h2>{C.trace.heading}</h2><Empty text={C.trace.empty} /></section>
  return <section className="ws-change-trace">
    <h2>{C.trace.heading}</h2>
    <p className="ws-change-muted">{C.trace.note}</p>
    {/* A count over an empty list — "0 of 1 steps shown" with nothing under it
      * — is a number with no sentence, which is Sam's first failure mode, and
      * the walkthrough found it on a real record (T24-5). When nothing is
      * short of proven there is nothing to put first, and that is said. */}
    {shown.length
      ? <>
        <p className="ws-change-muted">{fill(C.trace.shown, { shown: shown.length, total })}{merged ? ' · ' + (merged === 1 ? C.trace.merged_one : fill(C.trace.merged_many, { n: merged })) : ''}</p>
        <ul className="ws-change-focus">{shown.map((l) => <li key={l.id}>{l.text}</li>)}</ul>
      </>
      : <p className="ws-change-muted">{traceNoneLine(model, foldedLines(groups).length)}</p>}
    {/* Where the folded steps went, in the focus set's own count-bearing
      * words. A bare "12 steps folded away" is a number with nowhere to go;
      * `focusView` already accounts for every one of them by category, and
      * this renders that accounting rather than inventing a second one. */}
    {foldedLines(groups).length ? <ul className="ws-change-focus ws-change-muted">{foldedLines(groups).map((l) => <li key={l.key}>{l.text}</li>)}</ul> : null}
    <a className="ws-change-secondary" href={'/sessions?session=' + encodeURIComponent(model.scope.confirmed[0] || '') + '#/trace'}>{C.trace.open}</a>
  </section>
}

/**
 * Where the folded steps went, in this screen's vocabulary.
 *
 * The Trace focus set accounts for every node it does not show, and its own
 * labels are Trace's words — "5 events that hold up", "3 trees recorded ·
 * tree 0d8a474fc679". Both are on the banned list for a Plain surface, and
 * the second is a hash. So the CATEGORY and the COUNT are carried across and
 * the WORDS are this screen's, from the copy file. A category the copy file
 * has no line for is dropped rather than rendered in Trace's words.
 *
 * @param {Array<{kind: string, count: number}>} groups
 * @returns {Array<{key: string, text: string}>}
 */
export function foldedLines(groups) {
  const F = C.trace.folded
  const total = new Map()
  for (const g of Array.isArray(groups) ? groups : []) {
    if (!g || typeof g.count !== 'number' || g.count < 1) continue
    total.set(g.kind, (total.get(g.kind) || 0) + g.count)
  }
  const out = []
  for (const [kind, n] of total) {
    const template = n === 1 ? F[kind + '_one'] : F[kind + '_many']
    if (!template) continue
    out.push({ key: kind, text: fill(template, { n }) })
  }
  return out
}

/** The technical layer: one click, never hidden, never on top (rule 1). */
export function Technical({ model, open }) {
  const T = C.technical
  const rows = [
    [T.change_id, model.id],
    [T.project_id, model.projectId],
    [T.opened_at, model.openedAt],
    [T.scope_digest, model.accepted.digest],
    [T.sessions, model.scope.confirmed.join(', ') || null],
    [T.provenance, model.scope.suggested.length || model.scope.rejected.length ? String(model.scope.suggested.length + model.scope.rejected.length) : null],
    [T.masking, model.masking ? model.masking.statement : null],
  ].filter(([, v]) => v != null && v !== '')
  return <details className="ws-change-technical" open={open ? true : undefined}>
    <summary>{T.open}</summary>
    <dl>{rows.map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{String(v)}</dd></div>)}</dl>
  </details>
}

/* ── the screen ───────────────────────────────────────────────────────── */

/**
 * @param {object} props
 * @param {object} props.model what `buildChangeView` decided
 * @param {string} [props.stage] the open stage; `define` by default
 * @param {(stage: string) => void} [props.onGo]
 * @param {(text: string, kind: string) => void} [props.onCopy]
 * @param {string[]} [props.notes] anything that could not be read, said aloud
 * @param {() => void} [props.onReload] re-read the documents after an act
 */
export function ChangeScreen({ model, stage, onGo, onCopy, notes, onReload, register, landing = null, onLand = null }) {
  const active = STAGES.indexOf(stage) >= 0 ? stage : 'define'
  const go = onGo || (() => {})
  const views = { define: Define, prepare: Prepare, build: Build, verify: Verify, review: Review, approve: Approve }
  const View = views[active]
  return <main className="ws-change"><div className="ws-change-inner">
    {/* T-S-5 (design §8 S2) — the packet's first real door, in the top bar.
      * One click from the Inbox row reaches this screen; this is the second,
      * and it is above the fold rather than below a panel a person has to
      * scroll past. It is secondary: the packet is what you take away. */}
    <div className="ws-change-topbar">
      <p className="ws-change-kicker">{C.kicker}</p>
      {model.id ? <a className="ws-change-secondary ws-change-packet-door" href={packetHref(model.id)}>{C.packet.door}</a> : null}
    </div>
    <h1 className="ws-change-title">{model.request.title || ''}</h1>
    <Conclusion conclusion={model.conclusion} />
    <TargetVersion version={model.version} />
    {/* STEP-7-LIMITS — under the version it is about. */}
    {model.hq ? <CodeLimit limit={model.hq.codeLimit} /> : null}
    {/* T-S-4 — what has been decided about THIS version, in the same text
      * block as the title, the sentence and the version, because "is this
      * approved, and is it still the thing I approved?" is read in the same
      * ten seconds as the rest. Both strings come from the one view-model. */}
    {model.hq ? <Readiness readiness={model.hq.readiness} /> : null}
    <UnrecordedTop unrecorded={model.hq && model.hq.unrecorded} />
    {model.hq && model.hq.decision && model.hq.decision.approved
      ? <>
        <p className="ws-decision-headline">{model.hq.decision.headline}</p>
        {model.hq.decision.movedLine ? <p className="ws-change-changed-since">{model.hq.decision.movedLine}</p> : null}
        {model.hq.decision.listMovedLine ? <p className="ws-change-changed-since">{model.hq.decision.listMovedLine}</p> : null}
        <p className="ws-decision-not-an-action">{model.hq.decision.sentence}</p>
      </>
      : null}
    <StageStrip stages={model.stages} active={active} onGo={go} />
    <p className="ws-change-muted ws-change-views-only">{C.stages.views_only}</p>
    <section
      className="ws-change-panel"
      role="tabpanel"
      id={'ws-change-panel-' + active}
      aria-labelledby={'ws-change-tab-' + active}
      tabIndex={0}
      data-primary-action=""
    >
      <View model={model} onGo={go} onCopy={onCopy} onReload={onReload} register={register} landing={landing} onLand={onLand} />
    </section>
    {(notes || []).length ? <ul className="ws-change-notes">{(notes || []).map((n, i) => <li key={i}>{n}</li>)}</ul> : null}
    {/* The whole story on one page (T-2-5), as a SECONDARY action: the six
      * stages above are where the work is done, and this is the same record
      * read end to end. It sits below the panel and above the technical layer,
      * because it is a plain-English surface and not a technical one. */}
    {model.id ? <p className="ws-change-wholestory"><a className="ws-change-secondary" href={'#/changes/' + encodeURIComponent(model.id) + '/packet'}>{C.packet.open}</a></p> : null}
    <Technical model={model} open={register === 'operator'} />
  </div></main>
}

/**
 * UNRECORDED-WORK-NOT-SILENT (PW-F6) — at the top of the Change, Brief and
 * Explore: what the agent said in linked work whose words the record does not
 * hold was not recorded, and has not been checked (`hq.unrecorded.top`).
 * Nothing when there is no such work.
 *
 * UNRECORDED-WORK-NO-QUOTE — beneath it, when the server placed the agent's
 * final claim in the log of such work: that ThruLine wasn't recording, that the
 * agent's own log holds what it said, and that this release does not show its
 * words (`hq.unrecorded.held`), with each log's file and line one level down,
 * folded away (`hq.unrecorded.logs[].receipt`). None of the agent's words is
 * here. The same field feeds the page and the file.
 */
export function UnrecordedTop({ unrecorded }) {
  if (!unrecorded || !unrecorded.count || !unrecorded.top) return null
  const T = HQ_COPY.change.unrecorded
  const logs = Array.isArray(unrecorded.logs) ? unrecorded.logs : []
  return <>
    <p className="ws-change-changed-since ws-change-unrecorded">{unrecorded.top}</p>
    {unrecorded.held && logs.length
      ? <div className="ws-change-unrecorded-log">
        <p className="ws-change-changed-since">{unrecorded.held}</p>
        <details className="ws-log-receipt">
          <summary>{T.receipt_summary}</summary>
          <ul>{logs.map((l) => <li key={l.id}><code>{l.receipt}</code></li>)}</ul>
        </details>
      </div>
      : null}
  </>
}

/* ── altitudes (P6-1; design record §3.1, §6) ─────────────────────────── */

/**
 * Brief — the story of this Change and nothing else (§3.1).
 *
 * One title, one sentence, one state chip, one scope line, the story Inspector
 * (T-S-3's four blocks) and ONE primary action, which is the Inspector's own.
 * No stage strip, no toolbar, no filter, no dock. Every state word, version
 * and count here is a field of `model.hq` — the one derived view-model
 * (`change-model.js`) — and this function derives none of its own; the fence
 * in `shell-chrome.test.js` reads its source.
 *
 * There is no canvas. The design's Brief shows the discrepancy thread lit on
 * the canvas, and nothing this build derives exposes one thread (P6-3 lights
 * it), so Brief shows the story without a picture rather than a picture that
 * pretends to be the thread.
 */
export function ChangeBrief({ model, onCopy, onReload, notes, register, onExplore, onDoor, onLink }) {
  const hq = model.hq
  /* T5 — the same draft Verify offers, from the same read. */
  const tell = tellOf(hq)
  const B = HQ_COPY.shell.brief
  const runs = list(hq.actionableRuns).length
  return <main className="ws-change ws-brief" aria-label={B.story_label}>
    <div className="ws-brief-grid">
      <section className="ws-brief-story">
        <div className="ws-change-topbar">
          <p className="ws-change-kicker">{C.kicker}</p>
          {hq.id ? <a className="ws-change-secondary ws-change-packet-door" href={packetHref(hq.id)}>{C.packet.door}</a> : null}
        </div>
        <h1 className="ws-change-title">{hq.title || ''}</h1>
        <p className="ws-change-conclusion">{hq.headline.text}</p>
        {/* The chip carries the leading line as the model wrote it, not the bare
          * word: "Proven" never stands without its scope and version (§3.8), and
          * `stateLine` in change-model.js is the one place that joins them. */}
        <p className="ws-brief-state">
          {hq.word
            ? <span className="ws-brief-chip" data-state={hq.state}>{hq.inspector.shows.line || hq.word}</span>
            : <span className="ws-change-muted">{B.no_state}</span>}
        </p>
        <p className="ws-change-version" aria-label={C.version.label}>{hq.version.text}</p>
        {/* STEP-7-LIMITS — under the version it is about. */}
        <CodeLimit limit={hq.codeLimit} />
        <p className="ws-brief-scope">{hq.packet.coversLine}</p>
        {/* T-3-5 (card item 0), and the whole of this fix round. The readiness
          * computation was wired into the stage view, which P6 replaced with
          * this one as the screen a person actually opens — so the engine said
          * "the check that covers this was stopped before it finished" and the
          * screen never printed it. Same field, same single derivation
          * (`hq.readiness`): Brief derives nothing here either. */}
        <Readiness readiness={hq.readiness} />
        {/* RUN-LINK-TRUTH (PW-F15) — Brief is the altitude a person opens, and
          * approval was nowhere on it until the Inspector's Decision appeared.
          * While the version cannot be approved, the control is here, OFF, as
          * a secondary (the Inspector's action stays the one primary), with
          * the Approve stage's own pair — `hq.readiness.blocker`, chosen from
          * the engine's reason. It writes nothing. */}
        {hq.decision && !showsDecision(hq) && hq.readiness && !hq.readiness.ready
          ? <div className="ws-brief-approve">
            <Blocked id="ws-brief-why-approve" label={C.approve.action} pair={hq.readiness.blocker} />
          </div>
          : null}
        <UnrecordedTop unrecorded={hq.unrecorded} />
        {/* P6-5 — Check it again, from the story: a secondary door, because
          * the Inspector's action is this screen's one primary. */}
        {hq.id && !runsFromHere(hq)
          ? <p className="ws-brief-run">
            <RunDoor changeId={hq.id} version={hq.version} unlinked={hq.readiness ? hq.readiness.unlinked : null} onDoor={onLink || onDoor} onCancel={stopRun(hq.id)} onRan={onReload} />{' '}
            <span className="ws-change-muted">{HQ_COPY.run.door_note}</span>
          </p>
          : null}
        {hq.decision && hq.decision.approved
          ? <>
            <p className="ws-decision-headline">{hq.decision.headline}</p>
            {hq.decision.movedLine ? <p className="ws-change-changed-since">{hq.decision.movedLine}</p> : null}
            {hq.decision.listMovedLine ? <p className="ws-change-changed-since">{hq.decision.listMovedLine}</p> : null}
            <p className="ws-decision-not-an-action">{hq.decision.sentence}</p>
          </>
          : null}
        <ReviewSummary reviews={model.reviews} />
        {model.standings && model.standings.any ? <FindingStandings standings={model.standings} changeId={hq.id || model.id} onSaved={onReload} /> : null}
        {/* T-6-6 (`HQ_COPY.empty.change_unlinked`, rule 12): with no recorded
          * work linked, the story says what a Change is, why nothing has been
          * checked, and the one thing that fills it — the exact command, with
          * a SECONDARY control that copies it, because the Inspector's action
          * is this screen's one primary. */}
        {model.scope && !model.scope.confirmed.length
          ? <section className="ws-brief-empty" data-empty="change_unlinked">
            <p className="ws-change-muted">{HQ_COPY.empty.change_unlinked.what}</p>
            <p className="ws-change-empty">{HQ_COPY.empty.change_unlinked.why}</p>
            <p className="ws-change-muted">{HQ_COPY.empty.change_unlinked.fill}</p>
            <p className="ws-inbox-command"><code>{runCheckCommand(hq.id || model.id || '')}</code></p>
            <button type="button" className="ws-change-secondary" onClick={() => onCopy && onCopy(runCheckCommand(hq.id || model.id || ''), 'command')}>{HQ_COPY.empty.change_unlinked.action}</button>
          </section>
          : null}
        {runs
          ? <p className="ws-change-muted ws-brief-runs">
            {runs === 1 ? B.runs_one : fill(B.runs_many, { n: runs })}{' '}
            <button type="button" className="ws-change-secondary" onClick={onExplore}>{B.open_explore}</button>
          </p>
          : null}
      </section>
      <div className="ws-brief-inspector" data-primary-action="">
        <Inspector hq={hq} cards={cardsTold(model.findings, tell)} onCopy={onCopy} onDecided={onReload} onRecheck={onReload} onDoor={onDoor} onLink={onLink} licence={model.licence} />
        <TellAgent tell={tell} onCopy={onCopy} />
      </div>
    </div>
    {(notes || []).length ? <ul className="ws-change-notes">{(notes || []).map((n, i) => <li key={i}>{n}</li>)}</ul> : null}
    <Technical model={model} open={register === 'operator'} />
  </main>
}

/** Where you are, for the top bar: every value is the model's. */
export function changeCrumbs(model) {
  const hq = model.hq || null
  const T = HQ_COPY.shell.topbar
  const v = hq ? hq.version : null
  /* BUILD-10 (P6B-HTT-7) — the served Change carries a project id and no
   * project name, and an id is not a word for this surface. The project is
   * named by the last segment of the folder the record holds for its work,
   * by `projectNameOf`, the one rule the Inbox card uses too (BUILD-14). With
   * no folder recorded the segment is dropped: the bar used to open with
   * "Project not named in this record", which read as a fault. */
  const project = projectNameOf(model.prepare ? model.prepare.workspace : null)
  return [
    ...(project ? [{ key: 'project', text: project }] : []),
    { key: 'work', text: (hq && hq.title) || model.request.title || '', work: true },
    /* The short form only when the model names one; otherwise the absence in
     * words. The full sentence (`hq.version.text`) is in the story below. */
    { key: 'version', text: v && v.short ? fill(T.version, { version: v.short }) : T.version_none },
  ]
}

/**
 * The Change screen inside the shell: top bar, sidebar, and the altitude.
 *
 * Brief is the story. Explore and Workbench are the whole Change screen as it
 * was before P6-1, unchanged — the six stages, the Verify lists, the decision
 * and the technical drawer. The Change screen has no controls that only
 * Workbench shows, so Explore and Workbench draw the same screen here; the workspace is where
 * the two differ.
 */
export function ChangeShell({ model, stage, onGo, onCopy, notes, onReload, altitude, onAltitude, register, onRegister, readAt = null, door = null, onDoorSpent = null }) {
  const brief = altitude === 'brief' && model.hq
  /* T10 — when this screen's last read COMPLETED; null until one has, and the
   * bar then says "read time not shown" rather than guessing. */
  const stamp = readStamp(readAt)
  /* P6-4 — where an Ask answer's evidence is. The stage that holds it, and,
   * from Brief, the altitude that draws the stages at all: Brief has no stage
   * strip, so a door that only moved the stage would leave a person on a
   * screen that had not visibly changed. Asking to be shown the evidence is a
   * choice, so it is remembered as one, like pressing E. */
  const goEvidence = (to) => {
    if (altitude === 'brief' && onAltitude) onAltitude('explore')
    if (onGo) onGo(to)
  }
  /* PATH-UI-2-R2 — a door from another screen (the whole-story page's "Go to …
   * to link it") asks for a line through the address. It is answered like every
   * other door to evidence: the same stage move, the same altitude rule
   * (`goEvidence`), and only once the stage is on the glass, because the line
   * and its name box do not exist before it. Each pass of this effect does one
   * step and waits for React to draw it, so nothing here counts on timing. It
   * is spent when it lands, and only then. */
  /* PATH-UI-2-R3 — the same request, made inside the app. The Inspector's and
   * the run sheet's "go to <line> to link it" doors go to the same control, so
   * they ask the same way: `land(n)` holds a request beside the one the address
   * may carry, and both are answered by the coverage section once the stage is
   * on the glass (`CriterionCoverage`, `landOnControls`). One request is in
   * flight at a time; it is spent when the stage has had its chance to take it,
   * so a request for a line the Change does not have cannot wait for a later
   * screen and take focus from a person who has moved on. */
  const [request, setRequest] = React.useState(null)
  const landing = door || request
  const land = (n) => { setRequest({ n }); goEvidence('verify') }
  React.useEffect(() => {
    if (!landing) return
    if (brief || stage !== 'verify') { goEvidence('verify'); return }
    /* Verify is on the glass and the coverage section, a child, has already
     * taken the request in this commit's effects. */
    if (door && onDoorSpent) onDoorSpent()
    if (request) setRequest(null)
  }, [landing, brief, stage])
  return <div className="ws-shell ws-shell-change" data-altitude={altitude}>
    <ShellTopBar
      crumbs={changeCrumbs(model)}
      altitude={altitude}
      onAltitude={onAltitude}
      register={register}
      onRegister={onRegister}
      readWhen={stamp ? stamp.text : null}
      readTitle={stamp ? stamp.title : null}
      onRefresh={onReload}
      /* Ask, for THIS Change, off the one derived view-model this screen
       * already holds. It reads nothing and writes nothing. */
      askPanel={model.hq ? ({ onClose }) => <AskPanel hq={model.hq} onGo={goEvidence} onClose={onClose} /> : null}
    />
    <div className="ws-shell-body">
      <ShellSidebar
        workName={(model.hq && model.hq.title) || model.request.title || ''}
        onWork={() => goToInbox(typeof window === 'undefined' ? null : window)}
        savedOff={HQ_COPY.shell.sidebar.saved_none_change}
        licence={model.licence ? { text: model.licence.line, href: LICENCE_HASH } : null}
      />
      {brief
        ? <ChangeBrief model={model} onCopy={onCopy} onReload={onReload} notes={notes} register={register} onExplore={() => onAltitude && onAltitude('explore')} onDoor={(n) => openDoor(criterionDoor(n), goEvidence)} onLink={land} />
        : <ChangeScreen model={model} stage={stage} onGo={onGo} onCopy={onCopy} notes={notes} onReload={onReload} register={register} landing={landing} onLand={land} />}
    </div>
  </div>
}

/** This machine's storage, or null where it is blocked. */
function localStore() {
  try { return typeof window === 'undefined' ? null : window.localStorage } catch (_blocked) { return null }
}

/** The loading, missing and refused states. Never a blank screen (Rule 12). */
export function ChangeNotice({ status, refusal }) {
  if (status === 'loading') return <main className="ws-change"><div className="ws-change-inner"><p className="ws-change-conclusion">{C.loading}</p></div></main>
  if (status === 'not_found') return <main className="ws-change"><div className="ws-change-inner">
    <h1 className="ws-change-title">{C.not_found.title}</h1>
    <p className="ws-change-conclusion">{C.not_found.body}</p>
  </div></main>
  return <main className="ws-change"><div className="ws-change-inner">
    <h1 className="ws-change-title">{C.failed.title}</h1>
    <p className="ws-change-conclusion">{fill(C.failed.body, { reason: refusal || '' })}</p>
  </div></main>
}

/* THE ADDRESS IS WATCHED IN ONE PLACE, and it is not here.
 *
 * This screen used to carry `movedToAnotherScreen` and `reboot`, a watcher
 * written for exactly one pair of screens — this one and the whole story on
 * one page — because that was the pair T-2-5 walked. The Inbox carried its own
 * for its own pair. Nothing watched the pair nobody had walked, and `T-2-8-F4`
 * found it: Back from here put `#/inbox` in the address bar and left this
 * screen on the glass. `routes.js` `watchAddress`, installed once in
 * `main.jsx`, now owns the choice of root for every screen there is.
 */

/** The id in `#/changes/<id>`, or null. Nothing else on this screen reads the address. */
export function changeIdFromHash(hash) {
  const m = /^#\/changes\/([A-Za-z0-9_-]{1,128})(?:\/([a-z]+))?$/.exec(hash || '')
  return m ? { id: m[1], stage: STAGES.indexOf(m[2]) >= 0 ? m[2] : 'define' } : null
}

/**
 * MOVED-WHILE-OPEN-TRUTH — what THIS screen has seen of the record's version,
 * across the reads it made itself.
 *
 * "This Change moved while this screen was open" is a claim about the
 * reader's own session, so only a screen that watched the served version
 * change between two of its own reads may say it. The first read of a Change
 * on a screen never can: whatever the record stands at then, the screen
 * arrived after it (the walk's case: prepare, accept a new list, then load the
 * Change). `moved` is sticky for as long as the screen stays open, because a
 * move seen once stays a move that happened while it was open; it is per
 * Change, so a screen taken to another Change starts again.
 *
 * PURE. The version compared is `decision.seen`, the record's own version as
 * the served decisions document names it — the same one an act sends back.
 *
 * @param {{id: string, version: string|null, moved: boolean}|null} before what the last read left
 * @param {string} id the Change this read is of
 * @param {object|null} model what `readChange` returned
 * @returns {{id: string, version: string|null, moved: boolean}}
 */
export function watchRecord(before, id, model) {
  const version = (model && model.hq && model.hq.decision && model.hq.decision.seen) || null
  if (!before || before.id !== id) return { id, version, moved: false }
  const changed = Boolean(before.version && version && before.version !== version)
  return { id, version: version || before.version, moved: before.moved || changed }
}

/**
 * The model, with the moved sentence a screen that WATCHED the move may say
 * in place of the one any screen may say (`decision.movedWhileOpenLine`,
 * `decision.movedLine`). Every place that draws the sentence reads `movedLine`
 * off `hq.decision`, so nothing else changes; nothing is written, and the
 * approval's version binding is not touched.
 */
export function movedWhileOpen(model) {
  const d = model && model.hq && model.hq.decision
  if (!d || !d.movedWhileOpenLine) return model
  return { ...model, hq: { ...model.hq, decision: { ...d, movedLine: d.movedWhileOpenLine } } }
}

export default function ChangeWorkspace({ id, stage: initialStage }) {
  const [state, setState] = React.useState({ status: 'loading' })
  const [stage, setStage] = React.useState(initialStage || 'define')
  /* P6-1: the altitude this person chose last (Brief if they never chose),
   * and the wording. Choosing one is remembered; opening at `?altitude=` is not. */
  const [altitude, setAltitudeState] = React.useState(() => initialAltitude({
    search: typeof window === 'undefined' ? '' : window.location.search,
    storage: localStore(),
  }))
  const [register, setRegister] = React.useState('manager')
  const chooseAltitude = (a) => { setAltitudeState(a); rememberAltitude(localStore(), a, { chosen: true }) }
  /* PATH-UI-2-R2 — the request a door on another screen made, read once from
   * the address this document was opened at (`door-request.js`), and spent by
   * the shell when it has landed. */
  const [door, setDoor] = React.useState(() => doorFromSearch(typeof window === 'undefined' ? '' : window.location.search))
  const spendDoor = () => { setDoor(null); spendDoorAddress(typeof window === 'undefined' ? null : window) }
  /* Bumped after a link act, so the screen re-reads the record rather than
   * editing its own copy of it. What a person sees after acting is what the
   * store holds, not what this file guessed the store would hold. */
  const [generation, setGeneration] = React.useState(0)
  /* T10 — stamped when a read RESOLVES ready: every mount and every reload. */
  const [readAt, setReadAt] = React.useState(null)
  /* MOVED-WHILE-OPEN-TRUTH — the record's version as this screen has read it. */
  const watched = React.useRef(null)
  React.useEffect(() => {
    let live = true
    readChange(id).then((r) => {
      if (!live) return
      let shown = r
      if (r && r.status === 'ready') {
        watched.current = watchRecord(watched.current, id, r.model)
        if (watched.current.moved) shown = { ...r, model: movedWhileOpen(r.model) }
      }
      setState(shown)
      if (r && r.status === 'ready') setReadAt(Date.now())
    })
    return () => { live = false }
  }, [id, generation])
  const copy = (text) => { try { navigator.clipboard.writeText(text) } catch { /* nothing else to do */ } }
  if (state.status !== 'ready') return <ChangeNotice status={state.status} refusal={state.refusal} />
  /* P6-5 — the one open run sheet is held here, above every door to it, so a
   * run that changes what the screen shows cannot unmount the sheet mid-run. */
  return <RunSheetHost><ChangeShell
    model={state.model}
    stage={stage}
    onGo={setStage}
    onCopy={copy}
    notes={state.notes}
    onReload={() => setGeneration((n) => n + 1)}
    altitude={altitude}
    onAltitude={chooseAltitude}
    register={register}
    onRegister={setRegister}
    readAt={readAt}
    door={door}
    onDoorSpent={spendDoor}
  /></RunSheetHost>
}
