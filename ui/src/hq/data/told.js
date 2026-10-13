/* What a person told the agent, as the screens draw it (C30 R-LEDGER).
 *
 *   toldOf(servedToldAgent, now) → null | {act, actId, at, when, versionId, laterWork, standing, waiting}
 *
 * PURE: no fetch, no window, no clock of its own — `now` is passed in. The
 * input is a thread's `told_agent` as `GET /api/hq/projects/<id>` serves it
 * (`crates/core/src/change_agent_message.rs`, `ToldAgent`): the thread's LAST
 * act — `copied`, `marked_pasted` or `unmarked_pasted` —, when, at which
 * version, which row, and whether the thread has had any work recorded since
 * (`later_work`, derived by the server at read, never stored). The key is absent
 * when the thread has no act, and then this returns null: nothing is made up.
 *
 * NOTHING IS SENT. The record has no act that could say a message was
 * delivered, and no function here says one was. A copy and a mark are the
 * person's own words about their own clipboard. Neither moves a verdict, a
 * count, `whose_move`, readiness or an approval.
 *
 * Writing an act is not here. Its one address is declared in told-actions.js.
 */

const pad = (n) => String(n).padStart(2, '0')
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** The sentence every surface that shows a copy or a mark says beside it
 *  (`NOTHING_SENT` in change_agent_message.rs; told.test.js holds the two together). */
export const NOTHING_SENT = 'Nothing was sent. ThruLine only keeps that you copied this and, if you say so, that you pasted it.'

/** The longest message the record keeps, in characters (`MAX_MESSAGE_CHARS`). */
export const TOLD_MESSAGE_MAX_CHARS = 8000

/**
 * A recorded UTC moment as the local time a person reads: "16:12" for today,
 * "16:12 on 7 Oct" for another day (the year only when it is not this one).
 * `''` when it is not a time. Never the word "yesterday" — "at yesterday" is not a time.
 */
export function toldWhen(iso, now) {
  const t = typeof iso === 'string' && iso ? Date.parse(iso) : NaN
  if (Number.isNaN(t)) return ''
  const d = new Date(t)
  const n = now instanceof Date ? now : new Date(now == null ? Date.now() : now)
  const clock = pad(d.getHours()) + ':' + pad(d.getMinutes())
  const sameDay = d.getFullYear() === n.getFullYear() && d.getMonth() === n.getMonth() && d.getDate() === n.getDate()
  if (sameDay) return clock
  const day = d.getDate() + ' ' + MONTHS[d.getMonth()]
  return clock + ' on ' + day + (d.getFullYear() === n.getFullYear() ? '' : ' ' + d.getFullYear())
}

/**
 * The last told-the-agent act of a thread, as the screens read it.
 * @param {object|null|undefined} doc  the served `told_agent`
 * @param {Date|number} now
 * @returns {null | {act: 'copied'|'marked_pasted'|'unmarked_pasted', actId: string, at: string, when: string,
 *   versionId: string|null, laterWork: boolean|null, standing: boolean, waiting: boolean}}
 *   `standing`: the last act is a copy or a pasted mark.
 *   `waiting`: it is, and no work has been recorded on the thread since — the Inbox's "Waiting on the agent".
 */
export function toldOf(doc, now) {
  if (!doc || typeof doc !== 'object') return null
  const act = doc.last_act
  if (act !== 'copied' && act !== 'marked_pasted' && act !== 'unmarked_pasted') return null
  const laterWork = typeof doc.later_work === 'boolean' ? doc.later_work : null
  const standing = act === 'copied' || act === 'marked_pasted'
  return {
    act,
    actId: typeof doc.act_id === 'string' ? doc.act_id : '',
    at: typeof doc.at === 'string' ? doc.at : '',
    when: toldWhen(doc.at, now),
    versionId: typeof doc.version_id === 'string' && doc.version_id ? doc.version_id : null,
    laterWork,
    standing,
    // an unknown "later work" never files a thread as waiting
    waiting: standing && laterWork === false,
  }
}

/** The rung "Told the agent", in the words both ladders use. `told` is a
 *  `toldOf` result or null; the rung is reached while a copy or a mark stands.
 *  @returns {{reached: boolean, value: string}} `value` is '' for no act (the caller says its own "not yet") */
export function toldRung(told) {
  if (!told) return { reached: false, value: '' }
  const at = told.when ? ' · ' + told.when : ''
  if (told.act === 'marked_pasted') return { reached: true, value: 'You said you pasted it' + at }
  if (told.act === 'copied') return { reached: true, value: 'Copied — not sent' + at }
  // a pasted mark taken back: the copy happened, the paste is no longer claimed
  return { reached: true, value: 'You took back “pasted”' + at + ' · not sent' }
}
