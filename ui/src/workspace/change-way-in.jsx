import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { readInbox } from './inbox.jsx'
import { START_HASH } from '../routes.js'
import './workspace.css'

/* The door to the Change workspace (T-2-4, fix round 1).
 *
 * WHY THIS FILE EXISTS. T-2-4 built a Change screen that nothing led to. The
 * landing was the dense Trace view, the word "Change" did not appear on it,
 * and the only way to the screen was to type `#/changes/<id>` — which means
 * knowing an id nothing showed you. The verifier confirmed it against the
 * rendered landing (zero occurrences of "changes" in the DOM) and failed the
 * card on it. A surface with no door is Sam's "gives up" failure mode
 * (`03-USER-LENS.md` §2), and it is worse than the confusion it replaced.
 *
 * WHAT IT IS. One strip at the top of the landing: a heading, how many Changes
 * are open and how many of them need your attention (the Inbox's own number,
 * A3-F14), and one row per Change — its title and the sentence the Change
 * screen's own headline states, read from the ONE derived model the Inbox
 * built for it, so the two can never disagree. Each row is a link to
 * `#/changes/<id>`.
 *
 * A3-F14: this strip used to decide each row's sentence itself, with the
 * evidence hard-coded empty, so a Change whose every accepted line was proven
 * read "None has evidence yet" here and "3 of 3 … have evidence" on its own
 * screen; and it counted every open Change beside an Inbox that counted the
 * ones needing a person. It now derives nothing: it reads through the Inbox's
 * reader (`readInbox`) and prints what that decided.
 *
 * WHAT IT IS NOT. It is not the Inbox (`B8.1`, `B8.4`): it ranks nothing,
 * scores nothing, and hides nothing behind "what matters". It lists what is
 * open, newest first, and says plainly when there are more than it shows.
 *
 * RULE 12. With nothing open it does not disappear and it does not say
 * "no data": it says what would put something there. Since T-6-6 its one
 * primary is the pick screen (`#/start`), which proposes a Change from the
 * record; the exact command (`thruline change new`) stays beside it as the
 * second way, with a control that copies it.
 *
 * RULE 1 AND RULE 2. Every string is `HQ_COPY.change.wayin`, except the
 * Change's own title, which is the record speaking. Nothing is green, nothing
 * is a badge, and no state word appears here.
 *
 * IT READS. Through `readInbox` and nothing else: the same GETs the Inbox
 * makes, already masked before they are served. It writes nothing; there is
 * no POST in this file.
 */

const C = HQ_COPY.change.wayin

/** How many Changes are listed before the rest are counted in a sentence. */
export const SHOWN = 5

/**
 * The rows and the count, from what the Inbox's reader returned and nothing
 * else (A3-F14).
 *
 * `inbox.changes` is every open Change, newest first, carrying the ONE
 * derived model for each Change that was read. A row's sentence is that
 * model's headline — the sentence the Change's own screen states — and a row
 * whose Change was not read carries none of its own. `needing` is the Inbox's
 * own count of Changes that need a person, over the ones `looked` at.
 *
 * @param {object} inbox what `readInbox` / `assembleInbox` returned
 * @returns {{rows: Array<{id: string, title: string, conclusion: ?string}>, more: number,
 *   total: number, needing: number, looked: number, unread: number}}
 */
export function wayInRows(inbox) {
  const all = Array.isArray(inbox && inbox.changes) ? inbox.changes.filter((c) => c && typeof c.id === 'string' && c.id) : []
  const rows = all.slice(0, SHOWN).map((c) => ({
    id: String(c.id),
    title: typeof c.title === 'string' ? c.title.trim() : '',
    conclusion: c.hq && c.hq.headline && typeof c.hq.headline.text === 'string' ? c.hq.headline.text : null,
  }))
  const needing = Array.isArray(inbox && inbox.items) ? inbox.items.length : 0
  const settled = inbox && typeof inbox.settled === 'number' ? inbox.settled : 0
  const unread = inbox && typeof inbox.unread === 'number' ? inbox.unread : 0
  return { rows, more: all.length - rows.length, total: all.length, needing, looked: needing + settled, unread }
}

/**
 * The one count sentence: how many are open and how many need you, and, when
 * not every Change was looked at, how many were (never a total it did not
 * read).
 */
