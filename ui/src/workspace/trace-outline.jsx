import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { missingLines, threadSteps } from './trace-view.js'

/* The Outline twin (P6-3; design record §3.3, §6) — the accessible view of
 * the Trace without the canvas.
 *
 * The same cards the picture draws, and the same lit thread, as two ordered
 * lists: the thread first (when one is lit), then every card in record order.
 * Each item says its lane, what it is, where it sits in the record, and the
 * record's links from it by kind — recorded order, documented reference,
 * inferred — in words. Selection is the workspace's own `sel`, so choosing an
 * item here lights the same thread the canvas lights, and the canvas shows it
 * when you go back.
 *
 * Keyboard: one tab stop per list (the selected item, else the first), Up and Down
 * move through the list and select, Home and End go to either end. Every word
 * is `HQ_COPY.trace.outline`; every row arrives already derived
 * (`trace-view.outlineRows`), so this file derives nothing.
 *
 * T-6-4: the Outline is the accessible EQUIVALENT of the picture, not a
 * summary. It is a named region; it names all four lanes with what each
 * holds, empty ones included; a card folded inside a closed group is listed
 * under that group (`trace-view.outlineTree`), so nothing the picture hides in
 * a fold is out of reach; every line from a card is named by its kind in
 * words, all of them, not the first few; and what the thread bar says is a
 * polite live region, so a change of thread is spoken.
 */

const O = HQ_COPY.trace.outline
const A = HQ_COPY.a11y

function Links({ links }) {
  if (!links.length) return <span className="ws-trace-outline-links">{O.links_none}</span>
  return (
    <span className="ws-trace-outline-links">
      {links.map((l) => fill(O.link, { kind: l.word, title: l.title })).join(' · ')}
    </span>
  )
}

/** Every row of a tree, in document order (a group, then what is folded inside it). */
function flat(rows) {
  const out = []
  const walk = (list) => (list || []).forEach((r) => { out.push(r); walk(r.members) })
  walk(rows)
  return out
}

/**
 * @param {object} props
 * @param {Array<object>} props.rows `outlineTree(...)` (or `outlineRows(...)`),
 *   in record order; a row's `members` lists what is folded inside it
 * @param {Array<object>} [props.threadRows] the thread's rows, in thread order
 *   (from the whole record, so a folded card is still listed)
 * @param {null|{ids: string[], missing: string[]}} props.thread the lit thread, or null
 * @param {string|null} props.selected
 * @param {(id: string) => void} props.onSelect
 * @param {() => void} props.onBack back to the picture
 * @param {Array<{lane: string, label: string, count: string}>} [props.lanes]
 *   `trace-view.laneCounts`: the four lanes the picture draws
 * @param {string|null} [props.status] what the thread bar says, spoken politely
 */
