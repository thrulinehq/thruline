import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { buildVerdict } from './verdict-model.js'
import { padUnderSticky } from './sticky-padding.js'
import { CheckingSequence, PHASES_CONTRACT, readPhaseCache, writePhaseCache, phaseLogHasUnit, phaseLogHasTiming, isUnitCount, UNIT_OF } from './checking-sequence.jsx'

/* HQ Verdict Panel — the Verdict tab (docs/HQ-VERDICT-PANEL-PLAN.md Step 2).
 * Every user-visible string comes from HQ_COPY, with one disclosed exception:
 * when `fetch` itself rejects (the network or the local server unreachable),
 * the browser's own error message passes through verbatim as the failure
 * reason — the environment's words, not copy (`phaseFailureReason`). Every
 * verdict sentence carries a receipt chip or the `ws-verdict-inference` class
 * — the headline included (chips for the claim ids it counts) — and the
 * "Checked:" line carries the "what was checked" replay link. The one sentence
 * with neither is the loading notice that stands in the "Checked:" line's place
 * while the phase log is on its way (below): it carries no number. The failure
 * notice in that place carries the inference class; its only digits are an
 * HTTP status inside its reason. The model
 * decides; this file only lays it out.
 *
 * Step 3: before the verdict, the checking sequence replays the phase log
 * (`GET /api/phases?session=`, §2.1.1 of docs/UI-SERVER-CONTRACT.md) — the
 * only fetch this file makes, same-origin, cached per (session, final tree)
 * in sessionStorage, and shared across mounts while in flight (one request
 * per (session, final tree) however many times the panel mounts). A fetch no mount holds
 * any more is not cancelled: it completes and fills the cache, so leaving
 * the tab mid-run and coming back costs no second run. The verdict never
 * depends on it.
 *
 * The "Checked:" line renders only from the phase log: its nouns are the
 * log's, and the model's counts under them contradicted it ("1 tree" beside
 * a log of 1681, ADJUDICATIONS item D fourth ruling, finding 2). With no log,
 * the line's place on the landed verdict holds the reason instead, in both
 * motion settings, with no count and no "what was checked" link (nothing to
 * replay): the fetch failed — in normal motion a failure lands the verdict at
 * once, so this is where its reason is read — or the log is still on its way,
 * or the session has no id to ask for one (`CheckedSlot`).
 *
 * A landing is remembered per session in sessionStorage, so a return to the
 * tab renders the verdict at once; "what was checked" replays once the log is
 * here. Under reduced motion the static, all-ticked list and the verdict
 * render together with no timers, and "what was checked" re-shows and scrolls
 * to the list; a failed log has no list to show, so the panel withholds it
 * and the reason renders once, in the "Checked:" line's place. */

const C = HQ_COPY.chrome
export const SHOWN = 5
export const HEADLINE_CHIPS = 3

function prefersReducedMotion() {
  try { return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches } catch { return false }
}

/* ── one in-flight phase fetch per (session, final tree), shared across mounts ── */

const inflight = new Map()

/** The in-flight key: the phase cache's key, (session, final tree). A tree that moves mid-flight starts its own fetch. */
const inflightKey = (sessionId, finalTree) => JSON.stringify([sessionId, finalTree == null ? null : finalTree])

/** How many (session, final tree) pairs have a phase fetch in flight right now (for tests). */
export function inflightPhaseFetches() {
  return inflight.size
}

/**
 * The reason a failed phase fetch shows inside `phases.failed`. The three
 * failures this file detects carry HQ_COPY text (`phases.failed_http`,
 * `phases.failed_body`, `phases.failed_timing`) as their message. Anything else — a `TypeError` from
 * `fetch` itself when the network or the local server is unreachable — is the
 * browser's own record of what went wrong, not copy, and is passed through as
 * the browser wrote it.
 */
export function phaseFailureReason(e) {
  return e && e.message ? e.message : String(e)
}

