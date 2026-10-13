/* Approve-Sheet on a live address: the approval ceremony for the version in
 * front of the person.
 *
 * Reads (../Repair-Loop/live.js): the one Change snapshot, the tracked Change's
 * documents and model, and /api/licence (Free or Pro only); and the project's
 * decisions (use-decisions.js, C29), which refuse Approve while a blocking one
 * is broken or unproven on exactly this version. Writes
 * (actions.js, each a named constant beside its control): POST …/decisions
 * (Approve, with the sheet's note when one was written, kept with the approval
 * by the same act) and POST …/decisions/withdraw (Withdraw approval). Without a
 * tracked Change, a ready version or a Pro licence, Approve renders off with
 * its reason. Every answer the server gives an act is drawn: recorded, the
 * code moved (with its files), the licence refusal, or any other refusal.
 */
import React from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { useLive, Reading, Said } from '../Repair-Loop/live.js'
import { selectApproveSheet } from './from-change.js'
import { approveVersion, withdrawApproval } from './actions.js'
import { useResource } from '../../data/api.js'
import { LANES, withFrame } from '../../data/project.js'
import { withKeys } from '../../keys/registry.js'
import { useDecisions } from '../../data/use-decisions.js'

const Live = withKeys(withFrame(Logic))

export default function Screen({ at, openCommand }) {
  const live = useLive(at, { licence: true })
  const lanes = useResource(LANES, { every: 30000 })
  /* C29: the project's decisions; a blocking one broken or unproven on this version refuses Approve */
  const decisions = useDecisions(projectOf(live.snapshot) || (at && at.project) || null)
  if (!at || !at.change) return <Said what="No Change is open. Pick one from the Inbox." />
  if (live.error && !live.snapshot) return <Said what={live.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!live.snapshot) return <Reading />
  const data = selectApproveSheet(live.snapshot, live.tracked, { licence: live.licence })
  /* Each act resolves to the server's answer, which the sheet reads and draws
   * (logic.js, `answerOf`); the sheet re-reads the record itself, and only
   * after an act was recorded and drawn (C30 P0-2). */
  const actions = {
    approve: (actor, note) => approveVersion({ changeId: data.act.changeId, seen: data.act.seen, actor, note }),
    withdraw: (actor, approvedFull) => withdrawApproval({ changeId: data.act.changeId, approvedFull: approvedFull || data.act.approvedFull, actor }),
    reload: live.reload,
  }
  return <PortedScreen Logic={Live} Template={Template} at={at} project={projectOf(live.snapshot)} data={data} actions={actions} extra={{ openCommand, frame: { lanes: lanes.data, change: live.snapshot.change }, decisions }} />
}
