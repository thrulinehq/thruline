/* Repair-Loop's writes: the only addresses this screen reaches, each a named
 * constant beside the one control that uses it (CLAUDE.md, the write-address
 * table). Nothing else on this screen writes.
 *
 *   Check it again  →  POST /api/changes/<id>/run/permission   (first time in a project)
 *                      POST /api/changes/<id>/run              (start the one check)
 *   Stop this check →  POST /api/changes/<id>/runs/<run>/cancel (write address 3)
 *   Track this work as a Change → POST /api/first-use/change {project, thread} (C26 b)
 *
 * The run routes need this launch's token (`GET /api/run/token`), read and
 * sent exactly as `ui/src/workspace/run-sheet.jsx` does. Copy writes nothing.
 * Copy message and Mark as pasted · undo keep the act, through the one client
 * in `../../data/told-actions.js` (write address 24, C30 R-LEDGER) — not here,
 * so this file's census is unchanged. Nothing is sent.
 */
import { fmtCount } from '../../data/count.js'
import { HQ_COPY } from '../../../workspace/hq-copy.js'
import { refusalText } from '../../../workspace/run-route.js'

/** "Track this work as a Change" (C26 b): the existing first-use write, with
 *  the thread on screen. It appends one Change named after the thread and
 *  confirms the thread's sessions in that Change's recording project — never
 *  a scope, an acceptance or an approval, and no suggested links
 *  (docs/hq-next/API.md, "POST /api/first-use/change with a thread"). */
export const TRACK_WORK = '/api/first-use/change'

/** The one read of this launch's token. */
export const RUN_TOKEN = '/api/run/token'
/** "Check it again": the one address a check is started at. */
export const RUN_START = (id) => '/api/changes/' + encodeURIComponent(id) + '/run'
/** "Check it again", the first time in a project: the person allows checks here. */
export const RUN_PERMISSION = (id) => '/api/changes/' + encodeURIComponent(id) + '/run/permission'
/** "Stop this check": write address 3. */
export const RUN_CANCEL = (id, run) => '/api/changes/' + encodeURIComponent(id) + '/runs/' + encodeURIComponent(run) + '/cancel'

async function answer(res) {
  const doc = await res.json().catch(() => null)
  return { ok: res.ok, status: res.status, doc }
}

