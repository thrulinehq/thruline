/* Record over time on a live address. Reads /api/hq/projects, /api/storage,
 * /api/lanes (the recording pill) and, per project,
 * /api/hq/projects/<id>/statements — one row per claim with the engine's own
 * verdict, and every session's span — from which the tallies and the daily
 * series are counted (from-projects.js `recordFromStatements`), and, beside
 * the verdict, the three facts the counts that need more are made of
 * (metrics.js). When that read fails the page says the statements could not be
 * read. The current project's readiness is read for the one "your move" badge
 * (data/your-move.js). Export CSV hands a file to the browser; nothing on this
 * screen writes to the server or the store.
 */
import React, { useEffect, useMemo, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { API, readJson, useResource, useStorage } from '../../data/api.js'
import { LANES, currentProjectId, withFrame } from '../../data/project.js'
import { useYourMove } from '../../data/your-move.js'
import { useCapture } from '../../data/capture.js'
import { recordFromProjects, recordFromStatements } from './from-projects.js'
import { Reading, Failed } from './state.jsx'

export const STATEMENTS = (id) => `/api/hq/projects/${encodeURIComponent(id)}/statements`
export const READS = Object.freeze({ projects: API.projects, storage: API.storage, statements: STATEMENTS })

const Live = withFrame(Logic)

/** Every project's statements document, or `{failed}` when any read fails. */
function useStatements(projects) {
  const ids = (projects || []).map((p) => p.id).join(',')
  const [state, setState] = useState({ ids: null, docs: null, failed: false })
  useEffect(() => {
    if (!projects) return undefined
    let live = true
    Promise.all(projects.map((p) => readJson(STATEMENTS(p.id)).then((d) => [p.id, d]))).then(
      (pairs) => live && setState({ ids, docs: Object.fromEntries(pairs), failed: false }),
      () => live && setState({ ids, docs: null, failed: true }),
    )
    return () => { live = false }
  }, [ids]) // eslint-disable-line react-hooks/exhaustive-deps
  return state.ids === ids ? state : { ids, docs: null, failed: false, loading: true }
}

export default function Screen({ at, openCommand }) {
  const projects = useResource(READS.projects)
  const storage = useStorage() // the shared reading (api.js): READS.storage, at most a minute old
  const lanes = useResource(LANES, { every: 30000 })
  const list = projects.data ? projects.data.projects || [] : null
  const capture = useCapture(currentProjectId(list, at && at.project))
  const st = useStatements(list)
  // the sidebar badge's one count (C46 §1.1): the current project's readiness, shared with every screen
  const reading = useYourMove((list || []).find((p) => p && p.id === currentProjectId(list, at && at.project)) || null)
  const data = useMemo(() => {
    if (!list) return null
    if (st.docs) return recordFromStatements({ projects: list, statements: st.docs, storage: storage.data, now: Date.now(), current: at && at.project, reading })
    if (st.failed) return recordFromProjects({ projects: list, storage: storage.data, now: Date.now(), current: at && at.project, reading })
    return null
  }, [list, st.docs, st.failed, storage.data, at && at.project, reading]) // eslint-disable-line react-hooks/exhaustive-deps
  if (projects.error && !projects.data) return <Failed />
  if (!data) return <Reading />
  return <PortedScreen Logic={Live} Template={Template} at={at} data={data} extra={{ openCommand, frame: { lanes: lanes.data }, captureHealth: capture.facts, captureError: capture.error, captureReading: capture.reading, retryCapture: capture.reload }} />
}
