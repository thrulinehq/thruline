import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

/* HQ Verdict Panel — the checking sequence (brief §10, plan Step 3).
 *
 * An honest instrument: the rows are EXACTLY the phases `/api/phases`
 * returned, in their order, with their counts, details and measured wall
 * times. No row is invented, no row is dropped (an unknown phase id renders
 * its raw id), and no timer is independent of the data: each row's display
 * time is its recorded `elapsed_ms`, scaled so the whole sequence fits the
 * cap (600 ms when the log came from the browser cache, 1500 ms on a fresh
 * fetch) by one factor, and never padded (see below). The engine has no
 * streaming transport (docs/UI-SERVER-CONTRACT.md §2.1.1), so this replays
 * the completed run's log; it never pretends to watch one.
 *
 * No timer pads a row (brief §10 rule 4, ruled at ADJUDICATIONS item E): a
 * phase recorded at 0 ms ticks at once, and its tick is still rendered.
 *
 * Unknown stays unknown. The engine writes `elapsed_ms: null` when it could
 * not measure a phase (§2.1.1); a row whose time is not a whole, non-negative
 * number has no schedule (`schedule` returns null, never a stand-in 0), and a
 * log carrying one is refused where a log is accepted — the lease and the
 * cache (`phaseLogHasTiming`) — so it lands as a failure with its own reason.
 *
 * The verdict does not depend on this. A failed fetch lands the verdict at
 * once (`landing` returns 'immediate' and the effect below calls onDone), so
 * the reason this component renders for a failed status is not what a user
 * reads: the landed verdict carries it, in the "Checked:" line's place, in
 * both motion settings (verdict-panel.jsx `CheckedSlot`), and the panel does
 * not show this list for a failed log under reduced motion.
 * `prefers-reduced-motion` renders a static list, every row ticked, no
 * timers. Skip is always available. */

export const PHASES_CONTRACT = 'thruline.phases.v1'
export const CACHED_CAP_MS = 600
export const FRESH_CAP_MS = 1500

const P = HQ_COPY.phases

/** A row whose measured time the log carries: a whole, non-negative number of milliseconds. `null` is the engine's unknown. */
export function isTimed(p) {
  return !!p && Number.isInteger(p.elapsed_ms) && p.elapsed_ms >= 0
}

/* ── the schedule: pure ─────────────────────────────────────────────── */

/**
 * Display durations, one per phase, in order.
 *   sum ≤ cap (600 cached / 1500 fresh); each is its recorded elapsed_ms,
 *   scaled by one factor when the recorded total exceeds the cap, so the
 *   ordering of recorded times is preserved and a 0 ms phase gets 0 ms.
 *   Recorded times under the cap play at their real length — the sequence is
 *   never stretched to fill it, and no row is padded.
 *   null when any row's time is unknown (`isTimed`): no schedule is built from
 *   a stand-in, and a 0 would state a measurement the log does not have.
 * @param {Array<{elapsed_ms?: number|null}>} phases
 * @param {{cached?: boolean}} [opts]
 * @returns {number[]|null}
 */
export function schedule(phases, opts = {}) {
  const rows = Array.isArray(phases) ? phases : []
  if (rows.length === 0) return []
  if (!rows.every(isTimed)) return null
  const cap = opts.cached ? CACHED_CAP_MS : FRESH_CAP_MS
  const recorded = rows.map((p) => p.elapsed_ms)
  const total = recorded.reduce((a, b) => a + b, 0)
  const k = total > cap ? cap / total : 1
  return recorded.map((ms) => ms * k)
}

/**
 * What the component does with a status. Pure, so it can be tested without a DOM.
 *   'play'      the log is here, every row is timed and motion is allowed: tick rows on the schedule, then land
 *   'immediate' land now — reduced motion (static list), a failed fetch, an empty log, or rows with no schedule
 *   'wait'      the fetch is in flight
 * @param {{kind: 'loading'|'ready'|'failed'}} status
 * @param {boolean} reducedMotion
 * @param {number} rowCount
 * @param {boolean} [timed] false when a row's time is unknown — there is nothing to play it by
 */
