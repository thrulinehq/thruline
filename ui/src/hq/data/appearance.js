/* The app-wide look a person chose (C30 R-PREFS): density, motion and colour
 * vision, as three attributes on the app root. PURE: no React, no stylesheet.
 *
 *   data-density          comfortable | compact
 *   data-motion           system | reduce
 *   data-colour-vision    standard | proven_teal
 *
 * What follows each attribute:
 *  - density: the list screens (Inbox, Changes) draw their rows one way or the
 *    other from the same stored value (data/face.js `densityOf`); the attribute
 *    is what any other CSS hooks to;
 *  - motion: appearance.css stops the app's own animations and transitions when
 *    it is `reduce`, and when it is `system` and this Mac asks for less motion;
 *  - colour vision: appearance.css re-points the one Proven token (palette.js),
 *    so every place the app draws Proven changes together.
 *
 * Until the preferences are read, and when they cannot be, the attributes carry
 * the defaults. The design gallery never mounts the app, so it carries none.
 */
import { DEFAULTS, allowed, valueOf } from './preferences.js'
import { ATTRIBUTES } from './root.js'

export { ATTRIBUTES, rootOf, motionReduced } from './root.js'

/** The three values to draw, from the preferences store's state (a value the route would refuse is the default). */
export function appearanceOf(state) {
  const prefs = state && state.prefs
  const pick = (name) => {
    const v = valueOf(prefs, name)
    return allowed(name, v) ? v : DEFAULTS[name]
  }
  return { density: pick('density'), motion: pick('motion'), colour_vision: pick('colour_vision') }
}

/** Set the three attributes on `el` (the app root). */
export function applyAppearance(el, state) {
  if (!el || typeof el.setAttribute !== 'function') return
  const a = appearanceOf(state)
  for (const [name, attr] of Object.entries(ATTRIBUTES)) {
    if (el.getAttribute(attr) !== a[name]) el.setAttribute(attr, a[name])
  }
}
