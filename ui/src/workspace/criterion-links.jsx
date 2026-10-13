import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { writeAction } from './data.js'
import { linkBlocker } from './coverage-view.js'
import { EVIDENCE_ANCHORS } from './ask-hq.js'
import { Blocked } from './blocked.jsx'
import { showsDecision } from './change-model.js'

/* What was looked at for each thing that counts as done, and the two acts a
 * person can take about it (T-3-1).
 *
 * WHY THIS IS ITS OWN FILE. `change-workspace.jsx` writes nothing, and a test
 * reads its source to prove it. This card adds the first write that belongs on
 * the Change screen, so the write lives here, alone, where it can be read in
 * one screenful: ONE address, ONE method, TWO acts, each requiring a name.
 * `criterion-links.test.js` asserts that `/api/changes/<id>/links` is the only
 * address this file posts to and that neither act can be taken without an
 * actor.
 *
 * WHAT IT MAY SAY. Every user-visible string comes from
 * `HQ_COPY.change.coverage`, with one disclosed exception: the accepted line
 * itself and the evidence's own recorded name, which are the RECORD speaking.
 * Since P6-DESIGN-FIXES-13 the section also carries the Change-level sentence,
 * state word and primary — and every one of those is a field of the one
 * derived view-model (`change-model.js`), passed in as `hq` and rendered
 * unchanged. This file is inside the `change-model.test.js` grep fence from
 * that card: it may not construct a state word.
 *
 * WHAT IT REFUSES TO SAY. No state word as the answer for a line — the five
 * words describe one piece of evidence and appear on the item, never on the
 * line above it. Nothing green. A line with nothing behind it says "No test ran
 * for this." and is not styled as a failure, because it is not one.
 */

const C = HQ_COPY.change.coverage

/** The one address this file writes to. */
export const CHANGE_LINKS = (id) => '/api/changes/' + encodeURIComponent(id) + '/links'

/**
 * Add or remove one link, as one explicit act with a name on it.
 *
 * `unlink` never deletes: the server appends a tombstone, and the removed link
 * stays on the screen under its own heading. That is stated in the copy the
 * control carries, not only in the engine.
 *
 * @param {string} changeId
 * @param {{criterion: number, kind: string, evidence: string, act: 'link'|'unlink',
 *   actor: string, reason: string}} request
 * @returns {Promise<import('./data.js').Read>}
 */
export function submitLink(changeId, request) {
  return writeAction(CHANGE_LINKS(changeId), {
    criterion: request.criterion,
    kind: request.kind,
    evidence: request.evidence,
    act: request.act,
    actor: request.actor,
    reason: request.reason || '',
  })
}

/**
 * What broke, in the check's own words, and where the finding came from.
 *
 * T-3-6-F3 (`Q2`, `B16.8`, `B16.10`), lifted out of `EvidenceRow` by
 * `T-3-6-F6-BRIEF` — WHY IT IS ITS OWN COMPONENT. F3 put this block on the
 * Verify tree; P6 then made `ChangeBrief` the component the Change route
 * renders for every stage segment, and Brief's "What the record shows" block
 * listed the same items without it. A fresh install therefore never showed
 * what broke or that the finding was seeded, and `B16.10`'s "wherever the
 * finding is shown" was met on a screen nobody landed on. One component, one
 * set of fields (`coverage-view.js`'s `item.failure` and `item.state`), two
 * call sites — Verify's `EvidenceRow` and Brief's Inspector — so neither can
 * drift from the other and a third surface cannot re-word it.
 *
 * It renders only on a line the record holds the opposite of: a passing run
 * has nothing that broke, and a "no line from the failure" sentence under one
 * would invent a failure to be silent about. The text inside is the RUN
 * speaking and is rendered verbatim; every other word is `HQ_COPY`. An absent
 * line is stated as absent and never invented; an undeclared class is stated
 * as undeclared and never read as ordinary.
 */
export function FailureBlock({ item }) {
  const it = item || {}
  if (it.state !== 'not_proven' || !it.failure) return null
  /* `tabIndex={-1}`: "Look at what broke" puts focus here (A3-F10); it is
   * never a Tab stop. */
  return <div className="ws-cov-item-failure" tabIndex={-1}>
    <p className="ws-cov-item-failure-heading">{it.failure.heading}</p>
    {it.failure.line
      ? <pre className="ws-cov-item-failure-line">{it.failure.line}</pre>
      : <p className="ws-cov-item-failure-none">{it.failure.none}</p>}
    <p className="ws-cov-item-class">{it.failure.classLabel}: {it.failure.classText}</p>
  </div>
}

