/* Team on a live address: the canvas's markup and logic, on what this
 * machine's store holds (W2-E). ThruLine v0.2 has no team model, so seats,
 * groups, approval rules and shared templates are drawn with their reason in
 * plain words; approvals, lines set aside, the plan and storage are the
 * record's own (`from-store.js`).
 *
 * Reads: /api/hq/projects, /api/changes, /api/storage, /api/licence, and the
 * current project's readiness for the one "your move" badge (data/your-move.js).
 * Writes (C30 R-LEDGER, actions.js): the two second-approval addresses,
 * POST /api/changes/<id>/seconds and …/seconds/withdraw, free, reached only by
 * the control in an approved version's row (seconds.jsx). No other control on
 * this screen writes anything.
 */
import React, { useMemo } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { API, useResource, useStorage } from '../../data/api.js'
import { teamFromStore } from './from-store.js'
import { secondVersion, withdrawSecond } from './actions.js'
import { Reading, Failed } from './state.jsx'
import { LANES, currentProjectId, withFrame } from '../../data/project.js'
import { useYourMove } from '../../data/your-move.js'

const Live = withFrame(Logic)

export const READS = Object.freeze({
  projects: API.projects,
  changes: API.changes(),
  storage: API.storage,
  licence: '/api/licence',
})

export default function Screen({ at, openCommand }) {
  const projects = useResource(READS.projects)
  const changes = useResource(READS.changes)
  const storage = useStorage() // the shared reading (api.js): READS.storage, at most a minute old
  const licence = useResource(READS.licence)
  const lanes = useResource(LANES, { every: 30000 })
  const list = projects.data && Array.isArray(projects.data.projects) ? projects.data.projects : []
  // the current project, as every screen reads it: `?project=`, else the most recently active
  const project = list.find((p) => p.id === currentProjectId(list, at && at.project)) || null
  // the sidebar badge's one count (C46 §1.1): the current project's readiness, shared with every screen
  const reading = useYourMove(project)
  const data = useMemo(() => (projects.data
    ? teamFromStore({
      project, projects: list,
      changes: changes.data && Array.isArray(changes.data.changes) ? changes.data.changes : [],
      storage: storage.data, licence: licence.data, now: Date.now(), reading,
    })
    : null), [projects.data, changes.data, storage.data, licence.data, project && project.id, reading])

  if (projects.error && !projects.data) return <Failed />
  // the record being re-read after a second was put (changes.reload) keeps the page up: only a first read shows "Reading"
  if (!data || (changes.loading && !changes.data)) return <Reading />
  if (changes.error) return <Failed what="ThruLine could not read the approvals on this machine. Reload this page; if it keeps happening, run thruline verify." />
  // C30 R-LEDGER: the two writes this screen reaches (actions.js), handed to the control in each approved version's row
  const actions = { second: secondVersion, withdraw: withdrawSecond, reload: changes.reload }
  return <PortedScreen Logic={Live} Template={Template} at={at} data={data} actions={actions} extra={{ openCommand, frame: { lanes: lanes.data } }} />
}
