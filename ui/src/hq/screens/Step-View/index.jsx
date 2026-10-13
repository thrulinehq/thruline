/* Step-View on a live address: `#/changes/<id>/step/<n>[?project=<p>]`
 * (C30 R-ROUTES "Step view").
 *
 * The step is read from the ONE shared Change snapshot (use-change.js →
 * change.js `steps`), so its number is the number every other screen and chip
 * uses. The session's own lines around it come from the server's session
 * timeline (`GET /api/hq/sessions/<sid>`, a read) for the step's session —
 * and only that one. This screen writes nothing and reaches no write address.
 *
 * An address that names no step the record numbers draws a visible "no such
 * step" page with links back to the Brief and Every statement. It never
 * redirects, and never falls back to the Inbox.
 */
import React, { useMemo } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { useChange } from '../../data/use-change.js'
import { API, useResource } from '../../data/api.js'
import { LANES, withFrame } from '../../data/project.js'
import { stepAt, stepViewFromChange } from './from-change.js'
import { Reading, Failed } from '../../frame-state.jsx'

const Live = withFrame(Logic)

export default function Screen({ at, openCommand }) {
  const change = useChange(at)
  const lanes = useResource(LANES, { every: 30000 })
  const row = stepAt(change.data, at && at.step)
  const timeline = useResource(row && row.sessionId ? API.session(row.sessionId) : null)
  /* a session read that is still the previous step's session is not this step's lines */
  const lines = timeline.data && row && timeline.data.session_id === row.sessionId ? timeline.data : null
  const data = useMemo(
    () => (change.data ? stepViewFromChange(change.data, at, { timeline: lines, timelineError: lines ? null : timeline.error }) : null),
    [change.data, at, lines, timeline.error],
  )
  if (change.error && !change.data) return <Failed what={change.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!data) return <Reading />
  return <PortedScreen Logic={Live} Template={Template} at={at} project={projectOf(change.data)} data={data} extra={{ openCommand, frame: { lanes: lanes.data, change: change.data && change.data.change } }} />
}
