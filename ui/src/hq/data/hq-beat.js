/* Whether HQ itself is still running, as seen from an open page (Settings, DESIGN-SPEC §3.10
 * "System-state bar conditions", stopped).
 *
 * The page is served by HQ, so a page that has just opened is not stopped. While it stays open it
 * asks HQ's cheap in-memory state route (`GET /api/state`, a read) every few seconds, and HQ is
 * stopped when a read gets NO answer at all (`readJson` tags that `unreachable`) or the answer is a
 * gateway's "nothing behind me" (502/503/504: a front that proxies /api/ to HQ answers that once HQ is
 * gone, as the census's front does; `thruline ui` itself gives a network failure). Any other HTTP error
 * is HQ answering badly, which is not stopped, and a read that answers again clears it. This is the only
 * thing that sets `hq.stopped` on the live page; before it, the page hard-coded `false` and noticed
 * nothing. It asks only while the Settings screen is mounted and the page is visible. READS ONLY: no write address.
 */
import { useEffect, useState } from 'react'
import { API, readJson } from './api.js'

/** How often (ms) the open page asks whether HQ is still there: a stop shows within a few seconds. */
export const HQ_BEAT_MS = 3000

const REAL_TIMERS = Object.freeze({ setInterval: (fn, ms) => setInterval(fn, ms), clearInterval: (t) => clearInterval(t) })

/** The statuses a gateway or proxy answers with when the server behind it is gone. */
const GATEWAY_STATUSES = Object.freeze([502, 503, 504])

/** True for an error the read layer tagged as "nobody answered", and for a gateway status (HQ is not behind it). */
export function hqUnreachable(error) {
  return !!error && (error.unreachable === true || GATEWAY_STATUSES.includes(error.status))
}

/**
 * `true` while HQ cannot be reached. `read`, `timers` and `doc` are the test seams (a stub reader, a fake
 * clock, a stubbed document).
 *
 * The beat runs only while the screen that calls this is mounted AND the page is visible
 * (`document.visibilityState === 'visible'`; CTO ruling 2026-10-12): a hidden tab asks nothing, on
 * becoming visible it reads once at once and resumes, and unmount clears the timer and the listener.
 * With no document at all (nothing to ask) it behaves as visible. The stopped/gateway classification
 * is `hqUnreachable`, unchanged.
 *
 * @returns {boolean}
 */
export function useHqStopped({ every = HQ_BEAT_MS, read = readJson, timers = REAL_TIMERS, doc } = {}) {
  const [stopped, setStopped] = useState(false)
  useEffect(() => {
    const page = doc === undefined ? (typeof document === 'undefined' ? null : document) : doc
    const visible = () => !page || page.visibilityState === 'visible'
    let live = true
    let t = null
    const beat = () => {
      read(API.state).then(
        () => { if (live) setStopped(false) },
        (error) => { if (live && hqUnreachable(error)) setStopped(true) },
      )
    }
    const stop = () => {
      if (t !== null) {
        timers.clearInterval(t)
        t = null
      }
    }
    const sync = () => {
      if (!live) return
      if (!visible()) return stop()
      if (t !== null) return
      beat() // back in view: one read at once, then the beat resumes
      t = timers.setInterval(beat, every)
    }
    if (visible()) t = timers.setInterval(beat, every) // no extra read at load: the page was just served by HQ
    if (page) page.addEventListener('visibilitychange', sync)
    return () => {
      live = false
      stop()
      if (page) page.removeEventListener('visibilitychange', sync)
    }
  }, [every, read, timers, doc])
  return stopped
}
