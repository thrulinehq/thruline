/* First run on a live address: every folder where an agent has worked (each
 * an HQ project). A folder that already has records opens its Inbox at once;
 * for one that has none, the one primary action, "Load every session", starts
 * reading every Claude Code and Codex recording of it through the existing
 * ingest (POST HQ_LOAD_ADDRESS, ui/src/hq/data/use-project.js) and opens the
 * Inbox straight away, where the status bar follows the load (C27).
 *
 * Step 2, "What ThruLine will do" (C30 R-ROUTES "Install"): the folder in
 * front of the person is read through GET /api/hq/projects/<id>/install
 * (install.js) — what each part would write, where it is already installed,
 * or why it would be refused. "Install for this project" sends exactly the
 * ticked boxes against that plan (POST, the run token): the click is the
 * consent. After it, the plan is the one the route read after writing, and
 * step 3's capture read is done again.
 *
 * Step 3, "Check it's recording" (C30 R-ROUTES "Capture health"): the folder
 * in front of the person — and only that folder — is read through
 * GET /api/hq/projects/<id>/capture (data/capture.js). It is read when the
 * folder is chosen, again when step 3 opens, after an install and on every
 * Look again, and every 2 s while step 3 is open and the record says it is
 * waiting for a first step, stopping once one is seen. Recording, its first
 * thread is read (first-ask.js) for "First steps recorded" and "Review what
 * counts as done". The install POST is the only write here besides the load. */
import React, { useEffect, useRef, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { useFirstRun } from '../../data/use-project.js'
import { useCapture } from '../../data/capture.js'
import { href } from '../../routes.js'
import { Quiet } from '../Inbox/index.jsx'
import { withFrame } from '../../data/project.js'
import { withCapture } from './capture-view.js'
import { useFirstAsk } from './first-ask.js'
import { useInstall, withInstall } from './install.js'

const Live = withFrame(Logic)

export default function Screen({ at, timers }) {
  const { data, loading, error, startError, start, finishedFor } = useFirstRun()
  useEffect(() => {
    if (finishedFor) window.location.hash = href('inbox', { project: finishedFor })
  }, [finishedFor])
  // the folder and step in front of the person, as the screen reports them (logic.js tellPlace); it opens on step 1
  const [place, setPlace] = useState(null)
  const folder = place ? place.folder : data ? data.defaultFolder || null : null
  const step = place ? place.step : 1
  const capture = useCapture(folder, { poll: step === 3, timers })
  const install = useInstall(folder)
  const ask = useFirstAsk(folder, step === 3 && !!capture.facts && capture.facts.phase === 'recording')
  // each folder's last Git answer, so a folder read once keeps it in the list
  const [known, setKnown] = useState({})
  useEffect(() => {
    const id = capture.project
    if (id && capture.facts) setKnown((k) => (k[id] === capture.facts.git ? k : Object.assign({}, k, { [id]: capture.facts.git })))
  }, [capture.project, capture.facts])
  // opening step 3 reads the folder again ("Read from the record when you open step 3")
  const lastStep = useRef(step)
  useEffect(() => {
    if (step === 3 && lastStep.current !== 3 && folder) capture.reload()
    lastStep.current = step
  }, [step]) // eslint-disable-line react-hooks/exhaustive-deps
  if (error) {
    return (
      <Quiet role="alert">
        ThruLine could not read which folders it has seen. Check that <code style={{ font: "13px 'Geist Mono', monospace", color: '#EDEDEF' }}>thruline ui</code> is still running, then reload this page.
      </Quiet>
    )
  }
  if (loading || !data) return <Quiet>Reading the record…</Quiet>
  const withRead = withCapture(Object.assign({}, data, { loadError: startError || '' }), { capture, ask, known, now: new Date() })
  const live = withInstall(withRead, install)
  // C27: a folder with records opens its Inbox at once; a load starts and the Inbox opens without waiting for it
  const open = (id) => { window.location.hash = href('inbox', { project: id }) }
  const load = (id) => start(id).then((ok) => { if (ok) open(id) })
  // the click is the consent: exactly these boxes, then step 3's capture read again
  const installChosen = (choices) => install.run(choices).then((r) => { capture.reload(); return r })
  return <PortedScreen Logic={Live} Template={Template} at={at} data={live} actions={{ load, open, place: setPlace, lookAgain: () => { capture.reload(); install.reload() }, install: installChosen }} />
}