/**
 * Lease the phase-log fetch for a (session, final tree). Every mount that
 * needs the log acquires; the first acquisition starts the one fetch, the
 * rest share its promise.
 *
 * Nothing cancels it. The server runs `claims --json --phases` to completion
 * whatever the socket does (`uiserve.rs` waits on the child's output), so an
 * abort saved nothing and threw the finished run away; a return to the tab
 * then paid a second full run (ADJUDICATIONS item E, second ruling). Instead
 * a fetch runs to completion even when no mount holds it any more, writes
 * its result to the phase cache for the (session, final tree) it was started
 * for, and stays in the in-flight map until it settles — so a mount that
 * returns while it is still running re-acquires the same promise. A settled
 * entry leaves the map: a success then lives in sessionStorage, and a
 * failure is fetched afresh by the next acquisition.
 *
 * The map is keyed like the cache, on (session, final tree): a mount for the
 * same session whose final tree has moved does not re-acquire the old tree's
 * promise (whose result is cached under the old tree); it starts its own.
 *
 * @param {string} sessionId
 * @param {string} [finalTree] the session's final tree; the cache key's second half
 * @returns {{promise: Promise<object>, release: () => void}}
 */
export function acquirePhaseLog(sessionId, finalTree) {
  const key = inflightKey(sessionId, finalTree)
  let entry = inflight.get(key)
  if (!entry) {
    const P = HQ_COPY.phases
    const promise = fetch(`/api/phases?session=${encodeURIComponent(sessionId)}`, { headers: { Accept: 'application/json' } })
      .then((r) => (r.ok
        ? r.json().catch((e) => Promise.reject(e instanceof SyntaxError ? new Error(P.failed_body) : e))
        : Promise.reject(new Error(fill(P.failed_http, { status: r.status })))))
      .then((doc) => {
        if (!doc || doc.contract !== PHASES_CONTRACT || !phaseLogHasUnit(doc)) throw new Error(P.failed_body)
        /* A row the engine could not time (`elapsed_ms: null`) is unknown, not 0: the log is refused with its own reason. */
        if (!phaseLogHasTiming(doc)) throw new Error(P.failed_timing)
        return doc
      })
    const created = { promise, holders: 0 }
    entry = created
    inflight.set(key, created)
    const leave = () => { if (inflight.get(key) === created) inflight.delete(key) }
    promise.then((doc) => { writePhaseCache(sessionId, finalTree, doc); leave() }, leave)
  }
  entry.holders += 1
  const held = entry
  let released = false
  return {
    promise: held.promise,
    /* A lease count only: a zero count cancels nothing (see above). */
    release() {
      if (released) return
      released = true
      held.holders -= 1
    },
  }
}

/** The log state for a failure: no doc, and its reason. What usePhaseLog sets; the landed verdict renders the reason in the "Checked:" line's place. */
export function failedLog(reason) {
  return { doc: null, status: { kind: 'failed', reason } }
}

/** The log's initial state for a session: the cache, or loading. Pure over storage. */
function initialLog(sessionId, finalTree) {
  if (!sessionId) return { doc: null, status: null }
  const hit = readPhaseCache(sessionId, finalTree)
  return hit ? { doc: hit, status: { kind: 'ready', cached: true, unparsed: hit.unparsed } } : { doc: null, status: { kind: 'loading' } }
}

/** The phase log for a session: cache hit, else the one shared fetch. Pure state machine over `status`. */
function usePhaseLog(sessionId, finalTree) {
  const [log, setLog] = React.useState(() => initialLog(sessionId, finalTree))
  React.useEffect(() => {
    const first = initialLog(sessionId, finalTree)
    setLog(first)
    if (!sessionId || first.doc) return undefined
    if (typeof fetch !== 'function') { setLog(failedLog(C.no_fetch)); return undefined }
    let alive = true
    /* The lease writes the cache when the fetch completes, held or not. */
    const lease = acquirePhaseLog(sessionId, finalTree)
    lease.promise
      .then((doc) => {
        if (!alive) return
        setLog({ doc, status: { kind: 'ready', cached: false, unparsed: doc.unparsed } })
      })
      .catch((e) => { if (alive) setLog(failedLog(phaseFailureReason(e))) })
    return () => { alive = false; lease.release() }
  }, [sessionId, finalTree])
  return log
}

