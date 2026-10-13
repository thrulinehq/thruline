/* Following one Change's check while it runs (C30 P0-3, R-RUN).
 *
 * The one address READ here is the run sheet, `GET /api/changes/<id>/run`
 * (run_route.rs `view`): what will run, the permission, the run going now and
 * the last outcome. The writes that start and stop a check keep their named
 * constants in Repair-Loop/actions.js; `useCheckRun` below is how every other
 * screen presses them, so there is one run client (C30 R-RUN).
 *
 * While the sheet says a check of this Change is going, it is read again every
 * POLL_MS; when it says the check has ended, reading stops and the screen
 * re-reads the record once. Nothing here decides an outcome: `isRunning` reads
 * the served `job` and `checks[].last.outcome`, exactly the fields the run
 * sheet's own model reads (workspace/run-route.js).
 */
import { useEffect, useRef, useState } from 'react'
import { API } from './api.js'
import { runSheetModel } from '../../workspace/run-route.js'
import { runControls } from '../screens/Repair-Loop/actions.js'
import { href } from '../routes.js'

export const POLL_MS = 1000

/** True while the served sheet says a check of THIS Change is starting or running. */
export function isRunning(doc) {
  if (!doc || typeof doc !== 'object') return false
  if (doc.job) return doc.job.for_this_change === true
  const check = Array.isArray(doc.checks) ? doc.checks[0] : null
  return Boolean(check && check.last && check.last.outcome === 'running')
}

/**
 * What this Change's running check has printed so far, and for how long it
 * has run, as the run sheet serves them (C30 R-RUN, run_route.rs `view`):
 *
 *   lines    the newest lines it printed, oldest first, masked on the server;
 *            `[]` when it has printed nothing yet; `null` when the sheet
 *            serves no output (a check started from a terminal prints there,
 *            and a server from before R-RUN serves none) — the screen then
 *            says it is not shown, and never draws output it was not served
 *   cut      the server says something printed is not among `lines`
 *   elapsedS whole seconds from the run's own recorded start (`elapsed_s`),
 *            or null when the sheet serves none (still starting)
 *
 * Only a check of THIS Change is read: another Change's run is never drawn
 * as this one's. null when no check of this Change is going.
 *
 * @returns {{run: string|null, lines: string[]|null, cut: boolean, elapsedS: number|null}|null}
 */
export function liveOutputOf(served) {
  if (!served || typeof served !== 'object') return null
  const job = served.job && typeof served.job === 'object' ? served.job : null
  const check = Array.isArray(served.checks) ? served.checks[0] || null : null
  const last = check && check.last && check.last.outcome === 'running' ? check.last : null
  const seconds = (v) => (Number.isInteger(v) && v >= 0 ? v : null)
  if (job) {
    if (job.for_this_change !== true) return null
    const out = job.output && typeof job.output === 'object' && Array.isArray(job.output.lines) ? job.output : null
    return {
      run: job.run || null,
      lines: out ? out.lines.filter((l) => typeof l === 'string') : null,
      cut: Boolean(out && out.cut === true),
      elapsedS: seconds(job.elapsed_s),
    }
  }
  /* a check of this Change started outside this HQ: its time, never its output */
  if (last) return { run: last.run || null, lines: null, cut: false, elapsedS: seconds(last.elapsed_s) }
  return null
}

