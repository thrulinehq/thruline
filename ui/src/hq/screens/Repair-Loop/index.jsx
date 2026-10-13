/* Repair-Loop on a live address: Check it again · Tell the agent, drawn from
 * where the Change's statement stops short of proof.
 *
 * Reads (live.js, data/run.js): the one Change snapshot (data/use-change.js),
 * and for a tracked Change its documents and run sheet — the sheet read again
 * every second while a check of this Change is going, so the elapsed time is
 * the record's and the end is drawn when the record has it (C30 P0-3).
 * Writes (actions.js, each a named constant beside its control):
 * POST …/run/permission and …/run (Check it again, with this launch's token;
 * the press is the consent), POST …/runs/<run>/cancel (Stop this check), and —
 * for work not tracked as a Change — POST /api/first-use/change {project,
 * thread} (Track this work as a Change, C26 b). And (C30 R-LEDGER, data/told-actions.js):
 * Copy message and Mark as pasted · undo keep that the person copied the
 * message or said they pasted it, POST …/threads/<t>/told-agent, free; the
 * ladder's first rung reads the project view's `told_agent`. Nothing is sent.
 */
import React, { useEffect, useRef, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { useLive, Reading, Said } from './live.js'
import { selectRepairLoop } from './from-change.js'
import { trackThenReread } from './actions.js'
import { recordTold, toldAnswerOf } from '../../data/told-actions.js'
import { trackOf } from './track.js'
import { useResource } from '../../data/api.js'
import { useCheckRun, takeRunIntent, arrivalStep } from '../../data/run.js'
import { withRunSheet } from '../../data/change.js'
import { LANES, withFrame } from '../../data/project.js'
import { withKeys } from '../../keys/registry.js'
import { href } from '../../routes.js'

const Live = withKeys(withFrame(Logic))

export default function Screen({ at, openCommand }) {
  const live = useLive(at)
  const lanes = useResource(LANES, { every: 30000 })
  const [track, setTrack] = useState({ busy: false, said: null, note: null })
  /* "Copy message" / "Mark as pasted": one write at a time, and what it said when it was refused */
  const [toldWrite, setToldWrite] = useState({ busy: false, said: null })
  const trackedId = live.tracked ? live.tracked.id : null
  /* the one run client (data/run.js): the sheet followed while a check runs, start and stop */
  const check = useCheckRun(trackedId, live.tracked ? live.tracked.runSheet : null, { onEnd: live.reload })
  const untracked = !!live.snapshot && !live.tracked && !live.trackedLoading
  /* "The click is the consent" (C30 R-RUN): arriving at `…/check?run=1` from a
   * press elsewhere starts the check once. The intent is taken out of the
   * address before anything starts, so a reload or Back never starts another. */
  const [intent, setIntent] = useState(false)
  const taken = useRef(false)
  useEffect(() => {
    if (taken.current) return
    taken.current = true
    if (takeRunIntent()) setIntent(true)
  }, [])
  const step = intent ? arrivalStep({ snapshot: live.snapshot, trackedLoading: live.trackedLoading, act: check.act, sheet: check.sheet }) : 'none'
  useEffect(() => {
    if (!intent || step === 'wait' || step === 'none') return
    setIntent(false)
    if (step === 'start') check.start()
    else if (step === 'follow') check.follow()
  }, [intent, step]) // eslint-disable-line react-hooks/exhaustive-deps
  if (!at || !at.change) return <Said what="No Change is open. Pick one from the Inbox." />
  if (live.error && !live.snapshot) return <Said what={live.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!live.snapshot) return <Reading />
  const now = new Date()
  const tracked = live.tracked ? { ...live.tracked, runSheet: check.sheet } : null
  /* the snapshot with the followed sheet folded in, so this sheet and the Brief read one result */
  const snapshot = tracked ? withRunSheet(live.snapshot, check.sheet, now) : live.snapshot
  const data = selectRepairLoop(snapshot, tracked, { now })
  if (!data || !data.statement) return <Said what="Nothing the agent said in this work claims it is finished, so there is nothing to check again or tell the agent yet." />
  data.track = trackOf({ untracked, busy: track.busy, said: track.said, note: track.note, canStart: data.act.canStart, tracked: !!live.tracked })
  /* arriving with the intent, the press is being answered: the form waits, it does not offer a second press */
  data.act.busy = check.act.busy || step === 'wait' || step === 'start'
  data.act.refusal = check.act.refusal
  data.act.told = toldWrite
  /* the thread on screen, as the first-use route takes it */
  const where = { project: live.snapshot.project && live.snapshot.project.id, thread: live.snapshot.change && live.snapshot.change.threadId }
  /* C30 R-LEDGER (write address 24): keep that the person copied the message, said they pasted it, or
   * took that back; then re-read, so the ladder's first rung is the record's. Nothing is sent. A copy
   * has already reached the clipboard when the write is refused, and the line says so. */
  const toldAct = async (act, { message, refersTo } = {}) => {
    if (toldWrite.busy) return
    setToldWrite({ busy: true, said: null })
    const answer = toldAnswerOf(await recordTold({ project: where.project, thread: where.thread, act, message, refersTo }))
    if (answer.kind === 'kept') {
      setToldWrite({ busy: false, said: null })
      live.reload()
    } else {
      setToldWrite({ busy: false, said: (act === 'copied' ? 'Copied to your clipboard, but ThruLine could not keep a record of it. ' : '') + answer.detail })
    }
  }
  const actions = {
    toldAct,
    checkAgain: check.start,
    stopCheck: check.stop,
    openApprove: () => { if (typeof window !== 'undefined') window.location.hash = href('approve', at) },
    track: () => {
      if (track.busy) return
      setTrack((t) => ({ ...t, busy: true, said: null }))
      trackThenReread(where, {
        reread: live.reload,
        open: (id) => { if (typeof window !== 'undefined') window.location.hash = href('check', { change: id, project: at.project }) },
      }).then((r) => setTrack({ busy: false, said: r.ok ? null : r.said, note: r.ok ? r.note : null }))
    },
  }
  return <PortedScreen Logic={Live} Template={Template} at={at} project={projectOf(live.snapshot)} data={data} actions={actions} extra={{ openCommand, frame: { lanes: lanes.data, change: live.snapshot.change } }} />
}
