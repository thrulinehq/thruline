/* The one place HQ Next reads from the local server.
 *
 * Every screen asks for what it draws through `useResource(url)`. A read that
 * fails is a failure the screen states (Rule 12) — never an empty screen that
 * reads like "nothing happened". Reads only: the write routes keep their own
 * named constants beside the one control that uses each (CLAUDE.md, the
 * write-address table).
 */
import { useEffect, useRef, useState } from 'react'

export const API = Object.freeze({
  projects: '/api/hq/projects',
  project: (id) => `/api/hq/projects/${encodeURIComponent(id)}`,
  thread: (project, thread) => `/api/hq/projects/${encodeURIComponent(project)}/threads/${encodeURIComponent(thread)}`,
  session: (id) => `/api/hq/sessions/${encodeURIComponent(id)}`,
  load: '/api/hq/load',
  change: (id) => `/api/changes/${encodeURIComponent(id)}`,
  changeIntegrity: (id) => `/api/changes/${encodeURIComponent(id)}/integrity`,
  changes: (project) => (project ? `/api/changes?project=${encodeURIComponent(project)}` : '/api/changes'),
  readiness: (id) => `/api/changes/${encodeURIComponent(id)}/readiness`,
  proposals: (id) => `/api/changes/${encodeURIComponent(id)}/proposals`,
  applicability: (id) => `/api/changes/${encodeURIComponent(id)}/applicability`,
  run: (id) => `/api/changes/${encodeURIComponent(id)}/run`,
  storage: '/api/storage',
  state: '/api/state',
})

/** Read one JSON document. Rejects with a message a person can read. */
export async function readJson(url, { signal } = {}) {
  let res
  try {
    res = await fetch(url, { signal, headers: { accept: 'application/json' } })
  } catch (e) {
    if (e && e.name === 'AbortError') throw e
    // `unreachable`: the browser got no answer at all (HQ is not running), as against an answer that was an error
    throw Object.assign(new Error(`ThruLine's local server did not answer (${url}).`), { unreachable: true })
  }
  const text = await res.text()
  // `status` rides on the error so a caller can tell a gateway with nothing behind it (502/503/504) from HQ answering badly
  if (!res.ok) throw Object.assign(new Error(`${url} answered ${res.status}${text ? `: ${text.slice(0, 200)}` : ''}`), { status: res.status })
  try {
    return JSON.parse(text)
  } catch {
    throw new Error(`${url} did not answer with JSON.`)
  }
}

/* What ThruLine keeps on this machine (L1.1) is measured by walking the whole
 * records folder: about a second on a large store, and every screen's status
 * bar states it. One reading is shared and kept for a minute, so opening or
 * switching a screen never waits for that walk (C27 (c)). The reading is kept
 * per reader (`get`), so a test's fake reader never sees another's answer; a
 * failed reading is not kept. `forgetStorage()` drops it after a load adds
 * records. */
export const STORAGE_MAX_AGE_MS = 60_000
let storageReading = null // { get, at, promise }
let storageSettled = null // { get, data }: the last reading that answered

/** The shared storage reading: a fresh one when none is younger than a minute. */
export function readStorage(get = readJson, now = Date.now()) {
  const kept = storageReading
  if (kept && kept.get === get && now - kept.at >= 0 && now - kept.at < STORAGE_MAX_AGE_MS) return kept.promise
  const promise = get(API.storage)
  const reading = { get, at: now, promise }
  storageReading = reading
  promise.then(
    (data) => {
      if (storageReading === reading) storageSettled = { get, data }
    },
    () => {
      if (storageReading === reading) storageReading = null
    },
  )
  return promise
}

/** The last reading that answered, at once, while a fresh one is measured
 *  when it is older than a minute; with none yet, the reading itself. What a
 *  Change screen states (C27 (c): a switch never waits for the walk). */
export function storageNow(get = readJson, now = Date.now()) {
  const reading = readStorage(get, now)
  const settled = storageSettled
  return settled && settled.get === get ? Promise.resolve(settled.data) : reading
}

/** Drop the shared storage reading, so the next one measures again. */
export function forgetStorage() {
  storageReading = null
  storageSettled = null
}

/** `{data}`: the shared storage reading, for screens that state it. */
export function useStorage() {
  const [data, setData] = useState(null)
  useEffect(() => {
    let live = true
    readStorage().then(
      (d) => live && setData(d),
      () => {},
    )
    return () => {
      live = false
    }
  }, [])
  return { data }
}

/** The timer seam: the real clock, unless a test hands in its own. */
const REAL_TIMERS = Object.freeze({ setInterval: (fn, ms) => setInterval(fn, ms), clearInterval: (t) => clearInterval(t) })

/**
 * `{data, error, loading, reload}` for one URL (or null: nothing to read).
 * `every` (ms) re-reads on a timer while the screen is open, for the screens
 * that follow work as it happens. `timers` is the clock seam (tests drive the
 * re-read schedule by ticks, never by wall-clock).
 */
export function useResource(url, { every, timers = REAL_TIMERS } = {}) {
  const [state, setState] = useState({ data: null, error: null, loading: !!url })
  const [tick, setTick] = useState(0)
  const live = useRef(true)
  useEffect(() => {
    live.current = true
    return () => {
      live.current = false
    }
  }, [])
  useEffect(() => {
    if (!url) {
      setState({ data: null, error: null, loading: false })
      return undefined
    }
    const ctl = new AbortController()
    setState((s) => ({ ...s, loading: true }))
    readJson(url, { signal: ctl.signal }).then(
      (data) => live.current && setState({ data, error: null, loading: false }),
      (error) => live.current && error.name !== 'AbortError' && setState((s) => ({ data: s.data, error, loading: false })),
    )
    return () => ctl.abort()
  }, [url, tick])
  useEffect(() => {
    if (!url || !every) return undefined
    const t = timers.setInterval(() => setTick((n) => n + 1), every)
    return () => timers.clearInterval(t)
  }, [url, every, timers])
  return { ...state, reload: () => setTick((n) => n + 1) }
}