/* ── the landing, remembered per session ─────────────────────────────── */

export function landedKey(sessionId) {
  return `thruline-verdict-landed:${sessionId}`
}

export function readLanded(sessionId, storage) {
  const store = storage || (typeof sessionStorage !== 'undefined' ? sessionStorage : null)
  if (!store || !sessionId) return false
  try { return store.getItem(landedKey(sessionId)) === '1' } catch { return false }
}

export function writeLanded(sessionId, storage) {
  const store = storage || (typeof sessionStorage !== 'undefined' ? sessionStorage : null)
  if (!store || !sessionId) return
  try { store.setItem(landedKey(sessionId), '1') } catch { /* local only; nothing else to do */ }
}

/* ── nothing replaces a landed verdict ───────────────────────────────── */

/**
 * The typed identity of the record a verdict was read from: the fields `buildVerdict` reads that a re-read
 * can move. It is a signature, not the record — O(claims + promises + checks), never O(events): an event
 * count and the last event's own typed fields stand for the run of events, which only grows.
 *
 * Closed world, deliberately. A difference is recognised only in these fields; anything else leaves the
 * landed verdict exactly as it is and says nothing, which is the safe direction — the reader is never shown
 * a notice this panel cannot substantiate, and no text is replaced either way.
 */
export function recordSignature(S) {
  if (!S) return ''
  const events = Array.isArray(S.events) ? S.events : []
  const last = events.length ? events[events.length - 1] : null
  const id = (x) => (x && x.id != null ? String(x.id) : '')
  return JSON.stringify([
    S.id == null ? '' : String(S.id),
    S.finalTree || '',
    events.length,
    last ? [last.seq == null ? null : last.seq, last.kind || ''] : null,
    S.eventCount == null ? null : S.eventCount,
    (S.claims || []).map((c) => [id(c), c.verdict || '', c.word || '']),
    (S.promises || []).map((p) => [id(p), p.status || '']),
    (S.checks || []).map((k) => [id(k), k.exit == null ? null : k.exit, k.inputTree || '']),
    (S.asks || []).length,
    !!(S.capture && S.capture.degraded),
    (S.capture && S.capture.condition) || '',
  ])
}

/**
 * Whether a re-read carries a DIFFERENT record from the one the verdict landed on. The same object, and a
 * re-read of the same record under a new object, are both "not changed": a read that returned what was
 * already on screen has moved nothing, and must not raise a notice.
 */
export function recordChanged(landedOn, latest) {
  if (!landedOn || landedOn === latest) return false
  return recordSignature(landedOn) !== recordSignature(latest)
}

/* ── the "Checked:" line ─────────────────────────────────────────────── */

/**
 * The "Checked:" counts, from the phase log only. No log → null. The line's
 * nouns are the log's, and the model's counts are not the same measurements
 * under them (its "1 tree" is the snapshots loaded, beside the log's "1681
 * trees", ADJUDICATIONS item D fourth ruling, finding 2), so they never stand
 * in. A phase the log does not carry is null too, and its unit is not rendered.
 * A count is a unit only when it is a whole, non-negative number. A log with
 * no unit (`phaseLogHasUnit`) is null as well — never "Checked:" with nothing
 * after it and a replay of nothing (ADJUDICATIONS item D fifth ruling,
 * finding 2); the lease and the cache reject such a log before it gets here.
 * `claims` is the log's `find_claims.count` — statements the extractor
 * matched. When the model reports more and the verifier synthesised
 * unclaimed-completion claims, `derived` counts those claims, one chip each —
 * never a bare difference with no receipt (sixth ruling, finding 6).
 */
export function checkedFrom(doc, V) {
  if (!phaseLogHasUnit(doc)) return null
  const units = { events: null, claims: null, checks: null, trees: null, verifiers: null }
  for (const p of doc.phases) if (isUnitCount(p)) units[UNIT_OF[p.phase]] = p.count
  const refs = (V.checked.derivedRefs || []).slice()
  const k = units.claims == null ? 0 : V.counts.reported - units.claims
  return { ...units, derived: k > 0 && refs.length ? { n: refs.length, refs } : null }
}