/** One piece of evidence under one accepted line. */
export function EvidenceRow({ item, onUnlink, actor, removed }) {
  const id = 'ws-cov-why-' + item.id
  return <li className="ws-cov-item">
    <p className="ws-cov-item-name">{item.name || item.evidenceId}</p>
    <p className="ws-cov-item-why">{item.basisText}</p>
    {item.note ? <p className="ws-cov-item-note">{item.note}</p> : null}
    {/* T-3-2 — was this about the version you are looking at? One sentence,
      * either way, keyed by the engine's own slug. A mismatch is stated, the
      * item stays on the screen, and it counts for nothing. */}
    {item.appliesText ? <p className={'ws-cov-item-applies' + (item.applies === false ? ' ws-cov-item-elsewhere' : '')}>{item.appliesText}</p> : null}
    {item.state && HQ_COPY.states[item.state]
      ? <p className="ws-cov-item-state">{HQ_COPY.states[item.state].word} — {HQ_COPY.states[item.state].subtitle}</p>
      : null}
    {/* What broke and where the finding came from — the one block, shared
      * with Brief's Inspector (T-3-6-F6-BRIEF). */}
    <FailureBlock item={item} />
    {item.actor ? <p className="ws-change-muted">{fill(removed ? C.removed_by : C.by_who, { who: item.actor })}</p> : null}
    {item.reason ? <p className="ws-change-muted">{item.reason}</p> : null}
    {removed
      ? null
      : actor
        ? <button type="button" className="ws-change-secondary" onClick={() => onUnlink(item)}>{C.unlink}</button>
        : <Blocked id={id} label={C.unlink} pair={C.disabled.needs_name} />}
  </li>
}

/**
 * The name box that gates every line's link control, as a selector (PATH-UI-2-R2).
 * A door that means "let me link this line" ends here: the box a person has to
 * fill before the control comes on. There is one `.ws-cov` section on the
 * screen, so the selector names one input; `packet-link-door.test.js` fails if
 * a second appears or this one stops matching.
 */
export const LINK_NAME_INPUT = '.ws-cov .ws-cov-actor input'

/**
 * PATH-UI-2-R3 — put the view where a door to a line's link control promised:
 * the name box in the cursor AND on the glass, with the line's row directly
 * under it. The row is directly under it because the coverage section pins the
 * line a door named to the top of its list (see `CriterionCoverage`), which is
 * what lets one scroll show the name box and the control together on a Change
 * of any length. Two things make this the whole of it:
 *
 *   - `focus({ preventScroll: true })`. A plain `focus()` scrolls the browser's
 *     own way, which is what R2's door did: it carried the view to the name box
 *     from wherever the line had just been scrolled to, and left the control
 *     behind. Focus never moves the view here; the one scroll below does.
 *   - `scrollIntoView({ block: 'start' })` on the name box's own block, so it
 *     is the first thing on the glass and the row is the next.
 *
 * A door, not an act: nothing is read, nothing is written, and the control
 * still asks for the name before it does anything.
 *
 * @param {Document|null} [doc]
 * @returns {boolean} whether the name box was there to go to
 */
export function landOnControls(doc) {
  const d = doc || (typeof document === 'undefined' ? null : document)
  if (!d || typeof d.querySelector !== 'function') return false
  const input = d.querySelector(LINK_NAME_INPUT)
  if (!input) return false
  if (typeof input.focus === 'function') input.focus({ preventScroll: true })
  const block = typeof input.closest === 'function' ? input.closest('.ws-cov-actor') : null
  const target = block || input
  if (typeof target.scrollIntoView === 'function') target.scrollIntoView({ block: 'start' })
  return true
}

/**
 * One accepted line, what bears on it, and what a person may do about it.
 *
 * The sentence for the line comes from the served state and nothing else. The
 * items are shown under it whether or not they bear: a run that settled nothing
 * is still something that happened, and hiding it would leave a person reading
 * "no test ran for this" about a line a command was definitely pointed at.
 */
