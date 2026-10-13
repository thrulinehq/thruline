/* A person's preferences (C30 R-PREFS), read and written through
 * `GET` / `POST /api/preferences` (docs/hq-next/API.md). They live in
 * preferences.json beside the ledger, never in the record.
 *
 * One shared store, so every screen and the key dispatcher see one value.
 * Every key the route serves is read and kept here, and every one can be
 * written: `set('face', 'technical')` for a key, `set('notifications.moved',
 * false)` for one notification switch, `setMany({...})` for a subset in one
 * request. Face, density, motion and colour vision are applied app-wide
 * (appearance.js, face.js); keys is read live by the key dispatcher.
 *
 *  - Until the read answers, and when it fails, every key is its default and
 *    `status` says which ('reading' / 'failed'): a default is never said to
 *    be the person's choice. `fromFile` names the keys the file held.
 *  - A write changes nothing here until the server has kept it. A refused or
 *    failed write leaves the value as it was and keeps why, in plain words, in
 *    `writeError[key]`, for the control that asked to say it.
 *
 * PURE apart from the `fetch` it is handed, so a test drives it without a server.
 */
import { useEffect, useSyncExternalStore } from 'react'

/** The one address, read and written (CLAUDE.md write-address table, row 15). */
export const PREFERENCES = '/api/preferences'

/** Every key and its default, as the route serves them (API.md "The keys"). */
export const DEFAULTS = Object.freeze({
  face: 'plain',
  density: 'comfortable',
  motion: 'system',
  colour_vision: 'standard',
  keys: 'on',
  masked_copy: true,
  notifications: Object.freeze({ ready: false, not_backed: false, agent_waiting: false, recording_stopped: false }),
  weekly_summary: false,
})

/** The values each key may take (the route refuses anything else). */
export const ALLOWED = Object.freeze({
  face: ['plain', 'technical'],
  density: ['comfortable', 'compact'],
  motion: ['system', 'reduce'],
  colour_vision: ['standard', 'proven_teal'],
  keys: ['on', 'mod', 'off'],
  masked_copy: [true, false],
  weekly_summary: [true, false],
})

/** The four C51 events. Browser permission is requested before an On choice is kept. */
export const SWITCHES = Object.freeze(['ready', 'not_backed', 'agent_waiting', 'recording_stopped'])

/** Every name `set` takes: a flat key, or `notifications.<switch>`. */
export const NAMES = Object.freeze([...Object.keys(ALLOWED), ...SWITCHES.map((x) => 'notifications.' + x)])

const isSwitch = (name) => typeof name === 'string' && name.startsWith('notifications.') && SWITCHES.includes(name.slice(14))

/** Whether `value` is one the route takes for `name`. */
export function allowed(name, value) {
  if (isSwitch(name)) return typeof value === 'boolean'
  return Object.prototype.hasOwnProperty.call(ALLOWED, name) && ALLOWED[name].includes(value)
}

/** The stored value of a name (`face`, `notifications.moved`) in a preferences object. */
export function valueOf(prefs, name) {
  if (!prefs) return undefined
  return isSwitch(name) ? (prefs.notifications || {})[name.slice(14)] : prefs[name]
}

/* the server's refusals, in plain words (API.md, POST) */
const REFUSED = {
  preferences_file_newer: 'a newer ThruLine wrote your preferences file, so this one leaves it as it is',
  preferences_file_not_regular: 'your preferences file is not an ordinary file, so ThruLine writes nothing through it',
  write_failed: 'the preferences file could not be written',
  invalid_value: 'this ThruLine does not know that choice',
  unknown_preference: 'this ThruLine does not know that choice',
  same_origin_required: 'ThruLine takes this only from its own page',
  request_too_large: 'the choice was too large to keep',
  malformed_request: 'the choice did not reach ThruLine whole',
  no_preferences: 'no choice reached ThruLine',
}

/** Why the file was not used, in plain words; null when it was read or is absent. */
export function fileWords(file) {
  if (file === 'newer_schema') return 'Your preferences file was written by a newer ThruLine, so this one uses the defaults and changes nothing in it.'
  if (file === 'not_a_regular_file') return 'Your preferences file is not an ordinary file, so ThruLine uses the defaults and does not follow it.'
  if (file === 'not_json' || file === 'unrecognised') return 'Your preferences file could not be read as preferences, so the defaults are in use. Your next choice here replaces it.'
  if (file === 'unreadable') return 'Your preferences file could not be read, so the defaults are in use.'
  return null
}

/* every key the route serves, each value checked against what it may be; anything else is its default */
function usable(doc) {
  const p = doc && typeof doc === 'object' ? doc.preferences : null
  if (!p || typeof p !== 'object') return null
  const out = {}
  for (const k of Object.keys(DEFAULTS)) {
    if (k === 'notifications') {
      const n = p.notifications && typeof p.notifications === 'object' ? p.notifications : {}
      out.notifications = Object.fromEntries(Object.entries(DEFAULTS.notifications).map(([s, d]) => [s, typeof n[s] === 'boolean' ? n[s] : d]))
    } else out[k] = ALLOWED[k].includes(p[k]) ? p[k] : DEFAULTS[k]
  }
  return out
}

