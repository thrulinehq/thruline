/* Done-Criteria's writes: the two addresses this file reaches, each a named
 * constant beside the one control that uses it (CLAUDE.md, the write-address
 * table). Accept, Edit, Drop, Restore, Add a statement, Add to out of scope, a
 * picked template and the folder chips change only this screen's draft; the
 * draft is written once, by the accept act. ("Track this work as a Change",
 * write address 14, is Repair-Loop/actions.js's one client, reused by index.jsx.)
 *
 *   Accept what counts as done (vN)  →  POST /api/changes/<id>/scope/accept   (write address 11)
 *                                        { criteria: [n…], edits: {n: wording}, lines?: [text…],
 *                                          exclusions?: [text…], allowed_scope: [folder…], actor, replacing? }
 *   Link step N / Unlink              →  POST /api/changes/<id>/links          (write address 4)
 *                                        { criterion, kind: 'check_run', evidence, act: 'link'|'unlink', actor, reason }
 */

/** "Accept what counts as done": the one write for what counts as done (T-2-3). */
export const SCOPE_ACCEPT = (id) => '/api/changes/' + encodeURIComponent(id) + '/scope/accept'
/** "Link step N" / "Unlink": write address 4. */
export const CHANGE_LINKS = (id) => '/api/changes/' + encodeURIComponent(id) + '/links'

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

/**
 * Accept the lines the person accepted, in their wording, over the folders
 * they kept: proposals by number, every other line (one they added, one of the
 * standing version they keep) by its words, and what they marked out of scope.
 * Lines left proposed or dropped are simply not named.
 */
export function acceptCriteria({ changeId, accepted, edits, lines, exclusions, folders, actor, replacing }, f = fetch) {
  const criteria = (accepted || []).filter((n) => Number.isInteger(n))
  const own = (lines || []).map((t) => String(t || '').trim()).filter(Boolean)
  const out = (exclusions || []).map((t) => String(t || '').trim()).filter(Boolean)
  if (!changeId || (!criteria.length && !own.length)) return Promise.resolve({ ok: false, status: 0, doc: { error: 'nothing_to_accept' } })
  const body = criteria.length ? { criteria, actor: actor || 'you' } : { actor: actor || 'you' }
  const wording = {}
  for (const [n, text] of Object.entries(edits || {})) if (criteria.includes(Number(n)) && String(text || '').trim()) wording[n] = String(text).trim()
  if (Object.keys(wording).length) body.edits = wording
  if (own.length) body.lines = own
  if (out.length) body.exclusions = out
  if (Array.isArray(folders)) body.allowed_scope = folders
  if (replacing) body.replacing = true
  return post(SCOPE_ACCEPT(changeId), body, f)
}

/**
 * The accept act's answer, for a person: `{ok, version}` when a version was
 * written, else `{ok: false, said}` — the server's own sentence when it sent
 * one (a refusal is already in the copy file's words), a plain one otherwise.
 */
export function acceptAnswer(r) {
  const doc = (r && r.doc) || {}
  if (r && r.ok && doc.accepted && Number.isInteger(doc.accepted.version)) return { ok: true, version: doc.accepted.version, said: null }
  if (r && r.ok) return { ok: false, version: null, said: 'ThruLine’s answer did not say which version it recorded. Reload this page to read what was recorded.' }
  if (typeof doc.plain === 'string' && doc.plain) return { ok: false, version: null, said: doc.plain }
  if (doc.error === 'refused' && typeof doc.detail === 'string' && doc.detail) return { ok: false, version: null, said: doc.detail }
  if (!r || !r.status) return { ok: false, version: null, said: 'ThruLine’s local server did not answer, so nothing was recorded. Reload this page and try again.' }
  if (r.status === 503 && typeof doc.detail === 'string') return { ok: false, version: null, said: 'ThruLine could not open the record, so nothing was recorded: ' + doc.detail }
  return { ok: false, version: null, said: 'ThruLine could not record what counts as done, so nothing was recorded. Reload this page and try again.' }
}

/** Link (or unlink) one passing run to one accepted line, with a name on it. */
export function linkRun({ changeId, criterion, evidence, unlink, actor }, f = fetch) {
  if (!changeId || !Number.isInteger(criterion) || !evidence) return Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })
  return post(CHANGE_LINKS(changeId), { criterion, kind: 'check_run', evidence, act: unlink ? 'unlink' : 'link', actor: actor || 'you', reason: '' }, f)
}
