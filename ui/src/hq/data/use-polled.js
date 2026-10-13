/* One address, read now and then re-read on a timer while it matters.
 *
 *   usePolled(url, { every }) → { data, error, loading, reload }
 *
 * What `useResource` (api.js) is not: a tick never starts a read while the last one is still
 * running (a slow read is never cut short and restarted by the next tick), a hidden page is not
 * read at all (and is read once, at once, when it comes back), and a read that fails keeps the
 * last good `data` — `error` then says so beside it, and the next good read clears it. A page
 * with nothing yet and a failed first read has `error` and no `data`.
 *
 * `every` is a number (ms), undefined for "never", or a function of the last good `data` that
 * returns one — so the document being read can decide whether it keeps being read.
 *
 * READS ONLY. `timers` is the clock seam and `doc` the visibility seam (tests drive both).
 */
import { useEffect, useRef, useState } from 'react'
import { readJson } from './api.js'

const REAL_TIMERS = Object.freeze({ setInterval: (fn, ms) => setInterval(fn, ms), clearInterval: (t) => clearInterval(t) })

/** Whether the page is in front of the person. With no document (a test, a worker) it is. */
export function pageVisible(doc = typeof document !== 'undefined' ? document : null) {
  return !doc || doc.visibilityState !== 'hidden'
}

export function usePolled(url, { every, timers = REAL_TIMERS, read = readJson, doc = typeof document !== 'undefined' ? document : null } = {}) {
  const [state, setState] = useState({ data: null, error: null, loading: !!url })
  const run = useRef(null)
  useEffect(() => {
    if (!url) {
      run.current = null
      setState({ data: null, error: null, loading: false })
      return undefined
    }
    const me = { live: true, busy: false, ctl: new AbortController() }
    run.current = me
    me.read = async () => {
      if (!me.live || me.busy) return false // never two reads of this address at once
      me.busy = true
      try {
        const data = await read(url, { signal: me.ctl.signal })
        if (me.live) setState({ data, error: null, loading: false })
      } catch (error) {
        if (me.live && !(error && error.name === 'AbortError')) setState((s) => ({ data: s.data, error, loading: false }))
      } finally {
        me.busy = false
      }
      return true
    }
    setState({ data: null, error: null, loading: true })
    me.read()
    return () => {
      me.live = false
      me.ctl.abort()
    }
  }, [url, read])
  const ms = typeof every === 'function' ? every(state.data) : every
  useEffect(() => {
    const me = run.current
    if (!url || !ms || !me) return undefined
    const tick = () => {
      if (pageVisible(doc)) me.read()
    }
    const t = timers.setInterval(tick, ms)
    // back in front after a stretch hidden: read once now rather than wait out the interval
    const onShow = () => {
      if (pageVisible(doc)) me.read()
    }
    if (doc && doc.addEventListener) doc.addEventListener('visibilitychange', onShow)
    return () => {
      timers.clearInterval(t)
      if (doc && doc.removeEventListener) doc.removeEventListener('visibilitychange', onShow)
    }
  }, [url, ms, timers, doc, read])
  return { ...state, reload: () => (run.current ? run.current.read() : undefined) }
}