export function landing(status, reducedMotion, rowCount, timed = true) {
  const kind = status && status.kind
  if (kind === 'loading') return 'wait'
  if (kind === 'failed') return 'immediate'
  if (reducedMotion || !rowCount || !timed) return 'immediate'
  return 'play'
}

/* ── what counts as a phase log ─────────────────────────────────────── */

/** The phases the "Checked:" line reads, each to its unit. */
export const UNIT_OF = { read_record: 'events', find_claims: 'claims', match_proof: 'checks', workspace_since: 'trees', run_verifiers: 'verifiers' }

/** A row the "Checked:" line can count: one of its phases, with a whole, non-negative count. */
export function isUnitCount(p) {
  return !!p && typeof p.phase === 'string' && Object.prototype.hasOwnProperty.call(UNIT_OF, p.phase) && Number.isInteger(p.count) && p.count >= 0
}

/**
 * Whether a phase log carries at least one countable row. A log with none —
 * empty, unknown ids only, `write_up` only, counts that are not whole numbers —
 * is not the six phases §2.1.1 promises. The lease and the cache both reject
 * it, so it is a failure everywhere: no rows listed or played beside "could
 * not be loaded", nothing cached (ADJUDICATIONS item D sixth ruling, finding 1).
 */
export function phaseLogHasUnit(doc) {
  return !!doc && Array.isArray(doc.phases) && doc.phases.some(isUnitCount)
}

/**
 * Whether every row of a phase log carries its measured time (`isTimed`). The
 * engine writes `elapsed_ms: null` for a phase it could not time; such a log
 * is refused where a log is accepted, beside `phaseLogHasUnit` — the lease
 * rejects it with `phases.failed_timing` and the cache ignores it — so it
 * lands through the failure notice and is never replayed on a coerced 0.
 */
export function phaseLogHasTiming(doc) {
  return !!doc && Array.isArray(doc.phases) && doc.phases.every(isTimed)
}

/* ── the browser cache (session + final tree) ───────────────────────── */

export function phasesCacheKey(sessionId) {
  return `thruline-phases:${sessionId}`
}

export function readPhaseCache(sessionId, finalTree, storage) {
  const store = storage || (typeof sessionStorage !== 'undefined' ? sessionStorage : null)
  if (!store || !sessionId) return null
  try {
    const raw = store.getItem(phasesCacheKey(sessionId))
    if (!raw) return null
    const doc = JSON.parse(raw)
    if (!doc || doc.contract !== PHASES_CONTRACT || !phaseLogHasUnit(doc) || !phaseLogHasTiming(doc)) return null
    if (!finalTree || doc.final_tree_hash !== finalTree) return null
    return doc
  } catch { return null }
}

export function writePhaseCache(sessionId, finalTree, doc, storage) {
  const store = storage || (typeof sessionStorage !== 'undefined' ? sessionStorage : null)
  if (!store || !sessionId || !finalTree || !doc) return
  try { store.setItem(phasesCacheKey(sessionId), JSON.stringify({ ...doc, final_tree_hash: finalTree })) } catch { /* cache only */ }
}

/* ── row copy ───────────────────────────────────────────────────────── */

export function phaseLabel(phase) {
  const id = phase && phase.phase
  return (id && P.labels[id]) || String(id == null ? '' : id)
}

export function phaseCount(phase) {
  const id = phase && phase.phase
  const n = phase && phase.count
  /* A count the log did not carry as a whole, non-negative number is not shown — never coerced to 0, which would
   * state a count the log does not have ("0 trees" for a `"1681"`; ADJUDICATIONS item D seventh ruling, item E addendum). */
  if (!Number.isInteger(n) || n < 0) return ''
  if (id && P.counts[id]) return n === 1 ? P.counts[`${id}_one`] : fill(P.counts[id], { n })
  return String(n)
}