export function CriterionRow({ row, changeId, actor, candidates, onAct, busy, note, whySlug, current }) {
  const [open, setOpen] = React.useState(false)
  const blocker = linkBlocker({ candidates, actor, whySlug })
  const whyId = 'ws-cov-link-why-' + row.n
  /* PATH-UI-2-R3 — `current` is the row a door brought a person to. It is
   * marked for a screen reader (`aria-current`), said in words for everyone
   * (the caption), and the name box's description reads the caption and the
   * line together, so door 12 and door 1 do not end in the same place. */
  return <li
    className={'ws-cov-row' + (current ? ' ws-cov-row-current' : '')}
    id={'ws-cov-row-' + row.n}
    aria-current={current ? 'true' : undefined}
  >
    {current ? <p className="ws-cov-row-landed" id={'ws-cov-landed-' + row.n}>{C.landed}</p> : null}
    <p className="ws-cov-criterion" id={'ws-cov-criterion-' + row.n}>{row.text}</p>
    <p className={'ws-cov-state ws-cov-state-' + row.state}>{row.sentence}</p>
    {row.items.length
      ? <ul className="ws-cov-items">{row.items.map((item) => <EvidenceRow
          key={item.id}
          item={item}
          actor={actor}
          onUnlink={(it) => onAct({ criterion: row.n, kind: it.kind, evidence: it.evidenceId, act: 'unlink' })}
        />)}</ul>
      : null}
    {row.removed.length
      ? <details className="ws-cov-removed">
        <summary>{C.removed_heading}</summary>
        <ul className="ws-cov-items">{row.removed.map((item) => <EvidenceRow key={item.id} item={item} actor={actor} removed />)}</ul>
      </details>
      : null}
    {blocker
      ? <Blocked id={whyId} label={C.link} pair={blocker} />
      : <>
        <button type="button" className="ws-change-secondary" disabled={busy} onClick={() => setOpen(!open)}>{C.link}</button>
        {open
          ? <ul className="ws-cov-candidates">{candidates.map((c) => <li key={c.kind + ':' + c.evidenceId}>
            <button type="button" className="ws-change-secondary" onClick={() => onAct({ criterion: row.n, kind: c.kind, evidence: c.evidenceId, act: 'link' })}>
              {c.name || c.evidenceId}
            </button>
          </li>)}</ul>
          : null}
      </>}
    {note ? <p className="ws-change-note">{note}</p> : null}
  </li>
}

/** Where the one decision lives, on this same screen (T-S-4). */
export const DECISION_BLOCK = 'ws-change-decision'

/**
 * Take a person to the decision, on the screen they are already on.
 *
 * It is a door, not an act: nothing is written, nothing is read, and the
 * Decision block still asks for a name before anything is recorded. In a
 * renderer with no document (the static-markup tests, and the server) it does
 * nothing at all rather than throwing.
 */
export function goToDecision() {
  const el = typeof document === 'undefined' ? null : document.getElementById(DECISION_BLOCK)
  if (!el) return false
  if (typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'start' })
  const first = typeof el.querySelector === 'function' ? el.querySelector('input, button, a') : null
  if (first && typeof first.focus === 'function') first.focus()
  return true
}

/**
 * A3-F10 — where "Look at what broke" goes. T-3-6-F3 made it the one primary
 * on a Change whose record holds a failure, ON, a door to what broke — and no
 * surface ever drew it on: it was a disabled button with an empty reason. It
 * goes to the failure's own words on this screen (the Inspector's first, which
 * Brief and Verify both draw), opening a fold that holds them, and falls back
 * to the line itself.
 *
 * @param {number|null} n the criterion the model names (`inspector.action.door`)
 * @returns {boolean} whether anything was there to go to
 */
export function goToWhatBroke(n) {
  if (typeof document === 'undefined') return false
  const el = document.querySelector('.ws-change-inspector .ws-cov-item-failure')
    || document.querySelector('.ws-cov-item-failure')
    || (n == null ? null : document.getElementById('ws-cov-line-' + n))
  if (!el) return false
  const fold = el.closest('details')
  if (fold) fold.open = true
  if (typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'start' })
  if (!el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1')
  if (typeof el.focus === 'function') el.focus()
  return true
}

/**
 * The door itself: the model's label, on, and it goes to what broke. It names
 * the line it opens (`data-door`, A3-F4). Handed an `onDoor` — the
 * Inspector's, which takes Brief to where the lines are and lands Verify on
 * the line — it goes there; otherwise it goes to what broke on this screen.
 */
export function WhatBrokeDoor({ action, onDoor }) {
  return <button type="button" className="ws-change-primary ws-what-broke" data-door={EVIDENCE_ANCHORS.criterion(action.door)} onClick={() => (typeof onDoor === 'function' ? onDoor(action.door) : goToWhatBroke(action.door))}>{action.label}</button>
}

