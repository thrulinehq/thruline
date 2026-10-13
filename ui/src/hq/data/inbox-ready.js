/* Which of a project's tracked Changes are ready for a decision (the Inbox's approve row).
 *
 * A thread of work has no readiness; a tracked Change does. So the Inbox reads, for the project's
 * open Changes (the 20 most recently active), each one's readiness — `GET /api/changes/<id>/readiness`
 * — and for each one that says it is ready, which thread of work it stands for, through the join
 * the Change screen already uses (`resolveChange`, use-change.js). Nothing is matched here a second way.
 *
 * NOT-APPROVAL-WORDS: this file files a Change that IS ready as the Inbox's approve row and says
 * nothing about a Change that is not; the only words it makes are "could not check … for approval",
 * about a look that failed, never about why approval is off (that is blockerOf's, on the Change screen).
 *
 * READS ONLY: every address below is a GET. A read that fails says nothing false: that Change gets
 * no approve row and the failure is counted, so the screen can say it (`readyNote`). A Change is
 * never filed as ready on a guess, on a stale answer, or because a read was skipped.
 */
import { useEffect, useRef, useState } from 'react'
import { API, readJson } from './api.js'
import { resolveChange } from './use-change.js'
import { shortVersion } from './project.js'

/** How many Changes are looked at, most recently active first. */
export const READY_CAP = 20
/** How many readiness reads are in flight at once. */
const POOL = 4

/**
 * What a readiness document says, when it says "ready for a decision": the plain answer only
 * (`readiness_ready`). "Ready with exceptions" rests in part on something nobody checked and is
 * left to the Change screen's own words, so this row never calls it plain-ready. Null otherwise.
 *
 * @returns {{version: string, target: string, criteria: object|null}|null}
 */
export function readyOf(doc) {
  const r = doc && doc.readiness
  if (!r || r.ready !== true || r.with_exceptions === true || r.headline_slug !== 'readiness_ready') return null
  const version = shortVersion(r.target_version)
  if (!version) return null
  // the lines a ready answer rests on, as the row draws them: every one satisfied by evidence
  const lines = (Array.isArray(r.obligations) ? r.obligations : [])
    .filter((o) => o && o.kind === 'criterion' && o.outcome === 'satisfied')
    .map((o, i) => ({ n: Number.isInteger(o.criterion_index) && o.criterion_index > 0 ? o.criterion_index : i + 1, state: 'proven', gate: false }))
  const scope = Number.isInteger(r.scope_version) ? `v${r.scope_version}` : null
  return { version, target: String(r.target_version), criteria: lines.length ? { version: scope, lines } : null }
}

/** When a Change was last active: its opening, or its newest link. */
function activeAt(row) {
  let best = String(row.opened_at || '')
  for (const a of row.associations || []) if (a && String(a.asserted_at || '') > best) best = String(a.asserted_at)
  return best
}

/** A Change's linked sessions as they stand (a rejected link is not one). */
const linked = (row) => (Array.isArray(row.associations) ? row.associations : []).filter((a) => a && a.provenance !== 'rejected')
const confirmed = (row) => linked(row).some((a) => a.provenance === 'confirmed')

/** An accepted scope is a real list even when readiness says its proof is missing. */
function trackedOf(row, doc, run) {
  const scope = row.accepted_scope
  const obligations = (doc && doc.readiness && doc.readiness.obligations) || []
  const outcomes = new Map(obligations.filter((o) => o && o.kind === 'criterion').map((o) => [o.criterion_index, o.outcome]))
  const criteria = scope && scope.acceptance && Array.isArray(scope.criteria)
    ? { version: `v${scope.version}`, lines: scope.criteria.map((_line, i) => ({ n: i + 1, state: outcomes.get(i + 1) === 'satisfied' ? 'proven' : 'notProven', gate: false })) }
    : null
  const standing = row.decisions && row.decisions.standing
  /* `current`: whether that approval names the version the Change is on now. The Change's own answer
   * is its readiness `target_version`; a thread's `current_version_id` is another id space (a Change's
   * target is a snapshot id, a thread's version is a commit or `wt-…`), so the two are never compared.
   * Without a readiness target nothing is said (`undefined`), and the caller keeps its older rule. */
  const target = doc && doc.readiness && typeof doc.readiness.target_version === 'string' && doc.readiness.target_version ? doc.readiness.target_version : null
  const approved = standing && standing.act === 'approved' && standing.target_revision
    ? { version: standing.target_revision, actor: standing.actor || 'you', at: standing.decided_at || standing.recorded_at || null,
      ...(target ? { current: standing.target_revision === target } : {}) }
    : null
  const check = run && Array.isArray(run.checks) ? run.checks.find((c) => c && typeof c.command === 'string' && c.command) : null
  return { id: row.id, criteria, approved, checkCommand: check ? check.command : null }
}

/** Whether an approval already stands for exactly this version. */
const approvedAlready = (row, target) => !!(row.decisions && row.decisions.standing && row.decisions.standing.target_revision === target)