function shortTree(hash) {
  return typeof hash === 'string' && hash.length > 8 ? hash.slice(0, 8) : hash || ''
}

/* ── phase details: verifier and claim ids by register ──────────────── */

/**
 * Phases whose engine detail only restates the row's own count, in a noun
 * the count has outgrown (A3-NOTES-NEVER-OPEN). The engine's `write_up`
 * detail is "{n} findings", over every severity — findings and notes
 * together — and the row already states that number, named truthfully, from
 * the copy file. Showing both would put the untruthful name beside the
 * truthful one. The engine's text is not altered; this row does not repeat
 * it. NEEDS ENGINE: a per-severity breakdown in the phase detail.
 */
export const COUNT_RESTATED = Object.freeze(['write_up'])

/** A verifier id as the engine spells it: `name@version`. */
const VERIFIER_ID = /([a-z][a-z0-9_]*@\d+)/
/** A claim id as the engine spells it: `clm_` and its derived id. The phase log names claims by id, never by their text. */
const CLAIM_ID = /^clm_[0-9A-Za-z]+$/
const DETAIL_ID = /(clm_[0-9A-Za-z]+|[a-z][a-z0-9_]*@\d+)/
/** The separator the engine joins ids with. */
const ID_SEPARATOR = ' · '

/** The Plain-register phrase for a verifier id; an unknown id is spelled out, never dropped. */
export function verifierPlain(id) {
  const name = String(id == null ? '' : id).replace(/@\d+$/, '')
  return HQ_COPY.chrome.verifiers[name] || name.replace(/_/g, ' ')
}

/**
 * A phase's detail, with the two id shapes `DETAIL_ID` recognises rendered for
 * the register: verifier ids (`name@N`) and claim ids (`clm_…`). Technical
 * shows each in a code chip. Plain shows a verifier id as its phrase and leaves
 * a claim id out with the separator that joined it — the row's count already
 * says how many statements there were, and `find_claims` names claims by id
 * only (§2.1.1), so a detail of claim ids alone renders nothing in Plain.
 * No other id is recognised. Everything else is the engine's own text, passed
 * through as written in both registers — including the session id in the
 * `read_record` detail, which Plain shows raw and Technical shows without a
 * chip. The log is not altered — only its rendering.
 */
function Detail({ detail, register }) {
  const parts = String(detail).split(DETAIL_ID)
  const operator = register === 'operator'
  const out = []
  parts.forEach((part, i) => {
    if (i % 2 === 0) { if (part) out.push(part); return }
    if (CLAIM_ID.test(part)) {
      if (operator) out.push(<code key={i} className="ws-checking-verifier ws-checking-claim">{part}</code>)
      else if (out.length && out[out.length - 1] === ID_SEPARATOR) out.pop()
      return
    }
    out.push(operator ? <code key={i} className="ws-checking-verifier">{part}</code> : <span key={i}>{verifierPlain(part)}</span>)
  })
  /* Plain: a separator left at either end by a dropped claim id goes with it. */
  if (!operator) {
    while (out.length && out[0] === ID_SEPARATOR) out.shift()
    while (out.length && out[out.length - 1] === ID_SEPARATOR) out.pop()
  }
  return out.length ? <span className="ws-checking-detail">{out}</span> : null
}

/* ── the component ──────────────────────────────────────────────────── */