async function postWithToken(url, body, f) {
  let token
  try {
    token = await answer(await f(RUN_TOKEN, { cache: 'no-store', headers: { Accept: 'application/json' } }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
  if (!token.ok || !token.doc || typeof token.doc.token !== 'string') {
    return { ok: false, status: token.status, doc: token.doc || { error: 'run_token_required' } }
  }
  try {
    return await answer(await f(url, {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json', 'X-ThruLine-Run-Token': token.doc.token },
      body: JSON.stringify(body || {}),
    }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

/**
 * Check it again: allow checks in this project if the person has not yet,
 * then start the one check. Returns the start's answer (or the permission's
 * refusal).
 */
export async function checkAgain({ changeId, checkId, needsPermission }, f = fetch) {
  if (!changeId || !checkId) return { ok: false, status: 0, doc: { error: 'not_available' } }
  if (needsPermission) {
    const granted = await postWithToken(RUN_PERMISSION(changeId), {}, f)
    if (!granted.ok) return granted
  }
  return postWithToken(RUN_START(changeId), { check: checkId }, f)
}

/** Stop this check: appends one cancelled record, after the process is stopped. */
export async function stopCheck({ changeId, runKey, actor }, f = fetch) {
  if (!changeId || !runKey) return { ok: false, status: 0, doc: { error: 'not_available' } }
  try {
    return await answer(await f(RUN_CANCEL(changeId, runKey), {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
      body: JSON.stringify({ actor: actor || '' }),
    }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

/* The run routes' refusals, in the run sheet's own words (HQ_COPY.run.refusals,
 * the same table `ui/src/workspace/run-sheet.jsx` says them from). One is said
 * differently here: the run sheet's "program_in_project" sends the person to a
 * "Look somewhere else" control this screen does not draw. */
const REFUSED = HQ_COPY.run.refusals
const HERE = {
  program_in_project: 'Nothing ran: the {tool} this check would use is inside this project, where the project’s own files can put anything, so ThruLine refused to start it. Install {tool} outside the project folder, then check it again.',
  actor_required: 'ThruLine could not tell who is stopping this check, so it was not stopped. Reload this page and try again.',
  no_such_run: 'ThruLine has no record of that check running for this Change, so nothing was stopped. Reload this page.',
}

/** A refused start or permission, in words. `null` for an answer that went. */
export function refusalWords(answer) {
  if (!answer || answer.ok) return null
  const doc = answer.doc || {}
  const code = typeof doc.error === 'string' && doc.error ? doc.error : answer.status ? 'other' : 'unreachable'
  if (code === 'already_running') return null // the check that is going is followed instead
  if (HERE[code]) {
    const tool = typeof doc.program === 'string' && doc.program ? doc.program : null
    return HERE[code].includes('{tool}') ? (tool ? HERE[code].replace(/\{tool\}/g, tool) : REFUSED.other) : HERE[code]
  }
  return refusalText(code, doc)
}

/** A refused stop, in the server's own sentence when it sent one. */
export function stopRefusalWords(answer) {
  if (!answer || answer.ok) return null
  const doc = answer.doc || {}
  if (typeof doc.plain === 'string' && doc.plain) return doc.plain
  return refusalWords(answer)
}

/**
 * The two controls, as the screen hands them to its buttons: each writes
 * through the function above, says the answer (`said(words|null)`), and then
 * starts following the sheet (`follow()`, which reads `GET …/run` until the
 * check ends). The follow is not awaited: a press is answered when its write
 * is, so Stop is on while the check it started runs.
 */
export function runControls(act, { f = fetch, said = () => {}, follow = () => {} } = {}) {
  return {
    checkAgain: async () => {
      const r = await checkAgain(act, f)
      said(refusalWords(r))
      if (r.ok || (r.doc && r.doc.error === 'already_running')) follow()
      return r
    },
    stopCheck: async () => {
      const r = await stopCheck(act, f)
      said(stopRefusalWords(r))
      follow()
      return r
    },
  }
}

const AGENT_OF = { claude_code: 'Claude Code', codex: 'Codex' }

/** What the screen says when some of the thread's sessions could not be linked. */
export function notConfirmedSentence(rows) {
  const n = Array.isArray(rows) ? rows.length : 0
  if (!n) return null
  return (n === 1 ? 'One session of this work was' : fmtCount(n) + ' sessions of this work were')
    + ' recorded under another recording project, and a Change can hold only the sessions of its own project, so '
    + (n === 1 ? 'it is' : 'they are') + ' not linked to this Change.'
}

/**
 * Track this work as a Change: exactly one POST, naming the HQ project and the
 * thread on screen. Every answer comes back as words a person reads; a
 * refusal's code stays off the Plain surface (C21-d.4).
 */
export async function trackWork({ project, thread }, f = fetch) {
  if (!project || !thread) return { ok: false, said: 'ThruLine cannot tell which work is on screen, so nothing was written. Reload this page and try again.' }
  let res
  try {
    res = await answer(await f(TRACK_WORK, {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
      body: JSON.stringify({ project, thread }),
    }))
  } catch (_e) {
    return { ok: false, said: 'ThruLine’s local server did not answer, so nothing was written. Reload this page and try again.' }
  }
  const doc = res.doc || {}
  if (res.ok && doc.change) {
    return { ok: true, change: String(doc.change), already: doc.already_tracked === true, note: notConfirmedSentence(doc.not_confirmed) }
  }
  const code = doc.error ? String(doc.error) : 'HTTP ' + res.status
  if (res.status === 503 && typeof doc.message === 'string' && doc.message) return { ok: false, said: doc.message + ' Nothing was written.', code }
  if (res.status === 404) return { ok: false, said: 'This work is no longer in the record ThruLine is reading, so nothing was written. Reload this page.', code }
  return { ok: false, said: 'ThruLine could not track this work, so nothing was written. Reload this page and try again.', code }
}

/** The click: track, then — only after it succeeded — re-read, or open the
 *  Change that already tracks this work. */
export async function trackThenReread(where, { reread, open } = {}, f = fetch) {
  const r = await trackWork(where, f)
  if (r.ok && r.already && typeof open === 'function') open(r.change)
  else if (r.ok && typeof reread === 'function') await reread()
  return r
}