/**
 * The receipts beside a sentence, as the record's own ids.
 *
 * `receipts="off"` renders none of them, and only the Change workspace asks
 * for that: its surface is the Plain one, where `clm` and `f_test_run_failed`
 * are undefined shorthand (03-USER-LENS.md rule 2), and where there is no
 * proof viewer for a chip to open — a chip with no handler is a control that
 * does nothing, which is worse than no chip. Trace does not pass the prop and
 * is unchanged, chip for chip.
 *
 * The grounding this file computes from `refs` is NOT changed by the prop: a
 * sentence with receipts is still marked as grounded whether or not they are
 * drawn, because whether a line rests on the record is a fact about the line.
 */
function Chips({ refs, onShowProof, receipts }) {
  if (receipts === 'off') return null
  if (!refs || !refs.length) return null
  return <>{refs.map((r) => <button key={r} type="button" className="ws-verdict-chip-ref" onClick={() => onShowProof && onShowProof(r)}>{r}</button>)}</>
}

/** The headline's receipts: up to three chips, then the rest as one count. */
function HeadlineRefs({ refs, onShowProof }) {
  if (!refs || !refs.length) return null
  const shown = refs.slice(0, HEADLINE_CHIPS)
  const more = refs.length - shown.length
  return <> <Chips refs={shown} onShowProof={onShowProof} />{more > 0 ? <span className="ws-verdict-more-refs">{fill(C.more_refs, { n: more })}</span> : null}</>
}

function Sentence({ text, role, refs, onShowProof, receipts }) {
  const inference = role === 'inference' || !refs || !refs.length
  return <div className={`ws-verdict-sentence${inference ? ' ws-verdict-inference' : ''}`}>{text} <Chips refs={refs} onShowProof={onShowProof} receipts={receipts} /></div>
}

/**
 * A line that may quote someone's recorded words: a Said line or a clicked body (verdict-model.js `narrateQuoted`).
 * The template's own words render as the sentence. A quotation part renders in <q>, marked with whose words they are
 * (`data-quote`, and its label from HQ_COPY), exactly as recorded. The label names the speaker in the register's own word
 * (`chrome.quote_label`); a quotation the model marks as only part of what was written (`excerpt`) takes
 * `chrome.quote_label_excerpt` instead, which never claims the whole. The inference class follows the role the model
 * grounded the line with; this component never decides it.
 */
function QuotedLine({ line, register, onShowProof, receipts }) {
  const inference = line.role === 'inference' || !line.refs || !line.refs.length
  const reg = register === 'operator' ? 'operator' : 'manager'
  return <div className={`ws-verdict-sentence${inference ? ' ws-verdict-inference' : ''}`}>{line.parts.map((p, i) => (typeof p.quote === 'string' ? <q key={i} className="ws-verdict-quote" data-quote={p.speaker} title={(p.excerpt === true ? C.quote_label_excerpt : C.quote_label)[p.speaker][reg]}>{p.quote}</q> : <React.Fragment key={i}>{p.text}</React.Fragment>))} <Chips refs={line.refs} onShowProof={onShowProof} receipts={receipts} /></div>
}

function useDispute(sessionKey) {
  const key = `thruline-verdict-dispute:${sessionKey}`
  const [votes, setVotes] = React.useState({})
  React.useEffect(() => {
    try { setVotes(JSON.parse(localStorage.getItem(key) || '{}') || {}) } catch { setVotes({}) }
  }, [key])
  const vote = (cardId, value) => {
    const next = { ...votes, [cardId]: value }
    setVotes(next)
    try { localStorage.setItem(key, JSON.stringify(next)) } catch { /* local only; nothing else to do */ }
  }
  return [votes, vote]
}

/* T-2-4 exports this: the Change workspace's Verify and Review views render
 * the SAME finding card, from the same model, so a card cannot say one thing
 * on one screen and another thing on another. Nothing about the card changed
 * to export it. */
