/* Settings on a live address: the canvas's markup and logic, on what this
 * machine's ThruLine serves.
 *
 * Reads (each may fail on its own; a failed read is said, never drawn as a value):
 *   /api/storage, /api/lanes, /api/observation, /api/mind/status, /api/licence,
 *   /api/hq/projects, /api/preferences (data/preferences.js, read once for the app),
 *   and, for the project in front of the person, /api/hq/projects/<id>/capture
 *   (data/capture.js: its `holds`, where its pause stands in the record) and
 *   /api/hq/projects/<id>/install (Onboarding/install.js: where ThruLine's hooks
 *   and speaker hooks stand), and /api/hq/projects/<id>/run-permissions
 *   (one row per exact recorded check folder, including historical grants).
 *   /api/hq/projects/<id>/done-templates gives named lines from the committed
 *   base policy for a read-only preview.
 * Writes, each reached by one control:
 *  - `POST /api/hq/projects/<id>/capture/pause` with `{act: "paused" | "resumed"}`
 *    records a pause of that project, or ends one (write address 26, through
 *    data/holds.js; C46 §1.2(1)). The six capture callers honour it, and the
 *    same route pauses HQ's live watch on Codex when it watches that folder.
 *    It is reachable only from the pause switch, and only once `holds` is read.
 *  - `POST /api/hq/projects/<id>/capture/exclusions` with
 *    `{act: "excluded" | "included", pattern}` records a path exclusion or
 *    ends one (write address 27, through data/holds.js). The current `holds`
 *    are read again after each act; sealed transcripts are not changed.
 *  - The speaker switch (C46 §1.2(2)): On is `POST /api/hq/projects/<id>/install`
 *    with only the speaker ticked (write address 20); Off is
 *    `POST /api/hq/projects/<id>/uninstall` `{part: "speaker", plan}` (write
 *    address 30). Both carry the run token and the plan the page read, and
 *    change only the project's own `.claude/settings.local.json`.
 *  - `POST /api/preferences` keeps the person's choices in preferences.json,
 *    never in the record (C30 R-KEYS, R-PREFS, through data/preferences.js).
 *  - `POST /api/changes/<id>/run/permission[/revoke]`, or historical
 *    `POST /api/hq/projects/<id>/run-permissions/<folder>/revoke`, acts only
 *    for the server-owned folder identity in the row (existing write addresses
 *    7/8 for current Changes; new write address 31 for historical revoke).
 *    Every answer is followed by a fresh permission read before the row
 *    claims where it stands. Neither control starts a check.
 *  - Look somewhere else opens the existing Check sheet for a current Change
 *    in a recorded folder. Opening writes nothing. The sheet alone owns its
 *    existing allow, run and cancel addresses, and names that choosing a place
 *    starts the check there.
 * No other Settings control writes anything.
 */