export function CheckingSequence({ session, phases, status, onDone, onSkip, reducedMotion, register }) {
  const rows = Array.isArray(phases) ? phases : []
  const S = session || {}
  /* A row the log could not time has no schedule: such a log lands at once, never paced on a stand-in. */
  const mode = landing(status, !!reducedMotion, rows.length, rows.every(isTimed))
  const cached = !!(status && status.cached)
  const [ticked, setTicked] = React.useState(mode === 'immediate' ? rows.length : 0)
  const [elapsed, setElapsed] = React.useState(0)
  const doneRef = React.useRef(onDone)
  doneRef.current = onDone

  /* Playback. One timer chain over the schedule; the last tick lands. */
  React.useEffect(() => {
    if (mode === 'wait') return undefined
    if (mode === 'immediate') {
      setTicked(rows.length)
      if (doneRef.current) doneRef.current()
      return undefined
    }
    const durations = schedule(rows, { cached })
    let i = 0
    let timer = null
    setTicked(0)
    const step = () => {
      if (i >= durations.length) { if (doneRef.current) doneRef.current(); return }
      timer = setTimeout(() => { i += 1; setTicked(i); step() }, durations[i])
    }
    step()
    return () => { if (timer) clearTimeout(timer) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, cached, rows.length, phases])

  /* The elapsed counter: real wall time since this sequence started, while
   * the fetch is in flight or the rows are ticking. No timers under reduced motion. */
  React.useEffect(() => {
    if (reducedMotion || mode === 'immediate') return undefined
    const t0 = Date.now()
    const id = setInterval(() => setElapsed((Date.now() - t0) / 1000), 100)
    return () => clearInterval(id)
  }, [mode, reducedMotion])

  const agent = S.harness || S.harnessRaw || HQ_COPY.chrome.agent_fallback_lower
  const sub = [S.label || S.id, S.finalTree ? fill(P.sub_tree, { hash: shortTree(S.finalTree) }) : null, S.harnessRaw && S.harnessRaw !== agent ? S.harnessRaw : null].filter(Boolean).join(' · ')
  const failed = status && status.kind === 'failed'
  const unparsed = Array.isArray(status && status.unparsed) ? status.unparsed.length : 0
  const cls = `ws-checking${mode === 'immediate' ? ' ws-checking-static' : ''}${failed ? ' ws-checking-failed' : ''}`

  return <section className={cls} role="status" aria-live="polite" aria-label={fill(P.title, { agent })}>
    <h2 className="ws-checking-title"><span className={`ws-checking-spinner${mode === 'wait' || (mode === 'play' && ticked < rows.length) ? ' ws-checking-spinning' : ''}`} aria-hidden="true" />{fill(P.title, { agent })}</h2>
    {sub ? <p className="ws-checking-sub">{sub}</p> : null}
    {failed ? <p className="ws-checking-reason">{fill(P.failed, { reason: String(status.reason || HQ_COPY.chrome.no_reason) })}</p> : null}
    <ol className="ws-checking-rows">
      {rows.map((phase, i) => {
        const state = i < ticked ? 'done' : i === ticked && mode === 'play' ? 'active' : 'pending'
        return <li key={`${i}:${phase && phase.phase}`} className={`ws-checking-row ws-checking-${state}`} data-phase={phase && phase.phase}>
          <span className="ws-checking-ic" aria-hidden="true">{state === 'done' ? '✓' : ''}</span>
          <span className="ws-checking-label">{phaseLabel(phase)}</span>
          <span className="ws-checking-count">{phaseCount(phase)}</span>
          {phase && phase.detail && !COUNT_RESTATED.includes(phase.phase) ? <Detail detail={phase.detail} register={register} /> : null}
        </li>
      })}
    </ol>
    {unparsed ? <p className="ws-checking-reason">{fill(P.unparsed, { n: unparsed })}</p> : null}
    <div className="ws-checking-foot">
      <span className="ws-checking-elapsed">{mode === 'immediate' || reducedMotion ? '' : fill(P.elapsed, { n: elapsed.toFixed(1) })}</span>
      <button type="button" className="ws-text-button ws-checking-skip" onClick={() => { if (onSkip) onSkip(); else if (onDone) onDone() }}>{P.skip}</button>
    </div>
  </section>
}

export default CheckingSequence
