/* Every-Statement on a live address: the canvas's markup and logic, on one
 * Change of the real record.
 *
 * The Change is the ONE shared snapshot, loaded by the shared hook
 * (ui/src/hq/data/use-change.js → change.js). This screen's facts are selected
 * from it by from-change.js and nothing else.
 *
 * Reads through use-change.js, and the tracked Change's run sheet. "Run it"
 * opens the check sheet already running (`…/check?run=1`: the press is the
 * consent, C30 R-RUN), or is off with the server's own reason; while a check
 * of this Change runs, "Stop this check" reaches the cancel route through the
 * one run client (data/run.js → Repair-Loop/actions.js).
 */
import React, { useMemo } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { useChange } from '../../data/use-change.js'
import { useCheckRun, trackedOf, openCheckActions } from '../../data/run.js'
import { withRunSheet } from '../../data/change.js'
import { everyStatementFromChange } from './from-change.js'
import { useResource } from '../../data/api.js'
import { LANES, withFrame } from '../../data/project.js'
import { withKeys } from '../../keys/registry.js'
import { Reading, Failed } from '../../frame-state.jsx'

const Live = withKeys(withFrame(Logic))

export default function Screen({ at, openCommand }) {
  const change = useChange(at)
  const lanes = useResource(LANES, { every: 30000 })
  /* C30 R-RUN: the tracked Change's check through the one run client — followed
   * every second while it runs, folded into the snapshot */
  const tracked = trackedOf(change.data)
  const check = useCheckRun(tracked, change.data ? change.data.runSheet : null, { onEnd: change.reload })
  const snap = change.data && tracked ? withRunSheet(change.data, check.sheet, new Date()) : change.data
  const data = useMemo(() => (snap ? everyStatementFromChange(snap) : null), [snap])
  if (change.error && !change.data) return <Failed what={change.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!data) return <Reading />
  const actions = openCheckActions({ act: check.act, stop: check.stop, at: { change: at && at.change, project: (at && at.project) || projectOf(change.data) } })
  return <PortedScreen Logic={Live} Template={Template} at={at} project={projectOf(change.data)} data={data} actions={actions} extra={{ openCommand, frame: { lanes: lanes.data, change: change.data && change.data.change } }} />
}
