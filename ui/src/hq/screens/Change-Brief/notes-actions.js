/* The Brief's one write: keep a note on the tracked Change (C30 R-LEDGER).
 *
 *   Add a note  →  POST /api/changes/<id>/notes   (write address 21, CLAUDE.md)
 *
 * The address is declared once, here, beside the one control that uses it
 * (notes.jsx), and `brief-notes.test.js` asserts this folder reaches exactly
 * it. The body is `{body}` and nothing else; the name on the note is the
 * account ThruLine is running as, chosen by the server. A note is words: the
 * server appends one row and nothing a verdict, a count, readiness or an
 * approval reads. It is free (approving is the one paid act).
 *
 * `crates/cli/src/uiserve/person_acts.rs` `add_note` answers one of:
 *   200 {schema_version, note}                      kept
 *   400 {error: "malformed_request" | "body_required"}  refused before the record
 *   403 {error: "same_origin_required"}             not from HQ's own page
 *   404 {error: "no_such_change"}
 *   409 {error: "too_long", field: "body", limit}   over 2,000 characters
 *   413 {error: "too_long"}                         larger than a note within the limit could make
 *   503 {error: "upgrade_pending", message}         the store needs `thruline upgrade`
 * and `send` turns a request that got no answer into status 0,
 * `{error: "unreachable"}`. `noteAnswerOf` reads every one; none is dropped.
 */

/** "Add a note": write address 21. */
export const CHANGE_NOTES = (id) => '/api/changes/' + encodeURIComponent(id) + '/notes'

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

/** Keep these words, as a note on this Change. Nothing is sent without a Change or without words. */
export function addNote({ changeId, body }, f = fetch) {
  const words = typeof body === 'string' ? body.trim() : ''
  if (!changeId || !words) return Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })
  return send(CHANGE_NOTES(changeId), { body: words }, f)
}

/** Plain words for a refusal the server named, drawn after "Not kept." */
const REFUSAL_WORDS = {
  not_available: 'A note needs a Change to be kept on and some words to keep.',
  body_required: 'A note has to say something.',
  malformed_request: 'ThruLine could not read the request this page sent. Reload this page and try again.',
  same_origin_required: 'ThruLine keeps a note only from its own page. Reload this page and try again.',
  no_such_change: 'ThruLine has no record of this Change. Reload this page.',
  too_long: 'A note can be at most 2,000 characters. Shorten it and try again.',
  upgrade_pending: 'ThruLine needs an upgrade before it can record anything. Run thruline upgrade in a terminal, then try again.',
  unreachable: 'ThruLine did not answer. Reload this page to see whether the note was kept.',
}

/**
 * What one answer means for the form. PURE.
 * @param {{ok: boolean, status: number, doc: any}} res what `addNote` resolved to
 * @returns {{kind: 'kept', note: object} | {kind: 'refused', detail: string}}
 */
export function noteAnswerOf(res) {
  const r = res && typeof res === 'object' ? res : {}
  if (typeof r.status !== 'number') return { kind: 'refused', detail: REFUSAL_WORDS.unreachable }
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  if (r.status === 200 && doc.note && typeof doc.note === 'object') return { kind: 'kept', note: doc.note }
  if (typeof doc.error === 'string' && REFUSAL_WORDS[doc.error]) return { kind: 'refused', detail: REFUSAL_WORDS[doc.error] }
  if (r.status === 413) return { kind: 'refused', detail: REFUSAL_WORDS.too_long }
  if (r.status === 200) return { kind: 'refused', detail: 'ThruLine answered in a form this page cannot read. Reload this page to see what the record holds.' }
  return { kind: 'refused', detail: 'ThruLine could not keep this (it answered ' + r.status + '). Reload this page to see what the record holds.' }
}
