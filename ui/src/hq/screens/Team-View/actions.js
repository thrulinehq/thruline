/* Team-View's writes: the only two addresses this screen reaches, each a named
 * constant beside the one control that uses it (seconds.jsx), and
 * `team-view-seconds.test.js` asserts this folder reaches exactly them
 * (C30 R-LEDGER, CLAUDE.md write-address table, rows 22 and 23).
 *
 *   Second this version  →  POST /api/changes/<id>/seconds           (write address 22)
 *   Take my second back  →  POST /api/changes/<id>/seconds/withdraw  (write address 23)
 *
 * Both are FREE: approving a version is the one paid act, and a second never
 * makes an approval stand, never rewrites the decision file and moves no
 * result. The name is the one the person types; ThruLine records it and does
 * not check who is at the keyboard (ruling Q3), and the form says so.
 *
 * `crates/cli/src/uiserve/person_acts.rs` answers one of:
 *   200 {acted: {outcome: "recorded", second | withdrawal}, decisions}   on the record
 *   200 {acted: {outcome: "moved", seen, now, changed[], message}, decisions}
 *        the screen was showing another version than the approval's; nothing recorded
 *   400 {error: "malformed_request" | "actor_required"}                 refused before the record
 *   403 {error: "same_origin_required"}                                  not from HQ's own page
 *   404 {error: "no_such_change"}
 *   409 {error: "nothing_to_second", message}                            no approval stands
 *   409 {error: "same_person", with, message}                            the served sentence, Q3 included
 *   409 {error: "no_second_to_withdraw", message}                        that name has no standing second
 *   409 {error: "too_long", field, limit}                                a name over 200 characters
 *   413 {error: "too_long"}                                              larger than a name within the limit
 *   503 {error: "upgrade_pending", message}                              the store needs `thruline upgrade`
 * and `send` turns a request that got no answer into status 0,
 * `{error: "unreachable"}`. `answerOf` reads every one; none is dropped, and a
 * sentence the server served is shown as served.
 */
import { NAME_NOT_CHECKED, SECOND_NAME_MAX_CHARS } from '../../data/seconds.js'

/** "Second this version": write address 22. */
export const CHANGE_SECONDS = (id) => '/api/changes/' + encodeURIComponent(id) + '/seconds'
/** "Take my second back": write address 23. */
export const CHANGE_SECONDS_WITHDRAW = (id) => '/api/changes/' + encodeURIComponent(id) + '/seconds/withdraw'

async function send(url, body, f) {
  try {
    const res = await f(url, {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    const doc = await res.json().catch(() => null)
    return { ok: res.ok, status: res.status, doc }
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

const NOT_AVAILABLE = { ok: false, status: 0, doc: { error: 'not_available' } }

/** Put this name beside the version the screen showed. Nothing is sent without a Change, a version and a name. */
export function secondVersion({ changeId, seen, actor }, f = fetch) {
  const name = String(actor || '').trim()
  if (!changeId || !seen || !name) return Promise.resolve(NOT_AVAILABLE)
  return send(CHANGE_SECONDS(changeId), { actor: name, seen_version: seen }, f)
}

/** Take this name's own standing second back: a row of its own, nothing edited. */
export function withdrawSecond({ changeId, actor }, f = fetch) {
  const name = String(actor || '').trim()
  if (!changeId || !name) return Promise.resolve(NOT_AVAILABLE)
  return send(CHANGE_SECONDS_WITHDRAW(changeId), { actor: name }, f)
}

/** Plain words for a refusal the server named and did not give a sentence for, drawn after "Not recorded." */
const REFUSAL_WORDS = {
  not_available: 'A second approval needs a Change, the version that is approved and a name.',
  actor_required: 'A second approval has a name on it. Put your name in the box and try again.',
  malformed_request: 'ThruLine could not read the request this page sent. Reload this page and try again.',
  same_origin_required: 'ThruLine records a second approval only from its own page. Reload this page and try again.',
  no_such_change: 'ThruLine has no record of this Change. Reload this page.',
  too_long: 'A name can be at most ' + SECOND_NAME_MAX_CHARS + ' characters. Shorten it and try again.',
  upgrade_pending: 'ThruLine needs an upgrade before it can record anything. Run thruline upgrade in a terminal, then try again.',
  unreachable: 'ThruLine did not answer. Reload this page to see whether anything was recorded.',
}

/** The sentences the server serves, shown exactly as served. */
const SERVED = new Set(['nothing_to_second', 'same_person', 'no_second_to_withdraw'])

/**
 * What one answer means for the form. PURE.
 * @param {{ok: boolean, status: number, doc: any}} res what `secondVersion` / `withdrawSecond` resolved to
 * @returns {{kind: 'recorded', row: object, decisions: object|null}
 *   | {kind: 'moved', detail: string, files: string[]}
 *   | {kind: 'refused', detail: string}}
 */
export function answerOf(res) {
  const r = res && typeof res === 'object' ? res : {}
  if (typeof r.status !== 'number') return { kind: 'refused', detail: REFUSAL_WORDS.unreachable }
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  const acted = doc.acted && typeof doc.acted === 'object' ? doc.acted : null
  if (r.status === 200 && acted && acted.outcome === 'recorded') {
    const row = acted.second || acted.withdrawal
    if (row && typeof row === 'object') {
      return { kind: 'recorded', row, decisions: doc.decisions && typeof doc.decisions === 'object' ? doc.decisions : null }
    }
  }
  if (r.status === 200 && acted && acted.outcome === 'moved') {
    const detail = typeof acted.message === 'string' && acted.message.trim()
      ? acted.message.trim()
      : 'The approved version is not the one this screen was showing, so nothing was recorded. Reload to see the version that is approved.'
    return { kind: 'moved', detail, files: Array.isArray(acted.changed) ? acted.changed.map(String) : [] }
  }
  if (SERVED.has(doc.error) && typeof doc.message === 'string' && doc.message.trim()) {
    return { kind: 'refused', detail: doc.message.trim() }
  }
  if (doc.error === 'same_person') {
    // Served without its sentence: still say the one thing the ruling requires.
    return { kind: 'refused', detail: 'A second approval has to come from a different name. ' + NAME_NOT_CHECKED }
  }
  if (typeof doc.error === 'string' && REFUSAL_WORDS[doc.error]) return { kind: 'refused', detail: REFUSAL_WORDS[doc.error] }
  if (r.status === 413) return { kind: 'refused', detail: REFUSAL_WORDS.too_long }
  if (r.status === 200) return { kind: 'refused', detail: 'ThruLine answered in a form this page cannot read. Reload this page to see what the record holds.' }
  return { kind: 'refused', detail: 'ThruLine could not record this (it answered ' + r.status + '). Reload this page to see what the record holds.' }
}