export function Card({ card, primary, register, onShowProof, onCopy, vote, voted, receipts }) {
  const word = HQ_COPY.states[card.state].word
  const kindLabel = card.kind ? HQ_COPY.kinds[card.kind] : null
  /* Technical shows the detector id in a code chip; Plain never shows it. */
  const detector = register === 'operator' && card.detector ? <code className="ws-verdict-detector" title={C.detector_label}>{card.detector}</code> : null
  return <article className={`ws-verdict-card ws-verdict-card-${card.state}`}>
    <div className="ws-verdict-card-head"><span className={`ws-verdict-chip ws-verdict-chip-${card.state}`}>{word}</span>{kindLabel && kindLabel !== word ? <span className="ws-verdict-kind">{kindLabel}</span> : null}{detector}</div>
    {card.said ? <div className="ws-verdict-row"><span className="ws-verdict-rowlabel">{C.rows.said}</span><QuotedLine line={card.said} register={register} onShowProof={onShowProof} receipts={receipts} /></div> : null}
    <div className="ws-verdict-row"><span className="ws-verdict-rowlabel">{C.rows.record}</span><Sentence text={card.record.text} role={card.record.role} refs={card.record.refs} onShowProof={onShowProof} receipts={receipts} /></div>
    <div className="ws-verdict-row"><span className="ws-verdict-rowlabel">{C.rows.do}</span><Sentence text={card.do.text} role={card.do.role} refs={card.do.refs} onShowProof={onShowProof} receipts={receipts} /></div>
    <div className="ws-verdict-card-actions">
      {card.do.prompt ? <button type="button" className={primary ? 'ws-verdict-primary' : 'ws-verdict-secondary'} onClick={() => onCopy && onCopy(card.do.prompt, 'prompt')}>{C.copy_prompt}</button> : null}
      {/* No handler, no button. A "Show proof" that does nothing when clicked is
        * the paper cut this control exists against, and the Change workspace
        * mounts these cards with no proof viewer behind them. */}
      {card.proofNode && onShowProof ? <button type="button" className="ws-text-button ws-verdict-proof" onClick={() => onShowProof(card.proofNode)}>{C.show_proof}</button> : null}
    </div>
    {/* A3-F10 — the same rule as Show proof: no handler, no pair. The Change
      * screen and the whole-story page mount this card with nowhere to keep
      * an answer, and a Yes / No that stays unpressed when pressed is a dead
      * control. The session workspace passes `useDispute`'s vote and keeps it. */}
    {typeof vote === 'function'
      ? <div className="ws-verdict-dispute"><span>{C.was_this_right}</span><button type="button" aria-pressed={voted === 'yes'} onClick={() => vote(card.id, 'yes')}>{C.yes}</button><button type="button" aria-pressed={voted === 'no'} onClick={() => vote(card.id, 'no')}>{C.no}</button></div>
      : null}
  </article>
}

/**
 * The "Checked:" line's place on the landed verdict. The line renders only from
 * a phase log; with none, the reason renders in its place, in both motion
 * settings, with no count and no "what was checked" link — there is nothing to
 * replay (ADJUDICATIONS item D fourth ruling, finding 2):
 *   failed   `phases.failed_landed` with the failure's reason
 *   loading  `phases.checked_loading` — the verdict landed before the log
 *   no id    `phases.failed_landed` with `chrome.no_session_id` — nothing to ask for
 */
export function CheckedSlot({ log, verdict, onReplayChecks, onShowProof }) {
  const checked = log ? checkedFrom(log.doc, verdict) : null
  if (checked) return <CheckedLine checked={checked} onReplayChecks={onReplayChecks} onShowProof={onShowProof} />
  const status = log && log.status
  if (status && status.kind === 'loading') return <p className="ws-verdict-log-loading">{HQ_COPY.phases.checked_loading}</p>
  /* A log with no unit is not in the expected form. The lease and the cache both reject one, so a ready log has a
   * unit; this branch keeps the slot truthful if one ever arrives another way. */
  const reason = !status ? C.no_session_id : status.kind === 'ready' ? HQ_COPY.phases.failed_body : String(status.reason || C.no_reason)
  /* A transport notice, not a finding: its only digits are an HTTP status, so it is styled as inference rather than given a receipt. */
  return <p className="ws-verdict-log-failed ws-verdict-inference">{fill(HQ_COPY.phases.failed_landed, { reason })}</p>
}

