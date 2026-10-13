/* Reduced motion, asked once in one place (T-6-4; design record §4).
 *
 * PURE apart from the one question it asks of a window. The stylesheet already
 * collapses every animation and transition under `prefers-reduced-motion:
 * reduce` (`workspace.css`); this is for the parts JavaScript draws, so they
 * render their FINISHED state rather than a state a stylesheet then has to
 * undo: the lit thread's lines are drawn whole with no drawing mask, and a
 * card carries no transition at all.
 *
 * No window, no `matchMedia`, or a `matchMedia` that throws all read as "no
 * preference": the stylesheet's own reduce block still holds in a browser.
 */

export const REDUCED_MOTION_QUERY = '(prefers-reduced-motion: reduce)'

/**
 * Whether this window asks for reduced motion.
 *
 * @param {Window|any} [win] defaults to the global window
 * @returns {boolean}
 */
export function prefersReducedMotion(win) {
  try {
    const w = win !== undefined ? win : typeof window === 'undefined' ? null : window
    return !!(w && typeof w.matchMedia === 'function' && w.matchMedia(REDUCED_MOTION_QUERY).matches)
  } catch (_blocked) {
    return false
  }
}

export default prefersReducedMotion
