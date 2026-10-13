/* The app's keys (C30 R-KEYS): one keydown listener on the window, the `?`
 * map, and Find, ask, act's ⌘K, all decided by dispatch.js from the one key
 * table. Mounted once, by app.jsx; the design gallery never mounts it.
 *
 * The mode is the person's Settings › Keyboard choice, read from
 * `/api/preferences` (data/preferences.js): the default (On) until the read
 * answers and when it fails, and followed live (no remount) after.
 *
 * `t` is the one key no screen handles: it switches the stored face (Plain /
 * Technical) for the whole app (C30 R-PREFS, data/face.js). A write the server
 * refuses is said by prefs-notice.jsx, never pretended. */
import React, { useEffect, useRef, useState } from 'react'
import { SCREENS } from '../routes.js'
import { usePreferences, preferences } from '../data/preferences.js'
import { otherFace } from '../data/face.js'
import { createDispatcher } from './dispatch.js'
import { currentKeys } from './registry.js'
import { KeysMap } from './overlay.jsx'
import { MODES } from './rows.js'

/** Go to an address the way a click on its link does. */
export function go(to, win = window) {
  if (typeof to !== 'string' || !to) return
  if (to.startsWith('#')) win.location.hash = to
  else win.location.assign(to)
}

/** After j/k: the selected list row in view (the browser scrolls a focused row by itself). */
function reveal(win = window) {
  const later = typeof win.requestAnimationFrame === 'function' ? win.requestAnimationFrame.bind(win) : (fn) => setTimeout(fn, 0)
  later(() => {
    const row = win.document.querySelector('button.tl-row[aria-current="true"]')
    if (row && typeof row.scrollIntoView === 'function') row.scrollIntoView({ block: 'nearest' })
  })
}

/**
 * Carry out what the dispatcher decided, for one keydown. A screen handler
 * that answers `false` had nothing to act on: the key is left to the page.
 */
export function perform(out, e, { setPalette, setMap, handlers, toggleFace, win = window }) {
  switch (out.do) {
    case 'palette':
      e.preventDefault()
      setMap(false)
      setPalette((o) => !o)
      return
    case 'find':
      e.preventDefault()
      setPalette('ask')
      return
    case 'close-palette':
      setPalette(false)
      return
    case 'map':
      e.preventDefault()
      setMap(true)
      return
    case 'close-map':
      e.preventDefault()
      setMap(false)
      return
    case 'wait':
      e.preventDefault()
      return
    case 'go':
      e.preventDefault()
      go(out.href, win)
      return
    case 'face':
      e.preventDefault()
      if (typeof toggleFace === 'function') toggleFace()
      return
    case 'call': {
      const r = handlers[out.act]()
      if (r === false) return
      e.preventDefault()
      if (typeof r === 'string') go(r, win)
      if (out.act === 'next' || out.act === 'prev') reveal(win)
      return
    }
    default:
  }
}

/** `now` and `store` are seams for tests: the clock `g` is timed by, and the preferences store. */
export function useKeys({ at, palette, setPalette, now, store }) {
  const prefs = usePreferences(store)
  const kept = store || preferences
  const mode = MODES.includes(prefs.prefs.keys) ? prefs.prefs.keys : 'on'
  const [map, setMap] = useState(false)
  const live = useRef(null)
  // `t`: the stored face, the other way (a refusal is said by prefs-notice.jsx)
  const toggleFace = () => { kept.set('face', otherFace(kept.get().prefs.face)).catch(() => false) }
  live.current = { at, palette, map, mode, setPalette, toggleFace }
  useEffect(() => {
    const d = createDispatcher(now ? { now } : {})
    const on = (e) => {
      const c = live.current
      const handlers = currentKeys()
      const out = d.decide(e, { mode: c.mode, screen: SCREENS[c.at.screen], at: c.at, palette: c.palette, overlay: c.map, target: e.target, handlers })
      perform(out, e, { setPalette: c.setPalette, setMap, handlers, toggleFace: c.toggleFace })
    }
    window.addEventListener('keydown', on)
    return () => window.removeEventListener('keydown', on)
  }, []) // eslint-disable-line react-hooks/exhaustive-deps
  // a new address closes the map
  useEffect(() => setMap(false), [at.screen, at.change, at.project, at.pane])
  const screen = SCREENS[at.screen]
  return { mode, map: map ? <KeysMap screen={screen} at={at} mode={mode} onClose={() => setMap(false)} /> : null }
}
