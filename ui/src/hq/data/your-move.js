/* "Your move": the ONE reading every screen counts it from (C46 §1.1; C30 P0-5).
 *
 * The count itself is `yourMove` in data/project.js: the engine's count of the person's threads, plus each
 * thread the engine calls finished whose tracked Change is ready for a decision (the Inbox files
 * that thread as its approve row, "Waiting on you"). That needs a readiness read, which only the
 * Inbox made; so every other screen's badge said one fewer. This file is that read, shared:
 *
 *   readYourMove(row)   reads the project's open Changes' readiness through the Inbox's own read
 *                       (inbox-ready.js `readReadyChanges`, the join use-change.js `resolveChange`
 *                       makes) and, for the threads a ready Change lands on, the engine's move of
 *                       each (the project's view, which the join has just read: one read, not two)
 *   keepYourMove(r)     the Inbox and Changes keep what they read themselves, in the same shape
 *   keptYourMove(id)    the newest reading of a project, or null
 *   useYourMove(...)    a screen's reading for the project in front of the person: the kept one at
 *                       once, and a fresh read each time the screen opens or the project's row moves
 *
 * NOT-APPROVAL-WORDS: a reading names the Changes that ARE ready and the threads they land on, for
 * a count; it holds no words, and nothing here says why approval is off.
 *
 * READS ONLY: every address is a GET (inbox-ready.js, use-change.js). A reading is kept in memory,
 * for this page only; it is never written anywhere. A look that failed is kept as failed, and
 * `yourMove` then says no number at all rather than one that may be short.
 */
import { useEffect, useState } from 'react'
import { API, readJson } from './api.js'
import { readReadyChanges } from './inbox-ready.js'
import { readingOf } from './project.js'
import { INBOX_HIDES_ADDRESS } from './inbox-hide.js'

const kept = new Map() // project id → the newest reading
const inflight = new Map() // project id → { get, promise }
const listeners = new Set()

const tell = (id) => {
  for (const fn of listeners) fn(id)
}

/** Keep a reading (`project.js` `readingOf` shape) as the newest for its project. */
export function keepYourMove(reading) {
  if (!reading || !reading.project) return
  kept.set(String(reading.project), reading)
  tell(String(reading.project))
}

/** The newest kept reading of a project, or null. */
export function keptYourMove(id) {
  return (id != null && kept.get(String(id))) || null
}

/** Drop every kept reading (a test's seam; nothing in the app forgets one). */
export function forgetYourMove() {
  kept.clear()
  inflight.clear()
}

/** Be told when a project's kept reading changes. Returns the unsubscribe. */
export function subscribeYourMove(fn) {
  listeners.add(fn)
  return () => listeners.delete(fn)
}

/** One pass's GETs, each address read at most once (the join and the moves share the view; Changes'
 *  projects share one list of Changes). */
export function readOnce(get = readJson) {
  const seen = new Map()
  return (url) => {
    if (!seen.has(url)) seen.set(url, get(url))
    return seen.get(url)
  }
}

/**
 * Read one project's reading from the local server.
 *
 * @param {object} row  the project's row of `GET /api/hq/projects` (its `id` and `members`)
 * @param {{get?: Function, keep?: boolean}} opts  `keep`: share the read with any other screen
 *   asking at the same moment, and keep its answer (keepYourMove)
 * @returns {Promise<object>} `readingOf`'s shape
 */
export function readYourMove(row, { get = readJson, keep = false } = {}) {
  const id = row && row.id != null ? String(row.id) : null
  if (!id) return Promise.resolve(null)
  if (keep && id) {
    const running = inflight.get(id)
    if (running && running.get === get) return running.promise
  }
  const promise = (async () => {
    const g = readOnce(get)
    let view, hides
    try {
      [view, hides] = await Promise.all([g(API.project(id)), g(INBOX_HIDES_ADDRESS(id))])
    } catch {
      // Without the thread rows or their hide acts, a badge could be too high.
      return readingOf({ project: row, threads: [] }, { ready: {}, failed: 1, listFailed: true })
    }
    const r = await readReadyChanges(view, { get: g })
    return readingOf(view, r, hides)
  })()
  if (keep && id) {
    const entry = { get, promise }
    inflight.set(id, entry)
    promise.then(
      (reading) => {
        if (inflight.get(id) === entry) inflight.delete(id)
        keepYourMove(reading)
      },
      () => {
        if (inflight.get(id) === entry) inflight.delete(id)
      },
    )
  }
  return promise
}

/** A key that moves when the project's row does: a new read is then due. */
const rowKey = (row) => (row ? [row.id, row.last_activity, JSON.stringify(row.whose_move || null)].join('|') : '')

/**
 * The reading of the project in front of the person, for a screen that draws "your move" without
 * reading the readiness itself: the kept one at once (null when none yet), then a fresh read when
 * the screen opens and whenever the project's row moves. Null for no row.
 */
export function useYourMove(row, { get = readJson } = {}) {
  const id = row && row.id != null ? String(row.id) : null
  const [reading, setReading] = useState(() => keptYourMove(id))
  const key = rowKey(row)
  useEffect(() => {
    setReading(keptYourMove(id))
    if (!id) return undefined
    return subscribeYourMove((changed) => {
      if (changed === id) setReading(keptYourMove(id))
    })
  }, [id])
  useEffect(() => {
    if (!row || !id) return
    readYourMove(row, { get, keep: true }).catch(() => {})
  }, [key, get]) // eslint-disable-line react-hooks/exhaustive-deps
  return reading
}
