/* Loading the one project model (HQ Next, W2-A): the reads the Inbox and
 * First run need, and First run's load write. Inbox hide acts live in
 * inbox-hide.js and are re-read here with the current project view.
 *
 * Reads go through data/api.js. The write is named here, beside the one
 * control that uses it (First run's "Load every session"):
 *
 *   POST /api/hq/projects/<id>/load   (HQ_LOAD_ADDRESS)
 *
 * It asks the local server to read every recording on this machine that
 * belongs to the project, through the existing ingest (append-only; a second
 * run adds only what is new). It takes no body and nothing from the browser
 * but the project id the server itself served. The browser sends the page's
 * Origin, which the server requires to be its own.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { API, forgetStorage, useResource, useStorage } from './api.js'
import { currentProjectId, firstRunData, inboxData, inboxReading, inboxRereadEvery, projectsRereadEvery, readingOf } from './project.js'
import { readyNote, useReadyChanges } from './inbox-ready.js'
import { keepYourMove, keptYourMove } from './your-move.js'
import { usePolled } from './use-polled.js'
import { INBOX_HIDES_ADDRESS } from './inbox-hide.js'
import { loadWatch } from './load-watch.js'
import { href } from '../routes.js'

/** The write address First run's "Load every session" reaches. */
export const HQ_LOAD_ADDRESS = (id) => `/api/hq/projects/${encodeURIComponent(id)}/load`
/** The read address that reports a load's progress. */
export const HQ_LOAD_STATUS = API.load

/**
 * Start reading every session of one project. Resolves to the load status the
 * server answered with; rejects with a sentence a person can read.
 * 409 (a load is already running) is not an error: its body is the status.
 */
export async function startLoad(id, fetchImpl = typeof fetch === 'function' ? fetch : null) {
  if (!id) throw new Error('No folder is chosen.')
  if (!fetchImpl) throw new Error('This browser cannot reach ThruLine’s local server.')
  let res
  try {
    res = await fetchImpl(HQ_LOAD_ADDRESS(id), { method: 'POST', credentials: 'same-origin', headers: { accept: 'application/json' } })
  } catch {
    throw new Error('ThruLine’s local server did not answer. Is `thruline ui` still running?')
  }
  let body = null
  try {
    body = await res.json()
  } catch {
    body = null
  }
  if (res.status === 202 || res.status === 409) return body
  if (res.status === 507) throw new Error('There is not enough free disk space to read every session. Free some space, then try again.')
  if (res.status === 503) throw new Error('ThruLine needs an upgrade before it can read more. Run `thruline upgrade` in a terminal, then try again.')
  if (res.status === 404) throw new Error('ThruLine no longer knows this folder. Reload the page and choose it again.')
  throw new Error('ThruLine could not start reading the sessions. Try again; if it keeps failing, run `thruline doctor` in a terminal.')
}

/**
 * `GET /api/hq/projects`, re-read every `every` ms while it holds quick rows
 * (`complete: false`), and no longer once it is complete.
 */
export function useProjects({ every = 2000, timers } = {}) {
  const [poll, setPoll] = useState(undefined)
  const list = useResource(API.projects, timers ? { every: poll, timers } : { every: poll })
  useEffect(() => {
    setPoll(projectsRereadEvery(list.data, every))
  }, [list.data, every])
  return list
}

/** The one load watcher's reading, as a hook (C27). */
export function useLoad() {
  const [load, setLoad] = useState(() => loadWatch.current())
  useEffect(() => loadWatch.subscribe(setLoad), [])
  return load
}

/** What the Inbox's header says beside the read time when a look could not finish. */
export const REFRESH_FAILED = 'could not refresh just now'

/** The Inbox's data for the project `?project=` names, else the most recently
 *  active one that has records. While a load of that project runs, its view
 *  is re-read every ~3 s so new work appears as it lands (C27). While an agent
 *  is being recorded — the view shows a thread that is the agent's move, or
 *  `machineRecording` says an agent session on this machine was just written
 *  to — it is re-read every 5 s (and not while the page is hidden, never two
 *  reads at once, never at all when nothing is recording), so rows move without
 *  a reload. A re-read that fails keeps the last good data and the header says so
 *  (`data.note`). The project's open Changes that are ready for a decision are
 *  read with it (data/inbox-ready.js) and filed as approve rows. With nothing
 *  recorded at all, the default address goes to First run. */