/**
 * What this node says about the Change as a whole — the SAME state word, the
 * SAME count and the SAME one primary the headline of the screen carries
 * (P6-DESIGN-FIXES-13; design record §8 item 1).
 *
 * WHY. On the prototype the headline said the work was proven on this version
 * while this node's own inspector said "In progress… three have no evidence
 * yet… Not proven" and offered "Review what counts as done". Both sentences
 * were about the same record, four inches apart. Nothing here is derived:
 * `hq.headline.text` is the screen's own top sentence, `hq.inspector.shows.line`
 * is the leading criterion's line, and the off-state primary is the model's own
 * `inspector.action` — this component cannot disagree with the headline because
 * it computes nothing to disagree with.
 */
export function CriterionSummary({ hq, onDecide }) {
  if (!hq) return null
  const a = hq.inspector.action
  /* RUN-LINK-TRUTH-R3 — the one rule the Inspector's Decision reads, and no
   * second one: a live decide door here only when the Inspector draws the
   * Decision it points at. Every line reading proven is not enough; the
   * engine's readiness decides. */
  const decidable = showsDecision(hq)
  const go = typeof onDecide === 'function' ? onDecide : goToDecision
  return <div className="ws-cov-summary">
    <p className="ws-cov-summary-count">{hq.headline.text}</p>
    {hq.inspector.shows.line || hq.word
      ? <p className="ws-cov-summary-state">{hq.inspector.shows.line || hq.word}</p>
      : null}
    {decidable
      ? <button
        type="button"
        className="ws-change-primary ws-cov-decide"
        aria-controls={DECISION_BLOCK}
        onClick={() => go()}
      >{C.decide}</button>
      /* A3-F4: the model's live door ("Look at what broke") is drawn live.
        * A3-F10: it is the one door component, `WhatBrokeDoor`, the same
        * button the Inspector draws, naming the line that broke; with no
        * `onDoor` here it goes to what broke on this screen. */
      : a.enabled && a.door != null
        ? <WhatBrokeDoor action={a} />
        : <Blocked id="ws-cov-why-primary" label={a.offLabel || a.label} pair={{ reason: a.reason, enabler: a.enabler }} primary />}
  </div>
}

/**
 * The whole section: one line per thing that counts as done.
 *
 * The name box is above the list and not beside each control, because the
 * person's name is a fact about the person and not about the line. Until it is
 * filled, every act on the list is off and says so.
 */
export function CriterionCoverage({ changeId, view, hq, onChanged, onDecide, landing }) {
  const [actor, setActor] = React.useState('')
  const [busy, setBusy] = React.useState(false)
  const [note, setNote] = React.useState(null)
  /* PATH-UI-2-R3 — the line a door brought a person to. `landing` is the
   * request (`{ n }`) the Change screen hands down while a door is in flight;
   * this section takes it once, remembers the line for as long as it is on the
   * glass (leaving the stage forgets it), and does the landing in two steps
   * because the second needs the first on the screen: the row is pinned to the
   * top of the list, and only then is the view placed on the name box and the
   * row under it. A request for a line the Change does not have does nothing:
   * focus is never taken to a control a person was not sent to. */
  const [landed, setLanded] = React.useState(null)
  React.useEffect(() => {
    if (landing && view.rows.some((r) => r.n === landing.n)) setLanded({ n: landing.n })
  }, [landing])
  React.useEffect(() => {
    if (landed) landOnControls()
  }, [landed])
  const pinned = landed ? view.rows.find((r) => r.n === landed.n) || null : null
  const ordered = pinned ? [pinned, ...view.rows.filter((r) => r !== pinned)] : view.rows

  const act = async (request) => {
    setBusy(true)
    setNote(null)
    const answer = await submitLink(changeId, { ...request, actor, reason: '' })
    setBusy(false)
    if (answer.status !== 'ready') {
      setNote(answer.refusal)
      return
    }
    if (typeof onChanged === 'function') onChanged()
  }

  if (!view.rows.length) return null
  return <section className="ws-cov">
    <h3>{C.heading}</h3>
    <p className="ws-change-muted">{C.note}</p>
    <CriterionSummary hq={hq} onDecide={onDecide} />
    <label className="ws-cov-actor">
      <span>{C.who}</span>
      <input
        type="text"
        value={actor}
        onChange={(e) => setActor(e.target.value)}
        aria-describedby={pinned ? 'ws-cov-landed-' + pinned.n + ' ws-cov-criterion-' + pinned.n : undefined}
      />
    </label>
    <ol className="ws-cov-rows">{ordered.map((row) => <CriterionRow
      key={row.n}
      row={row}
      current={pinned === row}
      changeId={changeId}
      actor={actor}
      candidates={view.candidates}
      whySlug={view.whySlug}
      onAct={act}
      busy={busy}
      note={note}
    />)}</ol>
  </section>
}

export default CriterionCoverage
