/* The one write for "told the agent" (C30 R-LEDGER), shared by the two screens
 * that offer it (Repair-Loop's Copy message / Mark as pasted, and the Brief's
 * Tell the agent).
 *
 *   POST /api/hq/projects/<project>/threads/<thread>/told-agent   (write address 24, CLAUDE.md)
 *
 * Declared once, here, and `told-actions.test.js` asserts this is the only
 * file under `ui/src/hq` that POSTs to it. The body is `{act, message?, refers_to?}`
 * and nothing else: the thread's anchor, the version it is at and the Change
 * that tracks it are read by the SERVER, and the digest of a message is its
 * too. The name on the act is the account ThruLine is running as.
 *
 * NOTHING IS SENT. The record has three acts — copied, marked_pasted,
 * unmarked_pasted — and none that could say a message was delivered; the
 * clipboard write is the browser's, on the person's click. The act is free
 * (approving a version is the one paid act) and moves no verdict, count,
 * `whose_move`, readiness or approval.
 *
 * `crates/cli/src/uiserve/person_acts.rs` `told_agent` answers one of:
 *   200 {schema_version, act, told_agent, nothing_sent, message}   kept
 *   400 {error: "malformed_request" | "unknown_act" | "message_required"}
 *   403 {error: "same_origin_required"}
 *   404 {error: "no_such_project" | "no_such_thread"}
 *   409 {error: "too_long", field: "message", limit}
 *   409 {error: "no_copy_to_mark" | "already_marked" | "no_mark_to_undo", message}
 *   413 {error: "too_long"}
 *   503 {error: "upgrade_pending" | "store_busy", message}
 * and `send` turns a request that got no answer into status 0,
 * `{error: "unreachable"}`. `toldAnswerOf` reads every one; none is dropped.
 */

/** "Copy message" / "Tell the agent" / "Mark as pasted · undo": write address 24. */
export const TOLD_AGENT = (project, thread) =>
  '/api/hq/projects/' + encodeURIComponent(project) + '/threads/' + encodeURIComponent(thread) + '/told-agent'

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

/**
 * Keep one act on one thread. `act` is `copied` (needs `message`, kept as
 * written), `marked_pasted` or `unmarked_pasted`. `refersTo` names the row the
 * screen last saw (the copy a mark is about, the mark an undo takes back); when
 * it is absent the server means the latest. Nothing is sent without the work on
 * screen, and a copy is not sent without words.
 */
export function recordTold({ project, thread, act, message, refersTo }, f = fetch) {
  if (!project || !thread) return Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })
  if (act === 'copied') {
    if (typeof message !== 'string' || !message.trim()) return Promise.resolve({ ok: false, status: 0, doc: { error: 'message_required' } })
    return send(TOLD_AGENT(project, thread), { act, message }, f)
  }
  if (act !== 'marked_pasted' && act !== 'unmarked_pasted') return Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })
  return send(TOLD_AGENT(project, thread), refersTo ? { act, refers_to: refersTo } : { act }, f)
}

/** What was NOT done, in words, whatever the refusal: said after every one. */
const AND_NOTHING = ' Nothing was sent.'

/** Plain words for a refusal the server named. Each says nothing was saved and nothing was sent. */
const REFUSAL_WORDS = {
  not_available: 'ThruLine cannot tell which work is on screen, so nothing was saved. Reload this page and try again.',
  message_required: 'There is no message to keep: the draft is empty, so nothing was saved.',
  malformed_request: 'ThruLine could not read the request this page sent, so nothing was saved. Reload this page and try again.',
  unknown_act: 'ThruLine could not read the request this page sent, so nothing was saved. Reload this page and try again.',
  same_origin_required: 'ThruLine keeps this only from its own page, so nothing was saved. Reload this page and try again.',
  no_such_project: 'This work is no longer in the record ThruLine is reading, so nothing was saved. Reload this page.',
  no_such_thread: 'This work is no longer in the record ThruLine is reading, so nothing was saved. Reload this page.',
  too_long: 'A message for the agent can be at most 8,000 characters, so nothing was saved. Shorten the draft and copy it again.',
  no_copy_to_mark: 'There is no copied message on this piece of work to mark as pasted, so nothing was saved. Copy the message first.',
  already_marked: 'That copy is already marked as pasted, so nothing was saved. Reload this page.',
  no_mark_to_undo: 'There is no “pasted” mark on this piece of work to undo, so nothing was saved. Reload this page.',
  upgrade_pending: 'ThruLine needs an upgrade before it can record anything, so nothing was saved. Run thruline upgrade in a terminal, then try again.',
  unreachable: 'ThruLine did not answer, so it may not have kept this. Reload this page to see what the record holds.',
}

/**
 * What one answer means for the screen. PURE.
 * @param {{ok: boolean, status: number, doc: any}} res what `recordTold` resolved to
 * @returns {{kind: 'kept', told: object|null, act: object} | {kind: 'refused', detail: string, code: string}}
 *   `told` is the thread's `told_agent` after the act, as served.
 */
export function toldAnswerOf(res) {
  const r = res && typeof res === 'object' ? res : {}
  if (typeof r.status !== 'number') return { kind: 'refused', code: 'unreachable', detail: REFUSAL_WORDS.unreachable + AND_NOTHING }
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  if (r.status === 200 && doc.act && typeof doc.act === 'object') {
    return { kind: 'kept', act: doc.act, told: doc.told_agent && typeof doc.told_agent === 'object' ? doc.told_agent : null }
  }
  const code = typeof doc.error === 'string' ? doc.error : ''
  // a server sentence about the busy store, in its own words
  if (code === 'store_busy' && typeof doc.message === 'string' && doc.message) return { kind: 'refused', code, detail: doc.message + AND_NOTHING }
  if (REFUSAL_WORDS[code]) return { kind: 'refused', code, detail: REFUSAL_WORDS[code] + AND_NOTHING }
  if (r.status === 413) return { kind: 'refused', code: 'too_long', detail: REFUSAL_WORDS.too_long + AND_NOTHING }
  if (r.status === 200) return { kind: 'refused', code: 'unreadable', detail: 'ThruLine answered in a form this page cannot read. Reload this page to see what the record holds.' + AND_NOTHING }
  return { kind: 'refused', code: 'other', detail: 'ThruLine could not keep this (it answered ' + r.status + '). Reload this page to see what the record holds.' + AND_NOTHING }
}
