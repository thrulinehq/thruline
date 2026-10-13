import React from 'react'
import { fill } from './hq-narrate.js'

/* A control that is off, with both halves of the reason beside it.
 *
 * `03-USER-LENS.md` rule 6: every disabled control says why, and what would
 * enable it. The reason is the button's accessible description, so it reaches a
 * screen reader as part of the control and not as loose text near it.
 *
 * Its own file since T-3-1, because two screens now need it and a component
 * copied into a second file is a rule that can be obeyed in one place and not
 * the other. `change-workspace.jsx` re-exports it, so every existing import and
 * every existing test keeps working unchanged.
 */
export function Blocked({ label, pair, values, primary, id }) {
  const reason = fill(pair.reason, values || {})
  const enabler = fill(pair.enabler, values || {})
  /* A3-F4: an off control with a blank half is the defect that shipped a dead
   * "Look at what broke" with nothing beside it. Outside a production build
   * (tests, the dev server) it cannot be drawn at all; a production build
   * still draws the control rather than blank the screen. */
  if ((!String(reason || '').trim() || !String(enabler || '').trim()) && notProduction()) {
    throw new Error(`Blocked "${label}": a disabled control needs a reason AND an enabler (rule 6)`)
  }
  return <div className="ws-change-blocked">
    <button type="button" disabled aria-describedby={id} className={primary ? 'ws-change-primary' : 'ws-change-secondary'}>{label}</button>
    {/* T-6-4: a `disabled` button cannot take focus, so `.` puts focus on
      * its reason instead — out of the Tab order, reachable by script only. */}
    <p className="ws-change-blocked-why" id={id} tabIndex={-1}>{reason} {enabler}</p>
  </div>
}

/* `process` is Node's in the tests and Vite's replacement in a build; a
 * browser with neither reads as production. */
function notProduction() {
  try {
    return typeof process !== 'undefined' && !!process.env && process.env.NODE_ENV !== 'production'
  } catch (_absent) {
    return false
  }
}

export default Blocked
