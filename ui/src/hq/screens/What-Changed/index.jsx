/* What-Changed on a live address: the canvas's markup and logic, on one
 * Change of the real record — every edit, every line.
 *
 * The Change is the ONE shared snapshot, loaded by the shared hook
 * (ui/src/hq/data/use-change.js → change.js). This screen's facts are selected
 * from it by from-change.js and nothing else, except: INTERIM, the snapshot
 * does not yet carry each edit's hunks, so this file reads the same thread
 * (named by the snapshot itself) and joins the served hunks onto the
 * snapshot's edits by id (`withServedHunks`). Remove that read once change.js
 * carries `edits[].hunks` and `edits[].source`.
 *
 * "Check it again" opens the check sheet already running (`…/check?run=1`:
 * the press is the consent, C30 R-RUN), or is off with the server's own
 * reason; while a check of this Change runs, "Stop this check" reaches the
 * cancel route through the one run client (data/run.js →
 * Repair-Loop/actions.js). Nothing else here writes.
 */
import React, { useMemo } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { API, useResource } from '../../data/api.js'
import { useChange } from '../../data/use-change.js'
import { useCheckRun, trackedOf, openCheckActions } from '../../data/run.js'
import { withRunSheet } from '../../data/change.js'
import { whatChangedFromChange, withServedHunks } from './from-change.js'
import { LANES, withFrame } from '../../data/project.js'
import { Reading, Failed } from '../../frame-state.jsx'
import { withKeys } from '../../keys/registry.js'
import { href } from '../../routes.js'

const Live = withKeys(withFrame(Logic))

export default function Screen({ at, openCommand }) {
  const change = useChange(at)
  const lanes = useResource(LANES, { every: 30000 })
  /* C30 R-RUN: the tracked Change's check through the one run client — followed
   * every second while it runs, folded into the snapshot */
  const tracked = trackedOf(change.data)
  const check = useCheckRun(tracked, change.data ? change.data.runSheet : null, { onEnd: change.reload })
  const snap = change.data && tracked ? withRunSheet(change.data, check.sheet, new Date()) : change.data
  const actions = {
    ...openCheckActions({ act: check.act, stop: check.stop, at: { change: at && at.change, project: (at && at.project) || projectOf(change.data) } }),
    openTell: () => { if (typeof window !== 'undefined') window.location.hash = href('check', at) },
    openApprove: () => { if (typeof window !== 'undefined') window.location.hash = href('approve', at) },
  }
  const hasLines = !!snap && (snap.edits || []).every((e) => Array.isArray(e.hunks))
  const thread = useResource(snap && !hasLines && snap.project && snap.project.id && snap.change && snap.change.threadId
    ? API.thread(snap.project.id, snap.change.threadId) : null)
  const data = useMemo(() => {
    if (!snap) return null
    const d = whatChangedFromChange(hasLines ? snap : withServedHunks(snap, thread.data))
    if (d) d.change.followingLive = true // the shared hook re-reads every 20 s
    return d
  }, [snap, thread.data, hasLines])
  if (change.error && !snap) return <Failed what={change.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!data || (!hasLines && thread.loading && !thread.data)) return <Reading />
  return <PortedScreen Logic={Live} Template={Template} at={at} project={projectOf(snap)} data={data} actions={actions} extra={{ openCommand, frame: { lanes: lanes.data, change: snap && snap.change } }} />
}