export function useInbox(wanted, { machineRecording = false, timers, doc } = {}) {
  const list = useProjects()
  const id = useMemo(() => currentProjectId(list.data && list.data.projects, wanted), [list.data, wanted])
  const load = useLoad()
  const view = usePolled(id ? API.project(id) : null, {
    every: (current) => inboxRereadEvery(load, id, current, machineRecording),
    timers,
    doc,
  })
  const hides = usePolled(id ? INBOX_HIDES_ADDRESS(id) : null, {
    every: () => inboxRereadEvery(load, id, view.data, machineRecording),
    timers,
    doc,
  })
  useEffect(() => {
    if (view.data) hides.reload()
  }, [view.data]) // eslint-disable-line react-hooks/exhaustive-deps
  const noRecords = !!list.data && !id && !wanted
  useEffect(() => {
    if (noRecords && typeof window !== 'undefined') window.location.replace(href('start'))
  }, [noRecords])
  const storage = useStorage()
  const readyRead = useReadyChanges(view.data)
  // a result for another project (a switch is mid-read) is not this project's
  const ready = readyRead && view.data && readyRead.project === String(view.data.project && view.data.project.id) ? readyRead : null
  // C46 §1.1: what the Inbox read is the reading every other screen's "your move" counts from too
  useEffect(() => {
    if (ready && view.data && hides.data && hides.data.project === id) keepYourMove(readingOf(view.data, ready, hides.data))
  }, [ready, view.data, hides.data, id])
  const data = useMemo(() => {
    if (!view.data || !hides.data || hides.data.project !== id) return null
    return inboxData({ projects: list.data, view: view.data, storage: storage.data, ready: ready ? ready.ready : undefined, tracked: ready ? ready.tracked : {}, readyFailed: ready ? ready.failed + (ready.listFailed ? 1 : 0) : 0, hides: hides.data, now: new Date() })
  }, [list.data, view.data, hides.data, storage.data, ready, id])
  // said beside the read time, which stays the last good one: a failed re-read does not move it
  const note = [view.data && view.error ? REFRESH_FAILED : null,
    hides.data && hides.error ? 'could not refresh hidden Inbox rows just now' : null,
    readyNote(ready)].filter(Boolean).join(' · ')
  const shown = useMemo(() => (data && note ? { ...data, note } : data), [data, note])
  const noProject = !!list.data && !id
  // C29 R1: until the view arrives, the frame is drawn from the project list (and, before even
  // that answers, from nothing); only the list area says it is reading
  const reading = useMemo(() => (view.data ? null : inboxReading({ projects: list.data, projectId: id || wanted, storage: storage.data, now: new Date(), reading: keptYourMove(id || wanted) })), [view.data, list.data, id, wanted, storage.data])
  // an error blanks the screen only when there is nothing to show; with data, it is the note above
  const error = (!list.data && list.error) || (!view.data && view.error) || (!hides.data && hides.error) || null
  return { data: shown, reading, projectId: id, noProject, loading: !data && !noProject && !error, error, reloadHides: hides.reload, reloadInbox: view.reload }
}

/** First run's data, the load's progress (re-read every second while it runs), and the load action. */
export function useFirstRun() {
  const list = useProjects()
  const storage = useStorage()
  const [polling, setPolling] = useState(false)
  const load = useResource(API.load, { every: polling ? 1000 : undefined })
  const [startError, setStartError] = useState(null)
  const running = !!(load.data && load.data.running)
  const wasRunning = useRef(false)
  const [finishedFor, setFinishedFor] = useState(null)
  useEffect(() => {
    setPolling(running)
    if (wasRunning.current && !running && load.data && load.data.finished_at) {
      forgetStorage() // the load added records: the next reading measures again
      setFinishedFor(load.data.project || null)
    }
    wasRunning.current = running
  }, [running, load.data])
  const data = useMemo(() => (list.data ? firstRunData({ projects: list.data, load: load.data, storage: storage.data, now: new Date() }) : null), [list.data, load.data, storage.data])
  /* C27: a load never makes the person wait — it starts, and the caller opens
   * the Inbox at once; the status bar follows the load from there. */
  const start = async (id) => {
    setStartError(null)
    try {
      await startLoad(id)
      setPolling(true)
      load.reload()
      loadWatch.poke()
      return true
    } catch (e) {
      setStartError(e.message)
      return false
    }
  }
  return { data, loading: !data && !list.error, error: list.error || null, startError, start, finishedFor, reloadProjects: list.reload }
}