/** Run `fn` over `items`, `size` at a time; results in item order. */
async function pooled(items, size, fn) {
  const out = new Array(items.length)
  let next = 0
  const worker = async () => {
    while (next < items.length) {
      const i = next++
      out[i] = await fn(items[i])
    }
  }
  await Promise.all(Array.from({ length: Math.min(size, items.length) }, worker))
  return out
}

/**
 * Read the readiness of the project's open Changes and place each ready one on its thread.
 *
 * @param {object} view GET /api/hq/projects/<id> (its `project.members` name the ledger projects folded into it)
 * @param {{get?: Function, cap?: number, resolve?: Function, placed?: Map}} opts
 *   `placed` remembers which thread a Change stands for while its links do not change, so a re-read
 *   every few seconds does not walk the sessions again.
 * @returns {Promise<{project: string, ready: object, tracked: object, failed: number, listFailed: boolean}>}
 *   `ready[threadId] = {change, version, criteria}`
 */
export async function readReadyChanges(view, { get = readJson, cap = READY_CAP, resolve = resolveChange, placed = new Map(), runCache = new Map() } = {}) {
  const project = view && view.project
  const pid = project && project.id ? String(project.id) : null
  const out = { project: pid, ready: {}, tracked: {}, failed: 0, listFailed: false }
  const members = new Set(((project && project.members) || []).map((m) => m && m.ledger_project_id).filter(Boolean).map(String))
  if (!pid || !members.size) return out
  let list
  try {
    list = await get(API.changes())
  } catch (e) {
    return { ...out, listFailed: true }
  }
  const rows = ((list && list.changes) || [])
    .filter((r) => r && r.id && members.has(String(r.project_id)))
    .map((r, i) => [r, i])
    .sort((a, b) => activeAt(b[0]).localeCompare(activeAt(a[0])) || a[1] - b[1])
    .slice(0, cap)
    .map(([r]) => r)
  const found = await pooled(rows, POOL, async (row) => {
    let doc
    try {
      doc = await get(API.readiness(row.id))
    } catch (e) {
      return { failed: true }
    }
    const rd = readyOf(doc)
    if (!linked(row).length) return null
    if (!rd && !confirmed(row)) return null
    const key = row.id + '|' + linked(row).map((a) => `${a.session_id}:${a.provenance}`).join(',')
    let at = placed.get(key)
    if (!at) {
      try {
        at = await resolve({ change: row.id }, get)
      } catch (e) {
        return rd ? { failed: true } : null
      }
      placed.set(key, at)
    }
    if (!at || String(at.project) !== pid || !at.thread) return null
    let run = null
    if (confirmed(row) && row.accepted_scope && row.accepted_scope.acceptance) {
      const target = doc && doc.readiness && doc.readiness.target_version
      const runKey = typeof target === 'string' && target ? `${row.id}:${row.accepted_scope.version}:${target}` : null
      if (runKey && runCache.has(runKey)) run = runCache.get(runKey)
      else {
        try { run = await get(API.run(row.id)); if (runKey) runCache.set(runKey, run) } catch (e) { /* no command is claimed when the run sheet could not be read */ }
      }
    }
    return { thread: at.thread,
      ready: rd && !approvedAlready(row, rd.target) ? { change: row.id, version: rd.version, criteria: rd.criteria } : null,
      tracked: confirmed(row) ? trackedOf(row, doc, run) : null }
  })
  // the most recently active Change keeps a thread two of them stand for
  for (const f of found) {
    if (!f) continue
    if (f.failed) out.failed += 1
    else {
      if (f.ready && !out.ready[f.thread]) out.ready[f.thread] = f.ready
      if (f.tracked && !out.tracked[f.thread]) out.tracked[f.thread] = f.tracked
    }
  }
  return out
}

/** What the header says beside the read time when a look could not finish; null when it did. */
export function readyNote(r) {
  if (!r) return null
  if (r.listFailed) return 'could not check Changes for approval'
  return r.failed > 0 ? `could not check ${r.failed} ${r.failed === 1 ? 'Change' : 'Changes'} for approval` : null
}

/**
 * The ready Changes of the project `view` shows, re-read whenever the view is (so a poll of the
 * view moves them too), never two reads at once: a view that arrives while a read is running is
 * read next, and only the newest is.
 *
 * @returns {{project: string, ready: object, failed: number, listFailed: boolean}|null}
 */
export function useReadyChanges(view, { get = readJson } = {}) {
  const [result, setResult] = useState(null)
  const latest = useRef(null)
  const busy = useRef(false)
  const done = useRef(null)
  const mounted = useRef(true)
  const placed = useRef(new Map())
  const runCache = useRef(new Map())
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  useEffect(() => {
    latest.current = view || null
    if (!view) {
      setResult(null)
      return
    }
    const pump = async () => {
      if (busy.current) return
      busy.current = true
      try {
        while (latest.current && latest.current !== done.current) {
          const v = latest.current
          done.current = v
          let r
          try {
            r = await readReadyChanges(v, { get, placed: placed.current, runCache: runCache.current })
          } catch (e) {
            r = { project: v.project && v.project.id ? String(v.project.id) : null, ready: {}, failed: 0, listFailed: true }
          }
          if (mounted.current) setResult(r)
        }
      } finally {
        busy.current = false
      }
    }
    pump()
  }, [view, get])
  return result
}
