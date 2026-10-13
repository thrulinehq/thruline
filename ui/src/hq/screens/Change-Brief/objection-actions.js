/* The Brief's second write: keep an objection to one statement (C30 R-LEDGER).
 *
 *   This is wrong…  →  POST /api/hq/projects/<project>/objections   (write address 25, CLAUDE.md)
 *
 * The address is declared once, here, beside the one control that uses it
 * (objection.jsx), and `brief-objections.test.js` asserts this folder reaches
 * exactly it. The body is `{statement, reason}` and nothing else: the sentence
 * and its digest are read by the SERVER from the project's own record, and the
 * name on the objection is the account ThruLine is running as. An objection is
 * words beside a statement: the server appends one row and nothing a verdict,
 * a count, readiness or an approval reads. It is free (approving is the one
 * paid act). Taking one back is not offered (C34 ruling Q16).
 *
 * `crates/cli/src/uiserve/person_acts.rs` `objection` answers one of:
 *   200 {schema_version, statement, objection, message}   kept
 *   400 {error: "malformed_request" | "reason_required"}  refused before the record
 *   403 {error: "same_origin_required"}                   not from HQ's own page
 *   404 {error: "no_such_project" | "no_such_statement"}
 *   409 {error: "too_long", field: "reason", limit}       over 2,000 characters
 *   413 {error: "too_long"}                               larger than a reason within the limit could make
 *   503 {error: "upgrade_pending" | "store_busy", message}
 * and `send` turns a request that got no answer into status 0,
 * `{error: "unreachable"}`. `objectionAnswerOf` reads every one; none is dropped.
 */

/** "This is wrong…": write address 25. */
export const PROJECT_OBJECTIONS = (project) => '/api/hq/projects/' + encodeURIComponent(project) + '/objections'

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

/** Keep these words as an objection to this statement. Nothing is sent without a statement or without words. */
export function recordObjection({ project, statement, reason }, f = fetch) {
  const words = typeof reason === 'string' ? reason.trim() : ''
  if (!project || !statement || !words) return Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })
  return send(PROJECT_OBJECTIONS(project), { statement, reason: words }, f)
}

/** Plain words for a refusal the server named, drawn after "Not kept." */
const REFUSAL_WORDS = {
  not_available: 'An objection needs a statement to be kept beside and some words saying what is wrong.',
  reason_required: 'Say what is wrong with this statement.',
  malformed_request: 'ThruLine could not read the request this page sent. Reload this page and try again.',
  same_origin_required: 'ThruLine keeps an objection only from its own page. Reload this page and try again.',
  no_such_project: 'ThruLine has no record of this project. Reload this page.',
  no_such_statement: 'ThruLine has no record of this statement. Reload this page.',
  too_long: 'A reason can be at most 2,000 characters. Shorten it and try again.',
  upgrade_pending: 'ThruLine needs an upgrade before it can record anything. Run thruline upgrade in a terminal, then try again.',
  store_busy: 'ThruLine’s record was busy with something else. Try again in a moment.',
  unreachable: 'ThruLine did not answer. Reload this page to see whether the objection was kept.',
}

/**
 * What one answer means for the form. PURE.
 * @param {{ok: boolean, status: number, doc: any}} res what `recordObjection` resolved to
 * @returns {{kind: 'kept', objection: object} | {kind: 'refused', detail: string}}
 */
export function objectionAnswerOf(res) {
  const r = res && typeof res === 'object' ? res : {}
  if (typeof r.status !== 'number') return { kind: 'refused', detail: REFUSAL_WORDS.unreachable }
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  if (r.status === 200 && doc.objection && typeof doc.objection === 'object') return { kind: 'kept', objection: doc.objection }
  if (typeof doc.error === 'string' && REFUSAL_WORDS[doc.error]) return { kind: 'refused', detail: REFUSAL_WORDS[doc.error] }
  if (r.status === 413) return { kind: 'refused', detail: REFUSAL_WORDS.too_long }
  if (r.status === 200) return { kind: 'refused', detail: 'ThruLine answered in a form this page cannot read. Reload this page to see what the record holds.' }
  return { kind: 'refused', detail: 'ThruLine could not keep this (it answered ' + r.status + '). Reload this page to see what the record holds.' }
}
