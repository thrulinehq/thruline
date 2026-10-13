/* One ported artboard on a live address: the design's markup and logic,
 * given the real data (`data`) and the real actions (`actions`) for it, with
 * the canvas's prototype links pointed at the live screens.
 *
 * `project` is the project the open Change belongs to, when the screen read
 * it: links then name that project even when the address did not (the
 * project crumb goes to that project's Inbox, C30 R-LINKS).
 *
 * Every live screen is handed the person's stored preferences (C30 R-PREFS):
 * `prefs`, the one store's state, and `setPreference(name, value)`, the one
 * writer. Face and density are read and changed through them (data/face.js);
 * the design gallery runs `DcScreen` directly and hands a screen neither. */
import React from 'react'
import { DcScreen } from './dc/runtime.jsx'
import { linker } from './routes.js'
import { preferences, usePreferences } from './data/preferences.js'

/** The project id of a Change snapshot (data/change.js `project.id`), or undefined. */
export const projectOf = (snap) => (snap && snap.project && snap.project.id ? String(snap.project.id) : undefined)

export function linkAt(at, project) {
  const where = at || {}
  return !where.project && project ? { ...where, project } : where
}

export function PortedScreen({ Logic, Template, at, data, actions, extra, project, store = preferences }) {
  const prefs = usePreferences(store)
  const setPreference = (name, value) => store.set(name, value)
  return <DcScreen Logic={Logic} Template={Template} props={{ prefs, setPreference, data, actions, at, ...extra }} link={linker(linkAt(at, project))} />
}
