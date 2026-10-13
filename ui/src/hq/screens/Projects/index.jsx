/* Projects on a live address (C30 R-NAV, `g p`): every project on this
 * machine, from `GET /api/hq/projects`, drawn with First run's folder rows.
 * A row opens that project's Inbox; "Add…" in the sidebar goes to First run.
 * Reads only: /api/hq/projects, /api/storage, /api/lanes, and the current
 * project's readiness for the one "your move" badge (data/your-move.js).
 * Nothing here writes. */
import React, { useMemo } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { withKeys } from '../../keys/registry.js'
import { useResource, useStorage } from '../../data/api.js'
import { useProjects } from '../../data/use-project.js'
import { LANES, currentProjectId, withFrame } from '../../data/project.js'
import { useYourMove } from '../../data/your-move.js'
import { projectsData } from './from-projects.js'
import { Quiet } from '../Inbox/index.jsx'

// the list keys (C30 R-KEYS): what this screen's logic returns as `keys` is what j k ↵ reach
const Live = withKeys(withFrame(Logic))

export default function Screen({ at, openCommand }) {
  const list = useProjects()
  const storage = useStorage()
  const lanes = useResource(LANES, { every: 30000 })
  const wanted = at && at.project
  const rows = list.data && Array.isArray(list.data.projects) ? list.data.projects : []
  const reading = useYourMove(rows.find((p) => p && p.id === currentProjectId(rows, wanted)) || null)
  const data = useMemo(() => (list.data ? projectsData({ projects: list.data, storage: storage.data, now: new Date(), current: wanted, reading }) : null), [list.data, storage.data, wanted, reading])
  if (list.error && !list.data) {
    return (
      <Quiet role="alert">
        ThruLine could not read which projects it has recorded. Check that <code style={{ font: "13px 'Geist Mono', monospace", color: '#EDEDEF' }}>thruline ui</code> is still running, then reload this page.
      </Quiet>
    )
  }
  if (!data) return <Quiet>Reading the record…</Quiet>
  return <PortedScreen Logic={Live} Template={Template} at={{ ...(at || {}), project: data.currentProject || wanted }} data={data} extra={{ openCommand, frame: { lanes: lanes.data } }} />
}