const START = Object.freeze({ status: 'idle', prefs: DEFAULTS, fromFile: [], file: null, readError: null, pending: {}, writeError: {} })

/**
 * @param {{fetchImpl?: typeof fetch}} seams
 * @returns {{get, subscribe, load, set, setMany, dismiss}}
 */
export function createPreferences({ fetchImpl = (...a) => fetch(...a) } = {}) {
  let state = START
  let loading = null
  const subs = new Set()
  const emit = (next) => {
    state = Object.freeze(next)
    for (const fn of subs) fn()
  }
  const take = (doc) => ({ prefs: usable(doc), fromFile: Array.isArray(doc.from_file) ? doc.from_file : [], file: typeof doc.file === 'string' ? doc.file : null })

  async function load() {
    if (loading) return loading
    emit(Object.assign({}, state, { status: 'reading' }))
    loading = (async () => {
      try {
        const res = await fetchImpl(PREFERENCES, { headers: { accept: 'application/json' } })
        if (!res.ok) throw new Error('answered ' + res.status)
        const got = take(await res.json())
        if (!got.prefs) throw new Error('no preferences in the answer')
        emit(Object.assign({}, state, got, { status: 'read', readError: null }))
      } catch (e) {
        emit(Object.assign({}, state, { status: 'failed', prefs: DEFAULTS, fromFile: [], file: null, readError: (e && e.message) || String(e) }))
      } finally {
        loading = null
      }
    })()
    return loading
  }

  /*
   * Ask the server to keep `names` (flat keys or `notifications.<switch>`) as
   * `values`, in ONE request. Nothing here changes until the server's answer
   * shows every one kept; a refusal leaves each value as it was and keeps why in
   * `writeError[name]`. Resolves true when kept, false when not.
   */
  async function write(names, values) {
    if (names.some((n) => n in state.pending)) return false
    const clear = (o) => { const n = Object.assign({}, o); for (const k of names) delete n[k]; return n }
    const body = {}
    names.forEach((n, i) => {
      if (isSwitch(n)) body.notifications = Object.assign(body.notifications || {}, { [n.slice(14)]: values[i] })
      else body[n] = values[i]
    })
    emit(Object.assign({}, state, { pending: Object.assign({}, state.pending, ...names.map((n, i) => ({ [n]: values[i] }))), writeError: clear(state.writeError) }))
    let res
    try {
      res = await fetchImpl(PREFERENCES, { method: 'POST', headers: { 'Content-Type': 'application/json', accept: 'application/json' }, body: JSON.stringify(body) })
    } catch {
      return refuse(names, values, 'ThruLine did not answer')
    }
    let doc = null
    try {
      doc = await res.json()
    } catch {
      doc = null
    }
    if (!res.ok) return refuse(names, values, (doc && REFUSED[doc.error]) || 'ThruLine answered ' + res.status)
    const got = doc ? take(doc) : null
    if (!got || !got.prefs || names.some((n, i) => valueOf(got.prefs, n) !== values[i])) return refuse(names, values, 'ThruLine’s answer did not show the choice kept')
    emit(Object.assign({}, state, got, { status: 'read', readError: null, pending: clear(state.pending) }))
    return true
  }

  /** Ask the server to keep one choice: `set('face', 'technical')`, `set('notifications.moved', false)`. */
  async function set(name, value) {
    if (!allowed(name, value)) throw new Error('Not a choice ThruLine offers: ' + name)
    return write([name], [value])
  }

  /** Ask the server to keep a subset in one request: `{face: 'technical', notifications: {moved: false}}`. */
  async function setMany(patch) {
    const names = []
    const values = []
    const add = (name, value) => {
      if (!allowed(name, value)) throw new Error('Not a choice ThruLine offers: ' + name)
      names.push(name)
      values.push(value)
    }
    for (const [k, v] of Object.entries(patch && typeof patch === 'object' ? patch : {})) {
      if (k === 'notifications' && v && typeof v === 'object') for (const [sw, x] of Object.entries(v)) add('notifications.' + sw, x)
      else add(k, v)
    }
    if (!names.length) throw new Error('No choice to keep')
    return write(names, values)
  }

  function refuse(names, values, why) {
    const pending = Object.assign({}, state.pending)
    for (const n of names) delete pending[n]
    const errs = Object.assign({}, state.writeError)
    names.forEach((n, i) => { errs[n] = { value: values[i], why } })
    emit(Object.assign({}, state, { pending, writeError: errs }))
    return false
  }

  /** Stop saying that a write was refused (the person has read it). */
  function dismiss(name) {
    if (!(name in state.writeError)) return
    const errs = Object.assign({}, state.writeError)
    delete errs[name]
    emit(Object.assign({}, state, { writeError: errs }))
  }

  return {
    get: () => state,
    subscribe(fn) {
      subs.add(fn)
      return () => subs.delete(fn)
    },
    load,
    set,
    setMany,
    dismiss,
  }
}

/** The app's one store. */
export const preferences = createPreferences()

/** The store's state, read once on first use and followed after. */
export function usePreferences(store = preferences) {
  const s = useSyncExternalStore(store.subscribe, store.get, store.get)
  useEffect(() => {
    if (store.get().status === 'idle') store.load()
  }, [store])
  return s
}
