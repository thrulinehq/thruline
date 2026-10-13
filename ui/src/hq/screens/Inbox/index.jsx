/* Inbox on a live address: every thread of work in the current project
 * (`?project=`, else the most recently active), grouped by whose move it is,
 * read through the one project model (ui/src/hq/data/project.js). */
import React, { useRef, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { withKeys } from '../../keys/registry.js'
import { useInbox } from '../../data/use-project.js'
import { href } from '../../routes.js'
import { useResource } from '../../data/api.js'
import { LANES, recordingPill, withFrame } from '../../data/project.js'
import { useDecisions } from '../../data/use-decisions.js'
import { FrameState } from '../../frame-state.jsx'
import { writeInboxHide } from '../../data/inbox-hide.js'
import { useCapture } from '../../data/capture.js'

// the list keys (C30 R-KEYS): the same selected row handles j k ↵ . ⌘↵ y h e
const Live = withKeys(withFrame(Logic))

/** One quiet line, said inside the app frame (frame-state.jsx): how the design says "wait" and "this failed". */
export function Quiet({ children, role = 'status' }) {
  return <FrameState role={role}>{children}</FrameState>
}

export default function Screen({ at, openCommand, keysMode }) {
  const lanes = useResource(LANES, { every: 30000 })
  // an agent session on this machine written to in the last few minutes: how the Inbox notices a
  // recording that starts after the page opened (the project's own view re-reads only while it shows one)
  const { data, reading, projectId, noProject, error, reloadHides, reloadInbox } = useInbox(at && at.project, { machineRecording: recordingPill({ lanes: lanes.data }).on })
  const capture = useCapture(projectId)
  const decisions = useDecisions(projectId || (at && at.project) || null)
  const [hideNote, setHideNote] = useState(null)
  const pendingHide = useRef(false)
  const hideRow = (row) => {
    if (!projectId || pendingHide.current) return false
    pendingHide.current = true
    setHideNote(null)
    writeInboxHide(projectId, row).then(
      () => reloadHides(),
      (problem) => setHideNote(problem && problem.message ? problem.message : 'ThruLine could not hide that row. Reload the Inbox to try again.'),
    ).finally(() => { pendingHide.current = false })
    return undefined
  }
  if (error) {
    return (
      <Quiet role="alert">
        ThruLine could not read the record just now. Check that <code style={{ font: "13px 'Geist Mono', monospace", color: '#EDEDEF' }}>thruline ui</code> is still running, then reload this page.
      </Quiet>
    )
  }
  if (noProject) {
    return (
      <Quiet>
        Nothing is recorded on this machine yet.{' '}
        <a href={href('start')} style={{ color: '#FF7A29' }}>
          Choose a project to record
        </a>
      </Quiet>
    )
  }
  // C29 R1: never a blank page. Until the project's view arrives, the canvas's own frame is drawn
  // from the project list, and only the list area says it is reading (project.js inboxReading).
  const health = capture.facts
  const captureCard = health ? {
    ...(health.hooks.installed === true ? { claudeCode: { hookVersion: health.hooks.version ? `v${health.hooks.version}` : null } } : {}),
    ...(!health.codex.configured || ['disconnected', 'stopped'].includes(health.codex.state)
      ? { codex: { answering: false, configured: health.codex.configured, state: health.codex.state } } : {}),
  } : {}
  const shown = data ? { ...data, capture: captureCard, ...(hideNote ? { note: [data.note, hideNote].filter(Boolean).join(' · ') } : {}) } : data
  return <PortedScreen Logic={Live} Template={Template} at={{ ...(at || {}), project: projectId || (at && at.project) }} data={shown || reading} extra={{ openCommand, keysMode, frame: { lanes: lanes.data }, decisions, hideRow, reloadInbox, retryCapture: capture.reload }} />
}
