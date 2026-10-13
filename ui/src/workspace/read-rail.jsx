import React from 'react'
import { READ, readProjection } from './data.js'
import { readRailModel } from './read-model.js'
import { prefersReducedMotion } from './reduced-motion.js'

/* The Read (P6-2; design record §3.2) — the rail beside the record while it
 * opens.
 *
 * ONE READ, ONE REQUEST. On mount it asks `GET /api/read?session=<id>` once,
 * through `data.js` (the one place this workspace fetches), and renders what
 * `read-model.js` makes of the answer. It writes nothing and reaches no write
 * address.
 *
 * NOTHING HERE IS PACED. There is no schedule, no replay and no minimum
 * display time: the rows appear when the engine's answer does, with the
 * times the engine measured. The only timer is the wall-clock counter shown
 * while the answer is on its way, and only when motion is allowed; it stops
 * the moment the answer lands, and it never appears under reduced motion.
 *
 * Skip is always there. It hides the rail; it does not stop the read, which
 * the record on screen does not depend on.
 */
export function ReadRail({ session, onSkip, reducedMotion }) {
  const reduced = React.useMemo(() => (reducedMotion == null ? prefersReducedMotion() : !!reducedMotion), [reducedMotion])
  const [read, setRead] = React.useState({ status: 'loading' })
  const [waited, setWaited] = React.useState(0)

  React.useEffect(() => {
    let live = true
    setRead({ status: 'loading' })
    readProjection(READ(session)).then((r) => {
      if (!live || !r || r.status === 'idle') return
      setRead(r.status === 'ready' ? { status: 'ready', doc: r.doc } : { status: 'refused', refusal: r.refusal })
    })
    return () => { live = false }
  }, [session])

  const loading = read.status === 'loading'
  React.useEffect(() => {
    if (!loading || reduced) return undefined
    const t0 = Date.now()
    const id = setInterval(() => setWaited(Date.now() - t0), 100)
    return () => clearInterval(id)
  }, [loading, reduced])

  const m = readRailModel(read, { reducedMotion: reduced, waitedMs: waited, address: READ(session) })
  return <ReadRailBody m={m} reduced={reduced} onSkip={onSkip} />
}

/** The rail as the model says it, and nothing else. Pure: no fetch, no timer. */
export function ReadRailBody({ m, reduced, onSkip }) {
  return <aside className="ws-read-rail" aria-label={m.label} data-state={m.state} data-reduced={reduced ? 'yes' : 'no'}>
    <div className="ws-read-head">
      <h2 className="ws-read-title">{m.motion ? <span className="ws-read-spinner" aria-hidden="true" /> : null}{m.title}</h2>
      <button type="button" className="ws-text-button ws-read-skip" aria-label={m.skipLabel} onClick={() => onSkip && onSkip()}>{m.skip}</button>
    </div>
    <p className="ws-read-lead" role="status">{m.lead}</p>
    {m.live ? <p className="ws-read-live">{m.live}</p> : null}
    {m.waiting ? <p className="ws-read-waiting">{m.waiting}</p> : null}
    {m.rows.length
      ? <ol className="ws-read-rows">
        {m.rows.map((r, i) => <li key={i + ':' + r.id} className={'ws-read-row ws-read-' + (r.status || 'unknown')} data-phase={r.id} data-status={r.status}>
          <span className="ws-read-mark" aria-hidden="true" />
          <span className="ws-read-label">{r.label}</span>
          {r.count ? <span className="ws-read-count">{r.count}</span> : null}
          {r.time ? <span className="ws-read-time">{r.time}</span> : null}
          {r.note ? <span className="ws-read-note">{r.note}</span> : null}
        </li>)}
      </ol>
      : null}
    <details className="ws-read-technical">
      <summary>{m.technicalSummary}</summary>
      <dl>{m.technical.map(([k, v], i) => <React.Fragment key={i}><dt>{k}</dt><dd>{v}</dd></React.Fragment>)}</dl>
      {m.note ? <p className="ws-read-covered">{m.note}</p> : null}
    </details>
  </aside>
}

export default ReadRail