/** One read of the sheet: `{ok, status, doc}`, never a throw. */
export async function readRunSheet(id, f = fetch) {
  try {
    const res = await f(API.run(id), { cache: 'no-store', headers: { Accept: 'application/json' } })
    const doc = await res.json().catch(() => null)
    return { ok: res.ok, status: res.status, doc }
  } catch (_e) {
    return { ok: false, status: 0, doc: null }
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

/**
 * Read the sheet until it no longer says a check is going (or a read fails, or
 * `live()` says the screen has gone). Every sheet read is handed to `onSheet`.
 *
 * @returns {Promise<{ok: boolean, status: number, doc: object|null}>} the last read
 */
export async function followRun(id, { f = fetch, wait = () => sleep(POLL_MS), onSheet = () => {}, live = () => true } = {}) {
  for (;;) {
    const read = await readRunSheet(id, f)
    if (!live()) return read
    if (read.ok) onSheet(read.doc)
    if (!read.ok || !isRunning(read.doc)) return read
    await wait()
    if (!live()) return read
  }
}

/**
 * The run sheet a screen draws: the one read with the Change's documents
 * (`initial`), replaced by fresher reads while a check is followed. When a
 * followed check ends, `onEnd` runs once (the screen re-reads the record).
 *
 * @returns {{sheet: object|null, follow: () => Promise<void>}}
 */
export function useRunSheet(id, initial, { onEnd } = {}) {
  const [fresh, setFresh] = useState(null) // { id, doc }
  const following = useRef(false)
  const alive = useRef(true)
  const endRef = useRef(onEnd)
  endRef.current = onEnd
  useEffect(() => () => { alive.current = false }, [])
  /* the Change's own read is newer than anything followed before it */
  useEffect(() => { setFresh(null) }, [id, initial])

  const follow = async () => {
    if (!id || following.current) return
    following.current = true
    let wasRunning = false
    try {
      const end = await followRun(id, {
        live: () => alive.current,
        onSheet: (doc) => { if (isRunning(doc)) wasRunning = true; setFresh({ id, doc }) },
      })
      if (alive.current && end.ok && wasRunning && typeof endRef.current === 'function') endRef.current()
    } finally {
      following.current = false
    }
  }

  const sheet = fresh && fresh.id === id ? fresh.doc : initial || null
  /* arriving on a sheet that is already running follows it */
  const running = isRunning(sheet)
  useEffect(() => { if (running) follow() }, [id, running]) // eslint-disable-line react-hooks/exhaustive-deps
  return { sheet, follow }
}

/* ---------------------------------------------------------------------------
 * C30 R-RUN: one run client for every screen that draws "Check it again"
 * (Brief, Every-Statement, What-Changed, Repair-Loop, ⌘K).
 *
 * The writes stay where the census pins them (Repair-Loop/actions.js, the one
 * file that names the run, permission and cancel addresses); this file holds
 * what every screen shares around them:
 *
 *   runActOf(changeId, served)    what the controls act on, from the served
 *                                 run sheet (one reading, runSheetModel)
 *   useCheckRun(id, served)       the sheet followed while a check runs, plus
 *                                 start and stop, one press at a time
 *   checkRunHref / takeRunIntent  "the click is the consent": an address that
 *                                 opens the check sheet already running, read
 *                                 ONCE and taken out of the address before
 *                                 anything starts, so a reload, Back or
 *                                 Forward never starts a second run
 *   failuresOf(last)              the failing tests the served record names
 * ------------------------------------------------------------------------- */

/** The tracked Change id of a Change snapshot (data/change.js), or null. */
export function trackedOf(snapshot) {
  const c = snapshot && snapshot.change
  return c && c.id && c.threadId && c.id !== c.threadId ? String(c.id) : null
}

/**
 * What the run controls act on, from one served run sheet: the Change, whether
 * the server would start its check now, the check, the permission, the run
 * that can be stopped, and — when it will not start one — its reason in the
 * run sheet's own words. Nothing here decides an outcome.
 */
export function runActOf(changeId, served) {
  const sheet = served ? runSheetModel(served) : null
  const ready = sheet && sheet.status === 'ready' ? sheet : null
  const blocked = ready && ready.primary && ready.primary.blocked ? ready.primary.blocked : null
  const starting = Boolean(ready && ready.running === 'starting')
  return {
    changeId: changeId || null,
    /* the server would start it: a folder, a check, nothing going. After
     * "tool not found" the run sheet's primary is to ask the agent, and a
     * press still starts the check again. */
    canStart: Boolean(ready && ready.primary && (ready.primary.act === 'run' || ready.primary.act === 'allow-run' || ready.primary.act === 'ask')),
    needsPermission: Boolean(ready && ready.permission && ready.permission.needed),
    checkId: ready && ready.check ? ready.check.id || null : null,
    command: ready && ready.check ? ready.check.command || null : null,
    /* only a check that has started can be stopped */
    runKey: ready && ready.running === 'running' ? ready.runKey || null : null,
    running: ready ? ready.running || null : null,
    actor: ready ? ready.actor || '' : '',
    /* tracked, and the sheet could not be read */
    sheetRead: changeId ? Boolean(ready) : null,
    /* the server's reason it will not start one now: {reason, enabler} */
    blocked: blocked && !starting ? { reason: blocked.reason, enabler: blocked.enabler } : null,
    stopBlocked: starting && blocked ? { reason: blocked.reason, enabler: blocked.enabler } : null,
  }
}

/**
 * One Change's check, as a screen holds it: the served sheet (followed while a
 * check runs), the act, and the two presses. `start` allows checks in the
 * project first when the person has not yet (the press is the consent), then
 * starts the one check; `stop` reaches the cancel route. A press is answered
 * when its write is; following the sheet is not awaited.
 */
export function useCheckRun(changeId, served, { onEnd, f } = {}) {
  const followed = useRunSheet(changeId, served, { onEnd })
  const [press, setPress] = useState({ busy: false, said: null })
  const busy = useRef(false)
  const act = runActOf(changeId, followed.sheet)
  const controls = runControls(act, {
    ...(f ? { f } : {}),
    said: (words) => setPress((p) => ({ ...p, said: words })),
    follow: () => followed.follow(),
  })
  const one = (fn) => async () => {
    if (busy.current) return undefined
    busy.current = true
    setPress({ busy: true, said: null })
    try {
      return await fn()
    } finally {
      busy.current = false
      setPress((p) => ({ busy: false, said: p.said }))
    }
  }
  return {
    sheet: followed.sheet,
    act: { ...act, busy: press.busy, refusal: press.said },
    start: one(controls.checkAgain),
    stop: one(controls.stopCheck),
    follow: followed.follow,
  }
}

/** The query word that says "the press that led here is the consent to run". */
export const RUN_INTENT = 'run'

/** The check sheet's address, opening already running (`…?run=1`). */
export function checkRunHref(sheetAddress) {
  const a = String(sheetAddress || '')
  return a + (a.includes('?') ? '&' : '?') + RUN_INTENT + '=1'
}

/**
 * Read the run intent from the address ONCE, and take it out of the address
 * before anything starts (history.replaceState: no new history entry, so a
 * reload, Back or Forward lands on the plain sheet). If the address cannot be
 * rewritten, the intent is refused: a run is never started that a reload
 * could start again.
 *
 * @param {{location: {hash: string}, history: {replaceState: Function, state?: any}}|null} win
 * @returns {boolean} true at most once per press
 */
export function takeRunIntent(win = typeof window !== 'undefined' ? window : null) {
  if (!win || !win.location || !win.history || typeof win.history.replaceState !== 'function') return false
  const hash = String(win.location.hash || '')
  const at = hash.indexOf('?')
  if (at < 0) return false
  const params = new URLSearchParams(hash.slice(at + 1))
  if (params.get(RUN_INTENT) !== '1') return false
  params.delete(RUN_INTENT)
  const next = hash.slice(0, at) + (params.toString() ? '?' + params.toString() : '')
  try {
    win.history.replaceState(win.history.state === undefined ? null : win.history.state, '', next || '#/')
  } catch (_e) {
    return false
  }
  /* the address no longer carries it, or nothing starts */
  return !new URLSearchParams(String(win.location.hash || '').split('?')[1] || '').has(RUN_INTENT)
}

/**
 * What arriving with the intent does, once the sheet has been read:
 * `wait` (not read yet), `start`, `follow` (a check of this Change is already
 * going: nothing new is started), or `drop` (it cannot start here; the ready
 * form says why).
 */
export function arrivalStep({ snapshot, trackedLoading, act, sheet }) {
  if (!snapshot || trackedLoading) return 'wait'
  if (!act || !act.changeId) return 'drop'
  if (isRunning(sheet)) return 'follow'
  return act.canStart && !act.busy ? 'start' : 'drop'
}

/**
 * "Check it again" on a screen that opens the check sheet to run it (Every
 * statement, What changed, ⌘K): when the server would start the check, the
 * press opens the sheet already running (`…/check?run=1`, the press is the
 * consent); when it says it will not, the control is off with its reason;
 * work that is not tracked opens the sheet's ready form, which offers "Track
 * this work as a Change". A check already going is stopped from here too.
 *
 * @param {{act: object, stop?: Function, at: object, go?: (hash: string) => void}} o
 */
export function openCheckActions({ act, stop, at, go }) {
  const a = act || {}
  const where = at || {}
  const sheet = href('check', { change: where.change, project: where.project })
  const target = a.canStart ? checkRunHref(sheet) : sheet
  const to = go || ((h) => { if (typeof window !== 'undefined') window.location.hash = h })
  return {
    check: () => {
      if (a.busy) return undefined
      if (!a.canStart && a.blocked) return undefined
      return to(target)
    },
    checkHref: !a.canStart && a.blocked ? undefined : target,
    /* true when the press opens the sheet already running */
    runs: Boolean(a.canStart),
    stopCheck: a.runKey && !a.busy && typeof stop === 'function' ? stop : undefined,
    stopBlocked: a.stopBlocked || null,
    checkBlocked: !a.canStart && a.blocked ? a.blocked : null,
    needsPermission: Boolean(a.canStart && a.needsPermission),
    refusal: a.refusal || null,
  }
}

/* The failing test a recorded line names, in the runner's own form. A line
 * that matches none of them names no test, and the draft says so. */
const FAILURE_FORMS = [
  /^test\s+(\S+)\s+\.\.\.\s+FAILED\b/, //          cargo test
  /^FAILED\s+(\S+?)(?:\s+-\s.*)?$/, //             pytest summary
  /^---\s+FAIL:\s+(\S+)/, //                       go test
  /^(?:✕|×)\s+(.+?)(?:\s+\(\d+\s*m?s\))?$/, //     jest / vitest
]

/**
 * The failures of one served run (`checks[].last`), from its typed fields
 * only: the record's own first failure line and its failed count.
 *
 * @returns {{lines: string[], tests: string[], failed: number|null, unnamed: number|null}}
 */
export function failuresOf(last) {
  const line = last && typeof last.failure_line === 'string' && last.failure_line.trim() ? last.failure_line.trim() : null
  const failed = last && Number.isInteger(last.tests_failed) ? last.tests_failed : null
  const tests = []
  if (line) {
    for (const form of FAILURE_FORMS) {
      const m = line.match(form)
      if (m && m[1]) { tests.push(m[1].trim()); break }
    }
  }
  return { lines: line ? [line] : [], tests, failed, unnamed: failed == null ? null : Math.max(0, failed - tests.length) }
}