export function CheckedLine({ checked, onReplayChecks, onShowProof }) {
  const K = HQ_COPY.phases.checked
  const unit = (key, n) => (n === 1 ? K[`${key}_one`] : fill(K[key], { n }))
  /* A unit the log did not produce is not rendered (`checkedFrom`). */
  const parts = [['events', checked.events], ['claims', checked.claims], ['checks', checked.checks], ['trees', checked.trees], ['verifiers', checked.verifiers]].filter(([, n]) => n != null).map(([key, n]) => unit(key, n))
  const d = checked.derived
  return <p className="ws-verdict-checked"><span className="ws-verdict-checked-label">{HQ_COPY.phases.checked_label}</span> <span className="ws-verdict-checked-units">{parts.join(' · ')}</span>{d ? <> · <span className="ws-verdict-checked-derived">{d.n === 1 ? K.derived_one : fill(K.derived, { n: d.n })} <Chips refs={d.refs} onShowProof={onShowProof} /></span></> : null}{onReplayChecks ? <> · <button type="button" className="ws-text-button ws-verdict-checked-link" onClick={onReplayChecks}>{K.link}</button></> : null}</p>
}

const noop = () => {}

export function VerdictPanel({ session, selected, register: registerProp, onRegister, onShowProof, onCopy, onRecheck, onOpenTrace, onExport, onReplayChecks, reducedMotion: reducedProp }) {
  const [localRegister, setLocalRegister] = React.useState('manager')
  const register = registerProp || localRegister
  const setRegister = (r) => { if (onRegister) onRegister(r); else setLocalRegister(r) }
  const [showAll, setShowAll] = React.useState(false)

  /* The checking sequence. No session id → nothing to fetch → the verdict is
   * already landed (the Checked: line's place then says there is no id to ask
   * for a log by; no model count stands in). A session that landed earlier in
   * this browser session lands at once. Under reduced motion there is no
   * sequence to wait for: the static list and the verdict render together. */
  const reduced = React.useMemo(() => (reducedProp == null ? prefersReducedMotion() : !!reducedProp), [reducedProp])
  const openId = session && session.id ? String(session.id) : null
  const [landed, setLanded] = React.useState(() => !openId || reduced || readLanded(openId))

  /* NOTHING REPLACES A LANDED VERDICT. The session this panel is given is re-read under it while it is on
   * screen — an observation tick re-reads explain and claims and hands down a new one (`shell.jsx`
   * `onObservationUpdate`) — and every sentence here is derived from it, so before this the verdict a reader
   * was looking at was rewritten in place, and a re-read whose claims read failed rewrote it to "reported
   * nothing as done" (STEP0-AUDIT item 4 and its request 14 row). So the session is PINNED at the moment the
   * verdict lands, and the pinned one is what is read from then on: the phase log's key included, since the
   * Checked: line is part of what landed. The pin is released only when the reader asks for the new record
   * (`checkNewRecord`), never by a timer and never by a read. `replayChecks` keeps it: "what was checked"
   * replays the check that was done, not a new one. */
  const [pin, setPin] = React.useState(() => (landed ? { session } : null))
  const latest = React.useRef(session)
  latest.current = session
  const shown = pin ? pin.session : session

  const V = React.useMemo(() => buildVerdict(shown, { register }), [shown, register])
  const [votes, vote] = useDispute(shown && shown.key)
  const state = HQ_COPY.states[V.state]
  const cards = showAll ? V.findings : V.findings.slice(0, SHOWN)
  const clicked = selected ? V.clicked(selected) : null
  const pills = [['reported', V.counts.reported], ['proven', V.counts.proven], ['not_proven', V.counts.not_proven], ['couldnt', V.counts.couldnt]]
  if (V.counts.fog) pills.push(['fog', V.counts.fog])
  const exportSummary = () => { const md = V.exportMarkdown(); if (onExport) onExport(md); else if (onCopy) onCopy(md, 'summary') }

  const sessionId = shown && shown.id ? String(shown.id) : null
  const log = usePhaseLog(sessionId, shown && shown.finalTree)
  const [play, setPlay] = React.useState(0)
  /* N13-FOCUS-2411: the head sticks in .ws-hq-content; that box is padded by
   * the head's measured height so a focused control never scrolls under it.
   * Re-run per `play`, because the head is re-keyed (remounted) on each landing. */
  const headRef = React.useRef(null)
  React.useLayoutEffect(() => padUnderSticky(headRef.current), [play])
  const [replay, setReplay] = React.useState(false)
  const [listHidden, setListHidden] = React.useState(false)
  const [scrollTick, setScrollTick] = React.useState(0)
  const listRef = React.useRef(null)
  const seqStatus = replay && log.doc ? { ...log.status, kind: 'ready', cached: true } : log.status
  /* The pin is taken here, at the landing itself: whatever the sequence was reading when it finished is what
   * the verdict landed on. A replay lands again and keeps the pin it already has. */
  const land = React.useCallback(() => { setPin((p) => p || { session: latest.current }); setLanded(true); setPlay((n) => n + 1); writeLanded(sessionId) }, [sessionId])
  /* The record moved under a landed verdict. Nothing on screen changes for it; the reader is told, and chooses. */
  const changed = React.useMemo(() => landed && !!pin && recordChanged(pin.session, session), [landed, pin, session])
  const checkNewRecord = () => {
    const next = latest.current
    const nextId = next && next.id ? String(next.id) : null
    setReplay(false)
    setListHidden(false)
    /* With no log to fetch, and under reduced motion, a verdict lands at once — as it does on a first open —
     * so the new record is pinned here. Otherwise the sequence runs again and `land` pins what it read. */
    if (!nextId || reduced) { setPin({ session: next }); setPlay((n) => n + 1); writeLanded(nextId) }
    else { setPin(null); setLanded(false) }
  }
  const replayChecks = () => {
    if (reduced) { setListHidden(false); setScrollTick((n) => n + 1) }
    else if (log.doc) { setReplay(true); setLanded(false) }
    if (onReplayChecks) onReplayChecks()
  }
  React.useEffect(() => {
    if (!scrollTick) return
    const el = listRef.current
    if (el && typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'start' })
  }, [scrollTick])

  if (!landed && seqStatus) {
    return <div className="ws-verdict ws-verdict-checking">
      <CheckingSequence key={`${sessionId}:${replay ? 'replay' : 'first'}:${seqStatus.kind}`} session={shown} phases={log.doc ? log.doc.phases : []} status={seqStatus} onDone={land} onSkip={land} reducedMotion={reduced} register={register} />
    </div>
  }

  /* A failed log has no rows to list: its reason renders once, in the Checked: line's place (CheckedSlot). */
  return <div className={`ws-verdict${reduced ? ' ws-verdict-reduced' : ''}`}>
    {reduced && seqStatus && seqStatus.kind !== 'failed' ? <div ref={listRef} className="ws-verdict-static-checks" hidden={listHidden}>
      <CheckingSequence session={session} phases={log.doc ? log.doc.phases : []} status={log.status} onDone={noop} onSkip={() => setListHidden(true)} reducedMotion register={register} />
    </div> : null}
    <header ref={headRef} className={`ws-verdict-head${play > 0 && !reduced ? ' ws-verdict-landing' : ''}`} key={`land${play}`}>
      <div className="ws-verdict-toggle" role="group" aria-label={C.register_label}>{['manager', 'operator'].map((r) => <button key={r} type="button" aria-pressed={register === r} onClick={() => setRegister(r)}>{C.register[r]}</button>)}</div>
      <div className="ws-verdict-state"><span className={`ws-verdict-dot ws-verdict-dot-${V.state}`} aria-hidden="true" /><h2 className="ws-verdict-word">{state.word}</h2>{V.treeMoved && onRecheck ? <button type="button" className="ws-verdict-recheck" onClick={onRecheck}>{C.recheck}</button> : null}</div>
      {/* The headline and the pills count the same claim ids; the chips after
        * the headline are the receipts for both. */}
      <div className="ws-verdict-summary">
        <p className="ws-verdict-headline">{V.headline}<HeadlineRefs refs={V.headlineRefs} onShowProof={onShowProof} /></p>
        <div className="ws-verdict-pills">{pills.map(([k, n]) => <span key={k} className={`ws-verdict-pill ws-verdict-pill-${k}`}>{k === 'fog' && n === 1 ? C.counts.fog_one : fill(C.counts[k], { n })}</span>)}</div>
      </div>
      <CheckedSlot log={log} verdict={V} onReplayChecks={replayChecks} onShowProof={onShowProof} />
      {/* The record moved while this verdict was on screen. It is said, beside the verdict that was checked,
        * and nothing above it is rewritten; the reader decides when the new record is checked. Like the
        * failure notice above it this is a transport notice rather than a finding, so it carries the
        * inference class and no receipt: it points at no record of its own. */}
      {changed ? <p className="ws-verdict-record-changed ws-verdict-inference" role="status">{C.record_changed} <button type="button" className="ws-text-button ws-verdict-record-changed-check" onClick={checkNewRecord}>{C.record_changed_check}</button></p> : null}
    </header>

    {V.findings.length ? <section className="ws-verdict-findings" aria-label={C.went_wrong}>
      <div className="ws-verdict-section-head"><h3>{C.went_wrong}</h3><span>{V.findings.length === 1 ? C.findings_one : fill(C.findings_many, { n: V.findings.length })}</span></div>
      {cards.map((card, i) => <Card key={card.id} card={card} primary={i === 0} register={register} onShowProof={onShowProof} onCopy={onCopy} vote={vote} voted={votes[card.id]} />)}
      {V.findings.length > SHOWN && !showAll ? <button type="button" className="ws-verdict-show-all" onClick={() => setShowAll(true)}>{fill(C.show_all, { n: V.findings.length })}</button> : null}
    </section> : V.holdsUp.length ? <section className="ws-verdict-findings"><div className="ws-verdict-section-head"><h3>{C.everything_holds_up}</h3></div></section> : null}

    {V.holdsUp.length ? <details className="ws-verdict-holds"><summary><span className="ws-verdict-dot ws-verdict-dot-proven" aria-hidden="true" />{V.holdsUp.length === 1 ? C.holds_up_one : fill(C.holds_up_many, { n: V.holdsUp.length })}</summary>
      {V.holdsUp.map((h) => <div key={h.id} className="ws-verdict-holds-line"><q>{h.quote}</q> — <Sentence text={h.record.text} role={h.record.role} refs={h.refs} onShowProof={onShowProof} /></div>)}
    </details> : null}

    {clicked ? <section className="ws-verdict-clicked" aria-label={C.clicked_heading}>
      <div className="ws-verdict-section-head"><h3>{clicked.title}</h3></div>
      <QuotedLine line={clicked.body} register={register} onShowProof={onShowProof} />
      <button type="button" className="ws-text-button ws-verdict-proof" onClick={() => onShowProof && onShowProof(clicked.proofNode)}>{clicked.link}</button>
    </section> : null}

    <section className="ws-verdict-questions" aria-label={HQ_COPY.questions.heading}>
      <div className="ws-verdict-section-head"><h3>{HQ_COPY.questions.heading}</h3></div>
      <dl>{V.questions.map((q) => <div className="ws-verdict-question" key={q.key}><dt>{q.label}</dt><dd className={q.refs.length ? '' : 'ws-verdict-inference'}>{q.answer} <Chips refs={q.refs.slice(0, 3)} onShowProof={onShowProof} /></dd></div>)}</dl>
    </section>

    <footer className="ws-verdict-footer"><button type="button" onClick={exportSummary}>{C.export_summary}</button><button type="button" onClick={onOpenTrace}>{C.open_full_trace}</button></footer>
    <p className="ws-verdict-foot">{C.footer}</p>
  </div>
}

export default VerdictPanel
