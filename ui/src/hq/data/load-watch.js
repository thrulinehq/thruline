/* The one watcher of `GET /api/hq/load` (C27): every screen's status bar and
 * the Inbox's re-reads follow the same reading, so two screens never disagree
 * about whether ThruLine is reading new recordings.
 *
 * Reads only. While a load is running it re-reads every `fast` ms; otherwise
 * every `slow` ms, so a load started elsewhere (a terminal, another tab)
 * still shows up. It reads only while something is subscribed.
 */
import { API, readJson } from './api.js'

export function createLoadWatch({ read = () => readJson(API.load), fast = 3000, slow = 15000, setTimer = setTimeout, clearTimer = clearTimeout } = {}) {
  let data = null
  let timer = null
  let busy = false
  const subs = new Set()
  const running = () => !!(data && data.running)
  const schedule = () => {
    if (timer) clearTimer(timer)
    timer = subs.size ? setTimer(tick, running() ? fast : slow) : null
  }
  async function tick() {
    timer = null
    if (busy || !subs.size) return
    busy = true
    try {
      data = await read()
    } catch {
      // a failed read leaves the last reading; the status bar says nothing it does not know
    }
    busy = false
    for (const fn of subs) fn(data)
    schedule()
  }
  return {
    /** Follow the load; `fn(load)` on every reading. Returns the unsubscribe. */
    subscribe(fn) {
      subs.add(fn)
      if (data) fn(data)
      if (subs.size === 1) tick()
      return () => {
        subs.delete(fn)
        if (!subs.size && timer) { clearTimer(timer); timer = null }
      }
    },
    current: () => data,
    /** Read now (after starting a load). */
    poke: () => tick(),
  }
}

/** The app's one watcher. */
export const loadWatch = createLoadWatch()