export function TraceOutline({ rows, threadRows: given, thread, selected, onSelect, onBack, lanes, status }) {
  const list = Array.isArray(rows) ? rows : []
  const all = flat(list)
  const byId = new Map(all.map((r) => [r.id, r]))
  const threadRows = Array.isArray(given) ? given : thread ? thread.ids.map((id) => byId.get(id)).filter(Boolean) : []
  const lit = new Set(thread ? thread.ids : [])
  /* One tab stop per list (roving): the selected item, else the first. */
  const tabId = selected && byId.has(selected) ? selected : all.length ? all[0].id : null
  const threadTab = threadRows.some((r) => r.id === selected) ? selected : threadRows.length ? threadRows[0].id : null
  const ref = React.useRef(null)

  const move = (e, id, section) => {
    const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End']
    if (!keys.includes(e.key)) return
    e.preventDefault()
    const scope = section === 'thread' ? threadRows : all
    const i = scope.findIndex((r) => r.id === id)
    const j = e.key === 'Home' ? 0 : e.key === 'End' ? scope.length - 1 : Math.max(0, Math.min(scope.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))
    const next = scope[j]
    if (!next) return
    onSelect(next.id)
    const box = ref.current
    const el = box && box.querySelector(`[data-section="${section}"] [data-id="${CSS_ESCAPE(next.id)}"]`)
    if (el && typeof el.focus === 'function') el.focus()
    /* N13-FOCUS-2411: to the nearest edge, instantly; nothing a key triggers animates. */
    if (el && typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'nearest' })
  }

  const item = (r, section, i) => (
    <li key={section + ':' + r.id}>
      <button
        type="button"
        className="ws-trace-outline-item"
        data-id={r.id}
        data-lit={lit.has(r.id) ? 'yes' : undefined}
        aria-current={r.id === selected ? 'true' : undefined}
        tabIndex={(section === 'all' ? tabId : threadTab) === r.id ? 0 : -1}
        onClick={() => onSelect(r.id)}
        onKeyDown={(e) => move(e, r.id, section)}
      >
        <span className="ws-trace-outline-meta">
          {fill(O.lane, { lane: r.laneLabel })} · {r.kind} · {r.where}
          {section === 'thread' ? ' · ' + fill(HQ_COPY.trace.thread.position, { i: i + 1, n: threadRows.length }) : ''}
          {section === 'all' && r.members ? ' · ' + (r.members.length === 1 ? A.outline_folded_one : fill(A.outline_folded, { n: r.members.length })) : ''}
        </span>
        <span className="ws-trace-outline-title">{r.title}</span>
        <Links links={r.links} />
      </button>
      {section === 'all' && r.members
        ? <ol className="ws-trace-outline-folded" aria-label={fill(A.outline_folded_label, { title: r.title })}>{r.members.map((m, k) => item(m, 'all', k))}</ol>
        : null}
    </li>
  )

  return (
    <section className="ws-trace-outline" role="region" aria-label={O.label} ref={ref} data-count={all.length}>
      <h2>{O.title}</h2>
      <p className="ws-trace-outline-intro">{O.intro}</p>
      <p className="ws-trace-outline-intro">{O.keys}</p>
      <button type="button" className="ws-trace-outline-back" onClick={onBack}>{O.back}</button>

      {Array.isArray(lanes) && lanes.length
        ? <>
          <h3 id="ws-trace-outline-lanes">{A.outline_lanes_heading}</h3>
          <ul className="ws-trace-outline-lanes" aria-labelledby="ws-trace-outline-lanes">
            {lanes.map((l) => <li key={l.lane} data-lane={l.lane}>{fill(A.outline_lane, { lane: l.label, count: l.count })}</li>)}
          </ul>
        </>
        : null}

      <h3 id="ws-trace-outline-thread">{O.thread_heading}{threadRows.length ? ' · ' + threadSteps(threadRows.length) : ''}</h3>
      <p className="ws-trace-outline-status" role="status" aria-live="polite">{status || ''}</p>
      {threadRows.length ? (
        <>
          <ol aria-labelledby="ws-trace-outline-thread" data-section="thread">{threadRows.map((r, i) => item(r, 'thread', i))}</ol>
          {missingLines(thread).map((m) => (
            <p key={m} className="ws-trace-outline-missing">{m}</p>
          ))}
        </>
      ) : (
        <p className="ws-trace-outline-missing">{O.thread_none}</p>
      )}

      <h3 id="ws-trace-outline-all">{O.all_heading}</h3>
      <p className="ws-trace-outline-intro">{A.outline_order_note}</p>
      <ol aria-labelledby="ws-trace-outline-all" data-section="all">{list.map((r, i) => item(r, 'all', i))}</ol>
    </section>
  )
}

/* An id inside an attribute selector. `CSS.escape` where the platform has it. */
function CSS_ESCAPE(id) {
  if (typeof CSS !== 'undefined' && CSS && typeof CSS.escape === 'function') return CSS.escape(id)
  return String(id).replace(/["\\]/g, '\\$&')
}

export default TraceOutline
