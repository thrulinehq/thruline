/* The loader for one Change on a live address.
 *
 *   useChange({ change, project }) → { data, loading, error, empty }
 *
 * `change` is either an HQ thread id (16 hex, with `project` the HQ project id
 * — when `project` is absent every project is searched) or a tracked Change id
 * (`chg_…`), resolved to its thread through the sessions it is linked to: a
 * thread id is `sha256(first work prompt event id)[..16]` (docs/hq-next/API.md),
 * so the first linked session's prompts name it.
 *
 * Where a person has tracked this work as a Change, that Change's documents
 * are read through the existing `readChangeDocuments` and derived through the
 * existing `deriveChangeModel` — criteria, coverage, readiness and decisions are
 * never re-derived here. The snapshot itself is `changeSnapshot` (change.js).
 *
 * READS ONLY. Every address below is a GET; this file writes nothing and
 * reaches no write address.
 */
import { useEffect, useRef, useState } from 'react'
import { API, readJson, storageNow } from './api.js'
import { changeSnapshot } from './change.js'

export const THREAD_ID = /^[0-9a-f]{16}$/
const REREAD_MS = 20000

/** sha256 hex of a string, through WebCrypto (browser and node ≥ 19). */
export async function sha256Hex(text) {
  const bytes = new TextEncoder().encode(String(text))
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest)).map((b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * Where the address points: `{project, thread}` HQ ids, and the tracked Change
 * id when the address named one. Throws an Error a person can read.
 *
 * @param {{change: string, project?: string}} at
 * @param {(url: string) => Promise<object>} get
 */
export async function resolveChange(at, get) {
  const id = at && at.change ? String(at.change) : ''
  if (!id) throw new Error('No Change is open. Pick one from the Inbox.')
  if (THREAD_ID.test(id)) {
    if (at.project) return { project: at.project, thread: id, trackedId: null }
    const list = await get(API.projects)
    for (const p of (list && list.projects) || []) {
      const view = await get(API.project(p.id))
      if (((view && view.threads) || []).some((t) => t.id === id)) return { project: p.id, thread: id, trackedId: null }
    }
    throw notFound()
  }
  // a tracked Change: its linked sessions name the thread
  const doc = await get(API.change(id))
  const change = doc && doc.change
  const sessions = ((change && change.associations) || []).filter((a) => a && a.provenance !== 'rejected').map((a) => String(a.session_id))
  for (const sid of sessions) {
    let timeline
    try { timeline = await get(API.session(sid)) } catch (e) { continue }
    const project = timeline && timeline.hq_project_id
    if (!project) continue
    const view = await get(API.project(project))
    const ids = new Set(((view && view.threads) || []).map((t) => t.id))
    for (const item of (timeline.items || [])) {
      if (item.kind !== 'prompt') continue
      const tid = (await sha256Hex(item.event_id)).slice(0, 16)
      if (ids.has(tid)) return { project, thread: tid, trackedId: id }
    }
  }
  throw new Error('This Change is not linked to any recorded work on this machine yet. Link a session to it, or pick one from the Inbox.')
}

const notFound = () => new Error('This Change is not in the record on this machine. Pick one from the Inbox.')

/**
 * The tracked Change a person CONFIRMED one of this thread's sessions onto, or
 * null. A merely suggested link is not tracking (`B8.8`: a suggestion is never
 * a confirmation). The newest such Change wins, so a thread a person has just
 * tracked ("Track this work as a Change") resolves to that Change on the next
 * read. The served list carries each Change's associations; a row without them
 * is read on its own. `readTracked(id)` reads one Change's documents and
 * derives its model.
 */
export async function findTracked(view, trackedId, get, readTracked) {
  if (!readTracked) return null
  if (trackedId) return readTracked(trackedId)
  const mine = new Set(((view && view.sessions) || []).map((s) => String(s.id)))
  if (!mine.size) return null
  let list
  try { list = await get(API.changes()) } catch (e) { return null }
  const rows = ((list && list.changes) || []).filter((r) => r && r.id)
    .map((r, i) => [r, i])
    .sort((a, b) => String(b[0].opened_at || '').localeCompare(String(a[0].opened_at || '')) || b[1] - a[1])
    .map(([r]) => r)
  const confirmedHere = (assocs) => (assocs || []).some((a) => a && a.provenance === 'confirmed' && mine.has(String(a.session_id)))
  for (const row of rows) {
    let assocs = Array.isArray(row.associations) ? row.associations : null
    if (!assocs) {
      let doc
      try { doc = await get(API.change(row.id)) } catch (e) { continue }
      assocs = (doc && doc.change && doc.change.associations) || []
    }
    if (confirmedHere(assocs)) return readTracked(row.id)
  }
  return null
}

/** The recorded work on screen, as the first-use route knows it: each person
 *  session of the thread and the ledger project it is filed under. Added W2-D. */
export function workOf(view) {
  return ((view && view.sessions) || [])
    .filter((s) => s && s.id && s.origin !== 'subagent')
    .map((s) => ({ id: String(s.id), ledgerProject: s.ledger_project_id ? String(s.ledger_project_id) : null }))
}

/**
 * Everything for one address, read once: the snapshot, or an Error.
 * `opts.readVersion` is the version the screen first showed (so a later read
 * on a moved version says "the code moved while you were looking").
 */
export async function loadChange(at, { get = readJson, readTracked = null, now = Date.now(), readVersion = null, loadedAt = null } = {}) {
  let where
  try {
    where = await resolveChange(at, get)
  } catch (e) {
    if (/ 404/.test(String(e && e.message))) throw notFound()
    throw e
  }
  /* The thread, its project (for member roots) and the store size are read
   * together; a cold thread read can take several seconds. The store size is
   * the shared reading (api.js `readStorage`), so a switch never waits for
   * its walk of the records folder (C27 (c)). */
  const viewP = get(API.thread(where.project, where.thread))
  /* The project view also carries this thread's `told_agent` (C30 R-LEDGER): the last time a
   * person copied a message for the agent here, or said they pasted it. It is served there and
   * not in the thread view, so it is read off the same document the member roots come from. */
  const membersP = get(API.project(where.project)).then(
    (pv) => ({
      members: ((pv && pv.project && pv.project.members) || []).map((m) => m && m.root).filter(Boolean),
      told: (((pv && pv.threads) || []).find((t) => t && t.id === where.thread) || {}).told_agent || null,
    }),
    () => ({ members: [], told: null }),
  )
  const storageP = storageNow(get).then((st) => (st && typeof st.bytes === 'number' ? st.bytes : null), () => null)
  let view
  try {
    view = await viewP
  } catch (e) {
    if (/ 404/.test(String(e && e.message))) throw notFound()
    throw new Error('ThruLine could not read this Change. Reload this page; if it keeps happening, run thruline verify.')
  }
  let tracked = null
  try { tracked = await findTracked(view, where.trackedId, get, readTracked) } catch (e) { tracked = null }
  const [{ members, told }, storageBytes] = await Promise.all([membersP, storageP])
  const snapshot = changeSnapshot(view, tracked, now, { readVersion, loadedAt, storageBytes, members, told })
  return { snapshot, where, work: workOf(view) }
}

/** Reads a tracked Change's documents and derives its model, lazily. */
export async function readTrackedChange(id) {
  const [{ readChangeDocuments }, { deriveChangeModel }] = await Promise.all([
    import('../../workspace/change-workspace.jsx'),
    import('../../workspace/change-model.js'),
  ])
  const docs = await readChangeDocuments(id, { runs: true })
  if (!docs || docs.status !== 'ready') return null
  const hq = deriveChangeModel(docs.change, docs.coverage, docs.applicability, docs.proposals, docs.work)
  return { change: docs.change, hq, proposals: docs.proposals, runSheet: await readRunSheetDoc(id) }
}

/** C30 R-RUN: the tracked Change's run sheet (`GET …/run`), or null when it
 *  cannot be read — then no run of it is drawn, never a pass. */
export async function readRunSheetDoc(id, get = readJson) {
  try {
    const doc = await get(API.run(id))
    return doc && typeof doc === 'object' ? doc : null
  } catch (_e) {
    return null
  }
}

/** The hook. Re-reads every 20 s; the first version shown is kept. */
export function useChange(at) {
  const [state, setState] = useState({ data: null, loading: true, error: null, work: [] })
  const first = useRef({ key: null, readVersion: null, loadedAt: null })
  const readNow = useRef(null)
  const key = at ? String(at.change || '') + '|' + String(at.project || '') : ''
  useEffect(() => {
    let live = true
    if (first.current.key !== key) first.current = { key, readVersion: null, loadedAt: null }
    let busy = false
    const read = async () => {
      if (busy) return // a slow read is never stacked under the next one
      busy = true
      try {
        const out = await loadChange(at, { readTracked: readTrackedChange, now: Date.now(), readVersion: first.current.readVersion, loadedAt: first.current.loadedAt })
        if (!first.current.readVersion) first.current = { key, readVersion: out.snapshot.currentVersion, loadedAt: new Date().toISOString() }
        if (live) setState({ data: out.snapshot, loading: false, error: null, work: out.work || [] })
      } catch (e) {
        if (live) setState((s) => ({ data: s.data, loading: false, error: e }))
      } finally {
        busy = false
      }
    }
    setState({ data: null, loading: true, error: null, work: [] })
    readNow.current = read
    read()
    const t = setInterval(read, REREAD_MS)
    return () => { live = false; readNow.current = null; clearInterval(t) }
  }, [key]) // eslint-disable-line react-hooks/exhaustive-deps
  /* `reload` re-reads now (after a write), without dropping what is on screen */
  const reload = () => (readNow.current ? readNow.current() : undefined)
  return { ...state, reload, empty: !!(state.data && !(state.data.statements || []).length) }
}