export function countSentence({ total, needing, looked }) {
  const K = C.count
  if (looked < total) {
    if (!looked) return fill(K.part_none, { total })
    if (!needing) return fill(K.part_settled, { total, looked })
    return needing === 1 ? fill(K.part_needs_one, { total, looked }) : fill(K.part_needs, { total, looked, n: needing })
  }
  if (total === 1) return needing ? K.one_needs : K.one_settled
  if (!needing) return fill(K.many_settled, { total })
  return needing === 1 ? fill(K.many_needs_one, { total }) : fill(K.many_needs, { total, n: needing })
}

/** The address of one Change. The one place this file builds a link. */
export function changeHref(id) {
  return '#/changes/' + encodeURIComponent(id)
}

/**
 * @param {object} props
 * @param {{status: string, rows?: object[], more?: number, refusal?: string}} props.state
 * @param {(text: string) => void} [props.onCopy]
 */
export function ChangeWayIn({ state, onCopy }) {
  if (state.status === 'loading') {
    return <section className="ws-wayin"><p className="ws-wayin-muted">{C.reading}</p></section>
  }
  if (state.status === 'refused') {
    /* Rule 12: a read that failed is said, with the reader's own reason, and
     * nothing is claimed about what is or is not waiting. */
    return <section className="ws-wayin"><p className="ws-wayin-muted">{fill(C.failed, { reason: state.refusal || '' })}</p></section>
  }
  const rows = state.rows || []
  if (!rows.length) {
    /* T-6-6 (`HQ_COPY.empty.change_list`): what this strip is, why it is
     * empty, and the one thing that fills it. The primary is the in-product
     * pick screen, the same door the empty Inbox offers; the command is the
     * second way, and secondary. */
    const E = HQ_COPY.empty.change_list
    return <section className="ws-wayin" data-empty="change_list">
      <h2 className="ws-wayin-heading">{C.heading}</h2>
      <p className="ws-wayin-muted">{E.what}</p>
      <p className="ws-wayin-empty">{C.empty}</p>
      <p className="ws-wayin-empty">{E.fill}</p>
      <a className="ws-wayin-primary" href={START_HASH}>{E.action}</a>
      <p className="ws-wayin-muted">{C.empty_how}</p>
      <p className="ws-wayin-command"><code>{C.command}</code></p>
      <button type="button" className="ws-first-secondary" onClick={() => onCopy && onCopy(C.command)}>{C.copy_command}</button>
    </section>
  }
  const more = state.more || 0
  const unread = state.unread || 0
  const I = HQ_COPY.change.inbox
  return <section className="ws-wayin">
    <h2 className="ws-wayin-heading">{C.heading}</h2>
    <p className="ws-wayin-muted ws-wayin-count">{countSentence({ total: state.total || 0, needing: state.needing || 0, looked: state.looked || 0 })}</p>
    {unread ? <p className="ws-wayin-muted">{unread === 1 ? I.unread_one : fill(I.unread_many, { n: unread })}</p> : null}
    <ul className="ws-wayin-list">
      {rows.map((r) => <li key={r.id}>
        <a className="ws-wayin-row" href={changeHref(r.id)}>
          <span className="ws-wayin-title">{r.title}</span>
          <span className="ws-wayin-conclusion">{r.conclusion == null ? C.unread_row : r.conclusion}</span>
          <span className="ws-wayin-open">{C.open}</span>
        </a>
      </li>)}
    </ul>
    {more ? <p className="ws-wayin-muted">{more === 1 ? C.more_one : fill(C.more_many, { n: more })}</p> : null}
  </section>
}

/** The same strip, reading through the Inbox's reader. What the landing mounts. */
export default function ChangeWayInLive() {
  const [state, setState] = React.useState({ status: 'loading' })
  React.useEffect(() => {
    let live = true
    readInbox().then((r) => {
      if (!live) return
      if (r.status === 'refused') return setState({ status: 'refused', refusal: r.refusal })
      setState({ status: 'ready', ...wayInRows(r) })
    })
    return () => { live = false }
  }, [])
  const copy = (text) => { try { navigator.clipboard.writeText(text) } catch { /* nothing else to do */ } }
  return <ChangeWayIn state={state} onCopy={copy} />
}
