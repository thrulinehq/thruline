/* Change-Brief on a live address: the canvas's markup and logic, on one Change
 * of the real record.
 *
 * The Change is the ONE shared snapshot (ui/src/hq/data/change.js), read by
 * use-change.js for the address (an HQ thread id, or a tracked Change id). The
 * Brief draws the snapshot as it is: the Brief's logic reads DEMO's shape, and
 * the snapshot is that shape.
 *
 * Reads only (use-change.js): /api/hq/projects[/<p>[/threads/<t>]],
 * /api/hq/sessions/<s>, /api/changes[/<id>[/…]] through readChangeDocuments,
 * /api/storage; and the project's decisions (use-decisions.js, C29), and the
 * tracked Change's run sheet (GET /api/changes/<id>/run).
 *
 * Writes (C30 R-RUN), only through the one run client (data/run.js
 * `useCheckRun` → Repair-Loop/actions.js): "Check it again" starts the
 * Change's check in place when the server would start it (POST
 * …/run/permission the first time in a project, then POST …/run); "Stop this
 * check" is POST …/runs/<run>/cancel. Untracked work opens the check sheet.
 * And (C30 R-LEDGER, notes-actions.js): the ⋯ menu's "Add a note" keeps one
 * note on the tracked Change, POST …/notes, free. (objection-actions.js): the
 * ⋯ menu's "This is wrong…" keeps one objection beside the selected statement,
 * POST /api/hq/projects/<id>/objections, free, and changes no verdict.
 */
import React, { useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { useChange } from '../../data/use-change.js'
import { useResource } from '../../data/api.js'
import { LANES, withFrame } from '../../data/project.js'
import { withKeys } from '../../keys/registry.js'
import { href } from '../../routes.js'
import { useDecisions } from '../../data/use-decisions.js'
import { useCheckRun, trackedOf } from '../../data/run.js'
import { withRunSheet } from '../../data/change.js'
import { briefCheckActions } from './check.js'
import { addNote } from './notes-actions.js'
import { recordObjection } from './objection-actions.js'
import { recordTold, toldAnswerOf } from '../../data/told-actions.js'
import { Reading, Failed as Said } from '../../frame-state.jsx'

const Live = withKeys(withFrame(Logic))

export default function Screen({ at, openCommand }) {
  const { data: read, error, reload } = useChange(at)
  const lanes = useResource(LANES, { every: 30000 })
  /* C29: the project's decisions (GET /api/hq/projects/<id>/decisions), read through the one model */
  const decisions = useDecisions(projectOf(read) || (at && at.project) || null)
  /* C30 R-RUN: the tracked Change's check, through the one run client — the
   * sheet followed every second while it runs, and folded into the snapshot,
   * so "Checking now", its elapsed time and its end are the record's. */
  const check = useCheckRun(trackedOf(read), read ? read.runSheet : null, { onEnd: reload })
  /* "Tell the agent": one write at a time, and what it said when it was refused */
  const [toldWrite, setToldWrite] = useState({ busy: false, said: null })
  if (!at || !at.change) return <Said what="No Change is open. Pick one from the Inbox." />
  if (error && !read) return <Said what={error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!read) return <Reading />
  /* a Change with no statement is drawn in its frame, with an honest empty state (logic.js `nothingSaid`) */
  const data = trackedOf(read) ? withRunSheet(read, check.sheet, new Date()) : read
  /* Check it again runs here when the server would start it; otherwise it
   * opens the check sheet (C26 b: untracked work is offered "Track this work
   * as a Change" there), or is off with the server's own reason. */
  const actions = briefCheckActions({
    act: check.act,
    start: check.start,
    stop: check.stop,
    open: () => { if (typeof window !== 'undefined') window.location.hash = href('check', at) },
  })
  /* C30 R-LEDGER: "Add a note" keeps one note on the tracked Change (POST …/notes, free) and re-reads the record */
  const noteChangeId = trackedOf(read)
  const withNotes = { ...actions, noteChangeId, addNote: (body) => addNote({ changeId: noteChangeId, body }), reload }
  /* C30 R-LEDGER (write address 24): "Tell the agent" copies the message — the page does — and keeps that
   * it was copied, on the thread this Brief is about (POST …/told-agent, free); then re-reads, so the
   * button and the "Told the agent" line are the record's. Nothing is sent. */
  const tellAgent = async (message) => {
    if (toldWrite.busy) return
    setToldWrite({ busy: true, said: null })
    const answer = toldAnswerOf(await recordTold({ project: data.project && data.project.id, thread: data.change && data.change.threadId, act: 'copied', message }))
    if (answer.kind === 'kept') {
      setToldWrite({ busy: false, said: null })
      reload()
    } else {
      setToldWrite({ busy: false, said: 'copied to your clipboard, but ThruLine could not keep a record of it. ' + answer.detail })
    }
  }
  withNotes.tellAgent = tellAgent
  /* C30 R-LEDGER (write address 25): "This is wrong…" keeps one objection beside the selected statement
   * (POST …/objections, free) and re-reads the record, so the line under the verdict is the record's.
   * It changes no verdict. */
  withNotes.keepObjection = (statement, reason) => recordObjection({ project: data.project && data.project.id, statement, reason })
  withNotes.told = toldWrite
  return <PortedScreen Logic={Live} Template={Template} at={at} project={projectOf(data)} data={data} actions={withNotes} extra={{ openCommand, frame: { lanes: lanes.data, change: data.change }, decisions }} />
}
