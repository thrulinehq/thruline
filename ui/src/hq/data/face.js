/* How a screen reads and changes a stored preference (C30 R-PREFS).
 *
 * Every live screen is handed the preferences store's state as `props.prefs`
 * and its writer as `props.setPreference` (screen.jsx `PortedScreen`). A screen
 * asks THESE functions what the face (Plain / Technical) or the density is,
 * and asks them to change it, so there is one value for the whole app:
 *
 *  - live (`props.prefs` is there): the value is the stored one, and a change is
 *    a write the server must keep. Until the server has kept it nothing moves, and
 *    a refusal is said (prefs-notice.jsx, or the row on Settings), never hidden;
 *  - the design gallery hands a screen no props at all, so the screen keeps its
 *    own local value, exactly as the canvas drew it.
 *
 * PURE: no window, no React.
 */
import { allowed, valueOf } from './preferences.js'

/** The stored value of `name` for a screen, else the screen's own `local` value. */
export function storedOf(props, name, local) {
  const st = props && props.prefs
  const v = st && st.prefs ? valueOf(st.prefs, name) : undefined
  return allowed(name, v) ? v : local
}

/** Whether the screen was handed the store (it is live), so a change is a stored one. */
export const isLive = (props) => Boolean(props && props.prefs && typeof props.setPreference === 'function')

/** Ask for `value`: through the store when live, else the screen's own `setLocal`. */
export function choose(props, name, value, setLocal) {
  if (isLive(props)) {
    // a refusal is kept in the store's writeError and said where the person looks; it never throws here
    const done = props.setPreference(name, value)
    if (done && typeof done.catch === 'function') done.catch(() => false)
    return
  }
  if (typeof setLocal === 'function') setLocal(value)
}

export const FACES = Object.freeze(['plain', 'technical'])
const T1 = '#EDEDEF'
const T2 = '#A1A1AA'

/** The face a screen draws. `local` is the gallery's own ('plain' | 'technical'). */
export const faceOf = (props, local = 'plain') => {
  const f = storedOf(props, 'face', local)
  return FACES.includes(f) ? f : 'plain'
}

/** The face a click or `t` moves to. */
export const otherFace = (face) => (face === 'technical' ? 'plain' : 'technical')

/** Ask for a face. */
export const switchFace = (props, to, setLocal) => choose(props, 'face', to, setLocal)

/** The density a screen draws ('comfortable' | 'compact'). */
export const densityOf = (props, local = 'comfortable') => {
  const d = storedOf(props, 'density', local)
  return d === 'compact' ? 'compact' : 'comfortable'
}

/**
 * The top bar's Plain / Technical button for a LIVE screen: its click, its
 * accessible name, and the two colours (the face in use is bright). Only a
 * live screen has one; in the gallery the template draws the canvas's button.
 */
export function faceBar(props) {
  if (!isLive(props)) return undefined
  const face = faceOf(props)
  const tech = face === 'technical'
  return {
    face,
    tech,
    aria: tech ? 'Face: Technical. Switch to Plain' : 'Face: Plain. Switch to Technical',
    plain: tech ? T2 : T1,
    technical: tech ? T1 : undefined,
    toggle: () => switchFace(props, otherFace(face)),
  }
}
