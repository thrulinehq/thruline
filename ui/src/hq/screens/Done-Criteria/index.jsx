/* Done-Criteria on a live address: what counts as done for the Change.
 *
 * Reads (../Repair-Loop/live.js): the one Change snapshot, and the tracked
 * Change's documents (proposals, accepted version, coverage) and model; and
 * (templates.js) what this project's .thruline/policy.yml holds for templates.
 * Writes (actions.js, each a named constant beside its control):
 * POST …/scope/accept (Accept what counts as done) and POST …/links (Link
 * step N). Without a tracked Change nothing is proposed and neither is
 * reachable: the one act offered is "Track this work as a Change" (POST
 * /api/first-use/change {project, thread}, write address 14), through
 * Repair-Loop/actions.js's one client (C30 R-UNTRACKED).
 */
import React, { useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { useLive, Reading, Said } from '../Repair-Loop/live.js'
import { trackThenReread } from '../Repair-Loop/actions.js'
import { selectDoneCriteria } from './from-change.js'
import { acceptCriteria, acceptAnswer, linkRun } from './actions.js'
import { DONE_TEMPLATES } from './templates.js'
import { useResource } from '../../data/api.js'
import { LANES, withFrame } from '../../data/project.js'
import { href } from '../../routes.js'

const Live = withFrame(Logic)

export default function Screen({ at, openCommand }) {
  const live = useLive(at)
  const lanes = useResource(LANES, { every: 30000 })
  const projectId = projectOf(live.snapshot)
  const templates = useResource(projectId ? DONE_TEMPLATES(projectId) : null)
  const [track, setTrack] = useState({ busy: false, said: null, note: null })
  const [accepting, setAccepting] = useState({ busy: false, said: null, version: null })
  if (!at || !at.change) return <Said what="No Change is open. Pick one from the Inbox." />
  if (live.error && !live.snapshot) return <Said what={live.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!live.snapshot) return <Reading />
  const data = selectDoneCriteria(live.snapshot, live.tracked, { templates })
  data.track = { untracked: !live.tracked && !live.trackedLoading, loading: live.trackedLoading, busy: track.busy, said: track.said, note: track.note }
  data.act.accepting = accepting
  const changeId = data.act.changeId
  const where = { project: data.act.project, thread: data.act.thread }
  const actions = {
    /* the one accept act: its answer is said on the bar — the version it
       wrote, or the refusal in the server's words — and the record is read again */
    accept: (draft) => {
      setAccepting({ busy: true, said: null, version: null })
      return acceptCriteria({ changeId, ...draft }).then((r) => {
        const said = acceptAnswer(r)
        setAccepting({ busy: false, said: said.ok ? null : said.said, version: said.ok ? said.version : null })
        if (said.ok) live.reload()
        return { ...r, ok: said.ok }
      })
    },
    link: (criterion) => linkRun({ changeId, criterion, evidence: data.act.unlinkedRun && data.act.unlinkedRun.id }).then(live.reload),
    track: () => {
      if (track.busy) return
      setTrack((t) => ({ ...t, busy: true, said: null }))
      trackThenReread(where, {
        reread: live.reload,
        open: (id) => { if (typeof window !== 'undefined') window.location.hash = href('done', { change: id, project: at.project }) },
      }).then((r) => setTrack({ busy: false, said: r.ok ? null : r.said, note: r.ok ? r.note : null }))
    },
  }
  return <PortedScreen Logic={Live} Template={Template} at={at} project={projectOf(live.snapshot)} data={data} actions={actions} extra={{ openCommand, frame: { lanes: lanes.data, change: live.snapshot.change } }} />
}
