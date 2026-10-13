/* The look a person chose, applied to the whole app (C30 R-PREFS): density,
 * motion and colour vision become three attributes on the app root
 * (data/appearance.js), and appearance.css follows them. Mounted once, by
 * app.jsx; the design gallery never mounts it, so it draws the canvas.
 *
 * Until the preferences are read, and when they cannot be, the attributes carry
 * the defaults. A choice the server did not keep changes nothing here: the
 * attributes follow what is STORED (data/preferences.js). */
import { useLayoutEffect } from 'react'
import { usePreferences } from './data/preferences.js'
import { applyAppearance, appearanceOf, rootOf } from './data/appearance.js'
import './appearance.css'

export function useAppearance(store) {
  const state = usePreferences(store)
  const a = appearanceOf(state)
  useLayoutEffect(() => {
    applyAppearance(rootOf(document), state)
  }, [a.density, a.motion, a.colour_vision]) // eslint-disable-line react-hooks/exhaustive-deps
  return a
}
