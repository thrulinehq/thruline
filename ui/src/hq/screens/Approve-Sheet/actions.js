/* Approve-Sheet's writes: the only two addresses this screen reaches, each a
 * named constant beside the one control that uses it (CLAUDE.md, the
 * write-address table, rows 1 and 2). Nothing else on this sheet writes:
 * Copy the trailer is the clipboard, Cancel and Close write nothing.
 *
 *   Approve <version>  →  POST /api/changes/<id>/decisions           (write address 1)
 *   Withdraw approval  →  POST /api/changes/<id>/decisions/withdraw  (write address 2)
 *
 * Both carry the name typed into the box and the full version the screen was
 * showing (`seen_version`), exactly as `ui/src/workspace/change-workspace.jsx`
 * `Decision` sends them. An approval also carries the sheet's note when the
 * person wrote one (`note`, at most 2,000 characters): the server appends it in
 * the same transaction as the approval, so the approval and its note are
 * recorded together or not at all (C30 R-LEDGER, CLAUDE.md write-address row 1).
 * The server refuses an approval on a machine with no
 * Pro licence before the ledger is touched (T-DIST-4); withdrawing is free.
 * Each resolves to the server's answer, which `answerOf` below reads and the
 * sheet draws: nothing here re-reads or discards it (C30 P0-2).
 */

/** "Approve <version>": write address 1. */
export const CHANGE_DECIDE = (id) => '/api/changes/' + encodeURIComponent(id) + '/decisions'
/** "Withdraw approval": write address 2. */
export const CHANGE_WITHDRAW = (id) => '/api/changes/' + encodeURIComponent(id) + '/decisions/withdraw'

async function post(url, body, f) {
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

/** Approve exactly the version the screen shows, with a name on it and, when written, a note kept with the approval. */
export function approveVersion({ changeId, seen, actor, note }, f = fetch) {
  if (!changeId || !seen || !String(actor || '').trim()) return Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })
  const body = { actor: String(actor).trim(), seen_version: seen }
  const words = typeof note === 'string' ? note.trim() : ''
  if (words) body.note = words
  return post(CHANGE_DECIDE(changeId), body, f)
}

/** Take the standing approval back: a row of its own, nothing edited. */
export function withdrawApproval({ changeId, approvedFull, actor }, f = fetch) {
  if (!changeId || !approvedFull || !String(actor || '').trim()) return Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })
  return post(CHANGE_WITHDRAW(changeId), { actor: String(actor).trim(), seen_version: approvedFull }, f)
}

/* ---- the server's answer, read (C30 P0-2) ----
 *
 * `crates/cli/src/uiserve.rs` `change_decide` answers one of:
 *   200 {acted: {outcome: "recorded", decision}, decisions}   the act is on the record
 *   200 {acted: {outcome: "moved", seen, now, changed[]}, decisions}
 *        the code moved while the screen was open; nothing was recorded
 *        (`changed` is empty when the record cannot name the files)
 *   403 {error: "licence_required", status, reason, enable}  no licence that allows approving; nothing recorded
 *   403 {error: "same_origin_required"}                       not from HQ's own page
 *   400 {error: "malformed_request" | "actor_required"}       refused before the record
 *   400 {error: "refused", detail}                            refused by the record, in a sentence for a person
 *   409 {error: "too_long", field: "note" | "actor"}          over its limit; nothing recorded
 *   413 {error: "too_long"}                                   a request larger than a note within the limit could make
 *   503 {error: "upgrade_pending", message}                   the store needs `thruline upgrade`
 * and `post` above turns a request that got no answer into status 0,
 * `{error: "unreachable"}`. Every one is drawn; none is dropped. */

const shortRev = (rev) => (typeof rev === 'string' && rev.length > 7 ? rev.slice(0, 7) : rev || null)

/** Plain words for a refusal the server named, drawn after "Not approved." / "Not withdrawn." */
const REFUSAL_WORDS = {
  actor_required: 'An approval has to have a name on it. Put your name in the box and try again.',
  malformed_request: 'ThruLine could not read the request this page sent. Reload this page and try again.',
  same_origin_required: 'ThruLine records a decision only from its own page. Reload this page and try again.',
  upgrade_pending: 'ThruLine needs an upgrade before it can record anything. Run thruline upgrade in a terminal, then try again.',
  unreachable: 'ThruLine did not answer. Reload this page to see whether anything was recorded.',
  no_such_change: 'ThruLine has no record of this Change. Reload this page.',
  too_long: 'Something you typed is longer than ThruLine keeps. A note can be at most 2,000 characters and a name at most 200. Shorten it and try again.',
}

/**
 * What one answer means for the sheet. PURE.
 * @param {{ok: boolean, status: number, doc: any}} res what `approveVersion` / `withdrawApproval` resolved to
 * @returns {{kind: 'recorded', decision: object, decisions: object|null}
 *   | {kind: 'moved', moved: {version: string|null, files: string[], step: null, at: null}}
 *   | {kind: 'licence'}
 *   | {kind: 'refused', detail: string}}
 */
export function answerOf(res) {
  const r = res && typeof res === 'object' ? res : {}
  if (typeof r.status !== 'number') return { kind: 'refused', detail: REFUSAL_WORDS.unreachable }
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  const acted = doc.acted && typeof doc.acted === 'object' ? doc.acted : null
  if (r.status === 200 && acted && acted.outcome === 'recorded' && acted.decision && typeof acted.decision === 'object') {
    return { kind: 'recorded', decision: acted.decision, decisions: doc.decisions && typeof doc.decisions === 'object' ? doc.decisions : null }
  }
  if (r.status === 200 && acted && acted.outcome === 'moved') {
    return { kind: 'moved', moved: { version: shortRev(acted.now), files: Array.isArray(acted.changed) ? acted.changed.map(String) : [], step: null, at: null } }
  }
  if (r.status === 403 && doc.error === 'licence_required') return { kind: 'licence' }
  if (doc.error === 'refused' && typeof doc.detail === 'string' && doc.detail.trim()) {
    const d = doc.detail.trim()
    return { kind: 'refused', detail: d.charAt(0).toUpperCase() + d.slice(1) + (/[.!?]$/.test(d) ? '' : '.') }
  }
  if (typeof doc.error === 'string' && REFUSAL_WORDS[doc.error]) return { kind: 'refused', detail: REFUSAL_WORDS[doc.error] }
  if (r.status === 200) return { kind: 'refused', detail: 'ThruLine answered in a form this page cannot read. Reload this page to see what the record holds.' }
  return { kind: 'refused', detail: 'ThruLine could not record this (it answered ' + r.status + '). Reload this page to see what the record holds.' }
}
