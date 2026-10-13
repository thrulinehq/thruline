/* Capture health (C30 R-ROUTES): is one project being recorded, as the local
 * server reads it from facts on this machine.
 *
 *   GET /api/hq/projects/<id>/capture   (CAPTURE_ADDRESS; docs/hq-next/API.md)
 *
 * A read only: the route writes nothing, and this file sends nothing but that
 * GET. It holds three things, so every screen that says whether capture works
 * says it one way (F2):
 *
 *   readCapture   one read, refused in a sentence a person can read;
 *   captureFacts  the served document as typed facts and one phase —
 *                 none · waiting · recording · problem · unknown;
 *   useCapture    the read for the folder in front of the person, re-read
 *                 every 2 s (never faster) while the record says it is
 *                 waiting for a first step, and no longer once one is seen.
 *
 * Rules (C30 G3): a fact the route could not read stays null, with the
 * route's own reason beside it. A failed read is a failure, said as one: it
 * never reads as "installed" or as "not installed".
 */
import { useEffect, useMemo, useRef, useState } from 'react'

/** The read address: one project's capture health. */
export const CAPTURE_ADDRESS = (id) => `/api/hq/projects/${encodeURIComponent(id)}/capture`

/** The shape this client reads (the route's `schema_version`). */
export const CAPTURE_SCHEMA_VERSION = 1

/** The fastest re-read: a warm read costs 50–70 ms and a cold one about
 *  1.5 s (API.md, Cost), so a 2 s clock never stacks reads. */
export const CAPTURE_POLL_FLOOR_MS = 2000

const UNREADABLE_SHAPE = 'ThruLine answered in a form this page cannot read, so nothing could be checked.'

/**
 * One read. Resolves to the served document; rejects with a sentence.
 * @param {string} id the HQ project id
 * @param {{fetchImpl?: Function, signal?: AbortSignal}} [opts]
 */
export async function readCapture(id, { fetchImpl = typeof fetch === 'function' ? fetch : null, signal } = {}) {
  if (!fetchImpl) throw new Error('This browser cannot reach ThruLine’s local server.')
  let res
  try {
    res = await fetchImpl(CAPTURE_ADDRESS(id), { method: 'GET', signal, credentials: 'same-origin', headers: { accept: 'application/json' } })
  } catch (e) {
    if (e && e.name === 'AbortError') throw e
    throw new Error('ThruLine’s local server did not answer. Is thruline ui still running?')
  }
  if (res.status === 404) throw new Error('ThruLine no longer knows this folder. Reload the page and choose it again.')
  if (res.status === 503) throw new Error('ThruLine could not open its record, so nothing could be checked.')
  if (!res.ok) throw new Error(`ThruLine could not check this folder (it answered ${res.status}).`)
  let doc
  try {
    doc = JSON.parse(await res.text())
  } catch {
    throw new Error(UNREADABLE_SHAPE)
  }
  if (!doc || doc.schema_version !== CAPTURE_SCHEMA_VERSION || !doc.project || doc.project.id !== id || !doc.claude_code_hooks) {
    throw new Error(UNREADABLE_SHAPE)
  }
  return doc
}

const tri = (x) => (x === true ? true : x === false ? false : null)
const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? x : null)
const str = (x) => (typeof x === 'string' && x ? x : null)

/**
 * The served document as facts. Null when there is no document, or it has
 * no hooks part (nothing is read into it).
 *
 * phase:
 *   unknown    the hooks could not be read (`installed: null`), or the hooks
 *              are installed and the record could not be read
 *   none       no ThruLine recorder in any settings file Claude Code reads here
 *   problem    a recorder is installed and a ThruLine hook names a program
 *              that is not on this machine (`alive: false`)
 *   recording  installed, not known broken, and the hooks have delivered
 *   waiting    installed, not known broken, nothing through the hooks yet
 */
export function captureFacts(doc) {
  if (!doc || typeof doc !== 'object' || !doc.claude_code_hooks || typeof doc.claude_code_hooks !== 'object') return null
  const h = doc.claude_code_hooks
  const levels = Array.isArray(h.levels) ? h.levels.filter(Boolean) : []
  const commands = []
  for (const l of levels) {
    if (l.state !== 'read' || !Array.isArray(l.commands)) continue
    for (const c of l.commands) if (c) commands.push({ kind: c.kind, exists: c.program_exists !== false, scope: l.scope, path: l.path })
  }
  const recorders = commands.filter((c) => c.kind === 'recorder')
  const where = []
  for (const l of levels) {
    if (l.state === 'read' && Array.isArray(l.commands) && l.commands.some((c) => c && c.kind === 'recorder') && !where.some((w) => w.scope === l.scope && w.path === l.path)) {
      where.push({ scope: l.scope, path: str(l.path) })
    }
  }
  const installed = tri(h.installed)
  const alive = tri(h.alive)
  const hooks = {
    installed,
    alive,
    installedReason: str(h.installed_reason),
    aliveReason: str(h.alive_reason),
    events: Array.isArray(h.events) ? h.events.length : null,
    version: str(h.version),
    missing: num(h.missing_programs) ?? 0,
    commands: commands.length,
    recorders: recorders.length,
    recordersMissing: recorders.filter((c) => !c.exists).length,
    where,
  }
  const g = doc.git || {}
  const git = { repository: tri(g.repository), toplevel: str(g.toplevel), reason: str(g.reason) }
  const r = doc.record || {}
  const th = r.through_hooks && typeof r.through_hooks === 'object' ? r.through_hooks : null
  const step = (x) => (x && str(x.at) ? { at: x.at, agent: str(x.agent) } : null)
  const readable = !str(r.reason)
  const sessions = readable && r.sessions && typeof r.sessions === 'object'
    ? { claudeCode: num(r.sessions.claude_code), codex: num(r.sessions.codex) }
    : null
  const record = {
    readable,
    reason: str(r.reason),
    sessions,
    sessionsTotal: sessions ? (sessions.claudeCode || 0) + (sessions.codex || 0) : null,
    firstStep: step(r.first_step),
    latestStep: step(r.latest_step),
    deliveries: readable && th ? num(th.deliveries) : null,
    firstAt: th ? str(th.first_at) : null,
    latestAt: th ? str(th.latest_at) : null,
    latestEvent: th ? str(th.latest_event) : null,
  }
  const c = doc.codex || {}
  const codex = { configured: c.configured === true, state: str(c.state), paused: tri(c.paused), watchesThisProject: tri(c.watches_this_project), reason: str(c.reason) }
  let phase
  let reason = null
  if (installed === null) {
    phase = 'unknown'
    reason = hooks.installedReason
  } else if (installed === false) phase = 'none'
  else if (alive === false) phase = 'problem'
  else if (record.deliveries === null) {
    phase = 'unknown'
    reason = record.reason
  } else phase = record.deliveries > 0 ? 'recording' : 'waiting'
  return {
    project: doc.project ? { id: doc.project.id, root: str(doc.project.root) } : null,
    checkedAt: str(doc.checked_at),
    phase,
    reason,
    firstStepSeen: (record.deliveries || 0) > 0,
    hooks,
    git,
    record,
    codex,
  }
}

