/* The app root and the attributes on it (C30 R-PREFS). No imports: the
 * document-level click handler (in-page.js) reads this without the
 * preferences store or React. */

export const ATTRIBUTES = Object.freeze({
  density: 'data-density',
  motion: 'data-motion',
  colour_vision: 'data-colour-vision',
})

/** The app root: the element React mounts into, else the document element. */
export const rootOf = (doc) => (doc && (doc.getElementById('root') || doc.documentElement)) || null

/** Whether the app's own motion is to be cut: "Always reduce", or "Follow this Mac" on a Mac that asks for less. */
export function motionReduced(doc) {
  const root = rootOf(doc)
  if (root && root.getAttribute && root.getAttribute(ATTRIBUTES.motion) === 'reduce') return true
  const view = doc && doc.defaultView
  return !!(view && typeof view.matchMedia === 'function' && view.matchMedia('(prefers-reduced-motion: reduce)').matches)
}
