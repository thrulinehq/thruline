/* First run, step 3, "First steps recorded" (census §3.11 rows 8 and 17): the
 * first thing the person asked in this folder, and the thread of work it
 * opened — which the Inbox lists as a Change to track, and which "Review
 * what counts as done →" opens.
 *
 * Two existing reads, and nothing else:
 *   GET /api/hq/projects/<id>                  every thread, with when it started
 *   GET /api/hq/projects/<id>/threads/<tid>    the earliest thread's record
 * The request and its step come from the one Change model
 * (data/change.js `changeSnapshot`), so step 3 names the same step the
 * Change's own screens name. Nothing is written.
 *
 * A project whose record holds no thread answers `{thread: null}`: there is
 * then no Done link at all (C30 R-LINKS: never the Inbox in its place). */
import { useEffect, useState } from 'react'
import { API, readJson } from '../../data/api.js'
import { changeSnapshot } from '../../data/change.js'

const ms = (ts) => {
  const t = typeof ts === 'string' ? Date.parse(ts) : NaN
  return Number.isNaN(t) ? null : t
}

/** The thread that started first; a thread with no start time comes last. */
export function firstThread(threads) {
  const list = (Array.isArray(threads) ? threads : []).filter((t) => t && typeof t.id === 'string' && t.id)
  if (!list.length) return null
  return list.slice().sort((a, b) => {
    const x = ms(a.started_at)
    const y = ms(b.started_at)
    if (x === null && y === null) return 0
    if (x === null) return 1
    if (y === null) return -1
    return x - y
  })[0]
}

/**
 * The first ask of one project.
 * @returns {Promise<{thread: string|null, title?: string, step?: number|null, text?: string|null}>}
 */
export async function readFirstAsk(projectId, { get = readJson, now = new Date() } = {}) {
  const view = await get(API.project(projectId))
  const first = firstThread(view && view.threads)
  if (!first) return { thread: null }
  const tv = await get(API.thread(projectId, first.id))
  const snap = changeSnapshot(tv, null, now)
  const req = snap.request || null
  return {
    thread: first.id,
    title: snap.change.title,
    step: req && typeof req.step === 'number' ? req.step : null,
    text: req && req.text ? req.text : null,
  }
}

/**
 * The first ask, read once per project when `enabled` turns true (step 3
 * open, and the record says it is recording). `{project, state, …}` where
 * state is reading · read · failed; null until it is asked for.
 */
export function useFirstAsk(projectId, enabled, { get } = {}) {
  const [st, setSt] = useState(null)
  const want = !!projectId && !!enabled
  const have = !!st && st.project === projectId && st.state === 'read'
  useEffect(() => {
    if (!want || have) return undefined
    let live = true
    setSt({ project: projectId, state: 'reading' })
    readFirstAsk(projectId, get ? { get } : {}).then(
      (ask) => live && setSt(Object.assign({ project: projectId, state: 'read' }, ask)),
      () => live && setSt({ project: projectId, state: 'failed' }),
    )
    return () => {
      live = false
    }
  }, [projectId, want]) // eslint-disable-line react-hooks/exhaustive-deps
  return st && st.project === projectId ? st : null
}