import React, { useMemo, useRef, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { API, useResource } from '../../data/api.js'
import { useCapture } from '../../data/capture.js'
import { useHqStopped } from '../../data/hq-beat.js'
import { settingsFromServer } from './from-server.js'
import { Reading, Failed } from './state.jsx'
import { READS, setPause, setExclusion } from './writes.js'
import { postUninstall, uninstallAnswer, useInstall } from '../Onboarding/install.js'
import { currentProjectId, withFrame } from '../../data/project.js'
import { usePreferences, preferences } from '../../data/preferences.js'
import { allowBrowserNotifications } from '../../data/browser-notification-permission.js'
import { useRunPermissions, writeThenRead } from './run-permissions.js'
import { stopCheck, stopRefusalWords } from '../Repair-Loop/actions.js'
import { RunSheet } from '../../../workspace/run-sheet.jsx'
import { DONE_TEMPLATES } from '../Done-Criteria/templates.js'
import '../../../workspace/workspace.css'

const Live = withFrame(Logic)

/** The time the page counts as read: now, until HQ stops; then the last moment it was read ("read HH:MM · not refreshing"). */
function readAt(stopped, ref) {
  if (stopped && ref.current) return ref.current
  const now = new Date()
  if (!stopped) ref.current = now
  return now
}

export default function Screen({ at, openCommand }) {
  const [openCheck, setOpenCheck] = useState(null)
  const [notificationPermission, setNotificationPermission] = useState(null)
  const storage = useResource(READS.storage)
  const lanes = useResource(READS.lanes, { every: 60000 })
  const observation = useResource(READS.observation)
  const mind = useResource(READS.mind)
  const licence = useResource(READS.licence)
  const projects = useResource(READS.projects)
  const wanted = at && at.project
  const current = currentProjectId(projects.data && projects.data.projects, wanted) || null
  const capture = useCapture(current)
  const captureDoc = useMemo(() => (capture.doc ? capture.doc : capture.error ? { error: capture.error } : null), [capture.doc, capture.error])
  const install = useInstall(current)
  const installDoc = useMemo(() => (install.doc ? install.doc : install.error ? { error: install.error } : null), [install.doc, install.error])
  const permissions = useRunPermissions(current)
  const permissionsDoc = useMemo(() => (permissions.error ? { error: permissions.error } : permissions.doc), [permissions.doc, permissions.error])
  const templates = useResource(current ? DONE_TEMPLATES(current) : null)
  // the project's newest work, which Rules names in its What counts as done link
  const projectView = useResource(current ? API.project(current) : null)
  const stamp = [storage, lanes, observation, mind, licence, projects].map((r) => (r.data ? 1 : 0)).join('')
  // HQ itself stopping (DESIGN-SPEC §3.10, stopped): the page keeps what it read, says when it read it, and is no longer refreshed
  const hqStopped = useHqStopped()
  const readAtRef = useRef(null)
  const data = useMemo(() => settingsFromServer({
    storage: storage.data, lanes: lanes.data, observation: observation.data, mind: mind.data,
    licence: licence.data, projects: projects.data, projectsError: projects.error, capture: captureDoc, install: installDoc,
    runPermissions: permissionsDoc, templates, projectView: projectView.error ? { error: projectView.error } : projectView.data, current: wanted, now: readAt(hqStopped, readAtRef), hqStopped,
  }), [storage.data, lanes.data, observation.data, mind.data, licence.data, projects.data, projects.error, captureDoc, installDoc, permissionsDoc, templates.data, templates.error, templates.loading, projectView.data, projectView.error, wanted, stamp, hqStopped]) // eslint-disable-line react-hooks/exhaustive-deps
  const prefs = usePreferences()
  const actions = {
    reloadObservation: () => observation.reload(),
    reloadCapture: () => capture.reload(),
    // Identity changes only when the capture GET answers. A kept exclusion
    // answer remains the pane's source while this is the older document.
    captureDoc: capture.doc,
    // the record answers; then where it stands is read again, and the Codex watch with it
    setPause: (act) => setPause(current, act).then((a) => {
      capture.reload()
      observation.reload()
      return a
    }),
    setExclusion: (act, pattern) => setExclusion(current, act, pattern).then((a) => {
      capture.reload()
      return a
    }),
    // On: the install route with only the speaker ticked; Off: the removal. Then the plan is read again.
    setSpeaker: async (on) => {
      if (on) {
        const r = await install.run({ mcp: false, speaker: true, runcap: false })
        const sp = r && r.ok ? r.parts.speaker : null
        if (sp && (sp.outcome === 'wrote' || sp.outcome === 'already')) return { kind: 'kept' }
        return { kind: 'refused', detail: (sp && sp.refusal && sp.refusal.plain) || (r && r.plain) || 'ThruLine did not add the lines, so nothing changed.' }
      }
      const a = uninstallAnswer(await postUninstall(current, 'speaker', install.doc ? install.doc.plan : null))
      install.reload()
      return a
    },
    setRunPermission: (row) => writeThenRead(current, row, permissions.refresh),
    openCheckPicker: (change) => {
      if (data.checkWhere.choices.some((choice) => choice.change === change)) setOpenCheck({ project: current, change })
    },
    // a refused write is kept in prefs.writeError and said in the pane; it never throws here
    setPreference: async (key, value) => {
      if (key.startsWith('notifications.') && value === true) {
        const permission = await allowBrowserNotifications()
        setNotificationPermission(permission.reason)
        if (!permission.allowed) return false
      } else if (key.startsWith('notifications.')) setNotificationPermission(null)
      return preferences.set(key, value).catch(() => false)
    },
  }

  const all = [storage, lanes, observation, mind, licence]
  if (all.every((r) => r.error)) return <Failed />
  if (!storage.data && !storage.error) return <Reading />
  const picker = openCheck && openCheck.project === current && data.checkWhere.choices.some((choice) => choice.change === openCheck.change)
  return <>
    <PortedScreen Logic={Live} Template={Template} at={at} data={data} actions={actions} extra={{ openCommand, prefs, notificationPermission, frame: { lanes: lanes.data } }} />
    {picker ? <RunSheet key={openCheck.change} changeId={openCheck.change} browsePlaces
      onCancel={async (run, actor) => {
        const answer = await stopCheck({ changeId: openCheck.change, runKey: run, actor })
        return answer.ok ? { status: 'ready' } : { status: 'refused', refusal: stopRefusalWords(answer) }
      }}
      onRan={permissions.refresh}
      onClose={() => { setOpenCheck(null); permissions.refresh() }} /> : null}
  </>
}
