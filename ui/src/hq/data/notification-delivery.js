/* Per-browser delivery memory. Only opaque event IDs and recorder transitions
 * live here. Persist before showing a notification, so a reload never repeats
 * one item. If browser storage is unavailable, delivery stays off. */
import { href } from '../routes.js'

export const DELIVERY_KEY = 'thruline.hq.notifications.v1'
const KINDS = ['ready', 'not_backed', 'agent_waiting', 'recording_stopped']
const empty = () => ({ seen: {}, enabled: {}, recording: {}, stops: {} })

function read(storage) {
  try {
    const value = JSON.parse(storage.getItem(DELIVERY_KEY) || 'null')
    return value && typeof value === 'object' && value.seen && value.enabled && value.recording && value.stops ? value : empty()
  } catch { return null }
}
function keep(storage, state) {
  try { storage.setItem(DELIVERY_KEY, JSON.stringify(state)); return true }
  catch { return false }
}

export function createNotificationDelivery({ storage = typeof localStorage === 'undefined' ? null : localStorage,
  NotificationClass = typeof Notification === 'undefined' ? null : Notification,
  open = (url) => { window.focus(); window.location.hash = url },
} = {}) {
  let state = storage && read(storage)
  // Capture health has no durable stop receipt. A prior state from a closed
  // page must never imply a stop happened while this page was open.
  const observedRecordings = {}
  // Ready and waiting facts have no arrival feed. Baseline each project's
  // first complete read in this page, even when its switch stayed On across
  // a reload, so an item from while HQ was closed does not announce.
  const observedProjects = new Set()
  let openProject = null
  const available = () => !!(storage && state)
  const persist = () => available() && keep(storage, state)

  function disableOff(enabled) {
    if (!available()) return false
    for (const kind of KINDS) if (!enabled[kind]) state.enabled[kind] = false
    return persist()
  }

  function send(event) {
    if (!NotificationClass || NotificationClass.permission !== 'granted') return false
    // This mark must be durable before the browser receives the item.
    state.seen[event.key] = true
    if (!persist()) { delete state.seen[event.key]; return false }
    try {
      const note = new NotificationClass(event.title, { body: event.body, tag: event.key })
      if (note) note.onclick = () => open(event.url)
      return true
    } catch {
      delete state.seen[event.key]
      persist()
      return false
    }
  }

  function deliver(facts, enabled) {
    if (!available()) return { sent: 0, reason: 'Browser storage is unavailable, so ThruLine cannot ensure one notification per item.' }
    const project = facts?.projectId || null
    if (project !== openProject) {
      // Returning to a project starts a new observation. Items and recorder
      // transitions from while another project was open are a baseline.
      observedProjects.clear()
      for (const id of Object.keys(observedRecordings)) delete observedRecordings[id]
      openProject = project
    }
    const permissionReason = !NotificationClass || NotificationClass.permission !== 'granted'
      ? 'Browser notifications are not allowed now. The switches are saved, but this page cannot notify; turn a switch Off and On to ask again.'
      : null
    const events = Array.isArray(facts && facts.events) ? facts.events.slice() : []
    if (enabled.recording_stopped) {
      for (const current of (facts && facts.recordings) || []) {
        if (!current || !current.project) continue
        const previous = observedRecordings[current.project]
        if (state.enabled.recording_stopped && previous && previous.active && current.stopped) {
          const seq = (state.stops[current.project] || 0) + 1
          state.stops[current.project] = seq
          events.push({ key: `recording_stopped:${current.project}:${seq}`, kind: 'recording_stopped', title: 'Recording stopped', body: 'ThruLine stopped recording a project while this HQ page was open.', url: href('settings', { project: current.project, pane: 'agents' }) })
        }
        observedRecordings[current.project] = { active: !!current.active, stopped: !!current.stopped }
      }
    }
    let sent = 0
    for (const kind of KINDS) {
      if (!enabled[kind]) { state.enabled[kind] = false; continue }
      const matching = events.filter((event) => event && event.kind === kind && event.key)
      if (kind === 'ready' || kind === 'agent_waiting') {
        if (!facts.projectId || facts.complete?.[kind] !== true) continue
        const scope = `${kind}:${facts.projectId}`
        if (!observedProjects.has(scope)) {
          for (const event of matching) state.seen[event.key] = true
          state.enabled[kind] = true
          observedProjects.add(scope)
          continue
        }
      }
      if (!state.enabled[kind]) {
        for (const event of matching) state.seen[event.key] = true // baseline existing items on first enable
        state.enabled[kind] = true
        continue
      }
      for (const event of matching) if (!state.seen[event.key] && send(event)) sent += 1
    }
    persist()
    return { sent, reason: permissionReason }
  }
  return { available, disableOff, deliver }
}
