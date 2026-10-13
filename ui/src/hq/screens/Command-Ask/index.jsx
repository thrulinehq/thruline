/* Command-Ask (⌘K — find, ask, act). In the app it opens as an OVERLAY over
 * whatever screen is showing (app.jsx renders `<Command at onClose/>`): only the
 * palette, its answer and a scrim render, fixed and centred; Esc, a click on
 * the scrim, or following one of its links (by click or ↵) calls `onClose`.
 *
 * What it reads (no write, no model, no network beyond this machine's server):
 *   GET /api/hq/projects                       which project is current
 *   the one Change loader (data/use-change.js) for the open Change: a thread id
 *     or a tracked `chg_` id, its tracked documents and its run sheet folded
 *     into the one snapshot — Find's rows of this Change and all five answers
 *     come from it, and Check it again opens the sheet already running when the
 *     server would start it (C30 R-RUN)
 *   GET /api/hq/projects/<id>                  the project's Changes (Find with no Change open)
 *   the project's readiness (data/your-move.js) whose move each Find row is, as the Inbox files it
 *   GET /api/hq/projects/<id>/threads/<tid>    the open Change's own thread view, for the
 *                                              one fact the snapshot does not carry: which
 *                                              edits delete a file ("Was anything destructive?")
 *   GET /api/hq/projects/<id>/search?q=…       the rest of the record, for what is typed
 *                                              (data/search.js; C30 R-ROUTES "Record search")
 *   GET /api/hq/projects/<id>/decisions        the project's decisions, for Find (C29;
 *                                              data/use-decisions.js)
 * Nothing here writes. "Copy for the agent" writes to this machine's clipboard
 * and sends nothing; the message is the check sheet's own (tell.js).
 */
import React, { useMemo, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { API, useResource } from '../../data/api.js'
import { hhmm } from '../../data/change.js'
import { LANES, currentProjectId, withFrame } from '../../data/project.js'
import { useYourMove } from '../../data/your-move.js'
import { commandFromChange } from './from-change.js'
import { tellOf } from './tell.js'
import { useDecisions } from '../../data/use-decisions.js'
import { useChange } from '../../data/use-change.js'
import { useRecordSearch } from '../../data/search.js'
import { runActOf, trackedOf, openCheckActions } from '../../data/run.js'

const Live = withFrame(Logic)

export default function Screen({ at, onClose, askFirst = false, keysMode = 'on' }) {
  const where = at || {}
  const projects = useResource(API.projects)
  const lanes = useResource(LANES, { every: 30000 })
  /* C30 R-UNTRACKED / R-RUN: the open Change through the one Change loader, a
   * thread id and a tracked Change's `chg_` id alike. */
  const ch = useChange(where.change ? { change: where.change, project: where.project } : null)
  const snap = (where.change && ch.data) || null
  const snapProject = snap && snap.project && snap.project.id ? String(snap.project.id) : null
  /* the open Change's own project, else the one the address names, else the latest */
  const pid = snapProject || (projects.data ? currentProjectId(projects.data.projects, where.project) : null)
  const view = useResource(pid ? API.project(pid) : null)
  /* C46 §1.1: whose move each thread is, in the Inbox's own filing (the project's readiness, shared).
   * NOT-APPROVAL-WORDS: only which Changes ARE ready reaches from-change.js; nothing says why approval is off. */
  const row = projects.data && Array.isArray(projects.data.projects) ? projects.data.projects.find((p) => p && p.id === pid) || null : null
  const yourMove = useYourMove(row)
  const ready = yourMove && yourMove.project === String(pid) ? yourMove.ready : null
  const threadView = useResource(snapProject && snap.change.threadId ? API.thread(snapProject, snap.change.threadId) : null)
  /* C29: the project's decisions, findable by their words (GET /api/hq/projects/<id>/decisions) */
  const decisions = useDecisions(pid)
  /* what is typed, as the palette tells it; the record is searched for it */
  const [query, setQuery] = useState('')
  const found = useRecordSearch(pid, query, { thread: snap ? snap.change.threadId : null })

  const data = useMemo(() => {
    const now = new Date()
    const readAt = hhmm(now.toISOString())
    if ((projects.error && !snapProject) || view.error) return Object.assign(commandFromChange(null, null, { readAt }), { failed: true })
    if (!view.data || (where.change && !snap && !ch.error)) return Object.assign(commandFromChange(null, null, { readAt }), { loading: true })
    /* which of the open Change's edits delete a file: the thread view's typed `action`; null when it was not read */
    const deletions = threadView.data ? (threadView.data.edits || []).filter((e) => e && e.action === 'delete').map((e) => e.id) : null
    return commandFromChange(snap, view.data, { readAt, tell: snap ? tellOf(snap, { now }) : null, deletions, ready })
  }, [projects.error, snapProject, view.data, view.error, where.change, snap, ch.error, threadView.data, ready])

  /* the act the served run sheet allows; a thread no Change tracks has none, and its
   * Check it again opens the sheet's ready form ("Track this work as a Change") */
  const actions = where.change
    ? openCheckActions({ act: runActOf(trackedOf(snap), snap ? snap.runSheet : null), at: { change: where.change, project: where.project || pid || undefined } })
    : undefined
  const recording = snap && snap.change ? { recording: snap.change.recording } : null

  return <PortedScreen Logic={Live} Template={Template} at={where} project={pid || undefined} data={data} actions={actions} extra={{ overlay: true, onClose, askFirst, keysMode, decisions, projectId: pid || undefined, found, onQuery: setQuery, frame: { lanes: lanes.data, change: recording } }} />
}