/**
 * What a screen holds for one read of one folder:
 *   null                                  nothing read, nothing being read
 *   {state: 'reading'}                    the first read is out
 *   {state: 'failed', error}              the read failed: said, never a phase
 *   {state: 'read', installed, health, …facts}
 * `installed` is true only when the record says a recorder is installed;
 * `health` is the phase. (The two names are First run's data contract,
 * screens/Onboarding/DATA.md.)
 */
export function captureOf({ doc, error, reading } = {}) {
  if (error) return { state: 'failed', error: String(error) }
  const facts = captureFacts(doc)
  if (facts) return Object.assign({ state: 'read', installed: facts.hooks.installed === true, health: facts.phase }, facts)
  return reading ? { state: 'reading' } : null
}

/** The timer seam: the real clock, unless a test hands in its own. */
const REAL_TIMERS = Object.freeze({ setInterval: (fn, ms) => setInterval(fn, ms), clearInterval: (t) => clearInterval(t) })

/**
 * The capture health of the folder in front of the person.
 *
 * Read when `projectId` is set or changes, and again on `reload()` (Look
 * again). While `poll` is true and the last answer says waiting, it is read
 * again every `every` ms — never under CAPTURE_POLL_FLOOR_MS — and a tick that
 * finds a read still out starts no second one. Polling stops by itself once
 * the answer is anything but waiting: a first step seen, a problem, a failure.
 *
 * @returns {{project: string|null, doc: object|null, facts: object|null, error: string|null, reading: boolean, reload: () => void}}
 */
export function useCapture(projectId, { poll = false, every = CAPTURE_POLL_FLOOR_MS, timers = REAL_TIMERS, read = readCapture } = {}) {
  const id = projectId || null
  const [st, setSt] = useState(() => ({ project: id, doc: null, error: null, reading: !!id }))
  const [tick, setTick] = useState(0)
  const busy = useRef(false)
  const reader = useRef(read)
  reader.current = read
  useEffect(() => {
    if (!id) {
      setSt({ project: null, doc: null, error: null, reading: false })
      return undefined
    }
    let live = true
    const ctl = typeof AbortController === 'function' ? new AbortController() : null
    busy.current = true
    setSt((s) => (s.project === id ? Object.assign({}, s, { error: null, reading: true }) : { project: id, doc: null, error: null, reading: true }))
    Promise.resolve()
      .then(() => reader.current(id, { signal: ctl ? ctl.signal : undefined }))
      .then(
        (doc) => {
          if (live) setSt({ project: id, doc, error: null, reading: false })
        },
        (e) => {
          if (!live || (e && e.name === 'AbortError')) return
          setSt({ project: id, doc: null, error: (e && e.message) || 'ThruLine could not check this folder.', reading: false })
        },
      )
      .finally(() => {
        if (live) busy.current = false
      })
    return () => {
      live = false
      busy.current = false
      if (ctl) ctl.abort()
    }
  }, [id, tick])
  const mine = st.project === id
  const doc = mine ? st.doc : null
  const facts = useMemo(() => captureFacts(doc), [doc])
  const waiting = !!poll && !!facts && facts.phase === 'waiting'
  const ms = Math.max(CAPTURE_POLL_FLOOR_MS, typeof every === 'number' && Number.isFinite(every) ? every : 0)
  useEffect(() => {
    if (!id || !waiting) return undefined
    const t = timers.setInterval(() => {
      if (!busy.current) setTick((n) => n + 1)
    }, ms)
    return () => timers.clearInterval(t)
  }, [id, waiting, ms, timers])
  return {
    project: id,
    doc,
    facts,
    error: mine ? st.error : null,
    reading: mine ? st.reading : !!id,
    /* reading is set in the same batch as the ask, so no render between the ask and the answer looks settled */
    reload: () => {
      setSt((s) => (s.project === id ? Object.assign({}, s, { reading: true }) : s))
      setTick((n) => n + 1)
    },
  }
}
