/* The whole story on a live address (W2-E): one Change, from the ONE snapshot
 * (`data/change.js` through `data/use-change.js`'s `loadChange`), plus the
 * tracked Change's own model where a person tracked this work — read through
 * the existing `readChangeDocuments` / `deriveChangeModel` (`tracked.js`),
 * never re-derived. `from-change.js` chooses and groups those facts for this
 * page.
 *
 * Reads only (through loadChange): /api/hq/projects[/<p>[/threads/<t>]],
 * /api/hq/sessions/<s>, /api/changes[/<id>[/…]], /api/storage; and, for a
 * tracked Change only, GET /api/changes/<id>/export for Readable now
 * (`export-io.js`). Nothing on this screen writes anything: the export is
 * made in this browser from the text the rail previews, and saved or copied
 * there (C30 R-ROUTES "Export and Download").
 */
import React, { useEffect, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { loadChange } from '../../data/use-change.js'
import { wholeStoryFrom } from './from-change.js'
import { readTracked } from './tracked.js'
import { readRecords } from './export-io.js'
import { Reading, Failed } from './state.jsx'
import { API, readJson, useResource } from '../../data/api.js'
import { LANES, withFrame } from '../../data/project.js'

const Live = withFrame(Logic)

export default function Screen({ at, openCommand }) {
  const [state, setState] = useState({ data: null, error: null })
  const [readable, setReadable] = useState(null)
  const [integrity, setIntegrity] = useState(null)
  const lanes = useResource(LANES, { every: 30000 })
  const key = at ? String(at.change || '') + '|' + String(at.project || '') : ''
  useEffect(() => {
    let live = true
    let tracked = null
    setState({ data: null, error: null })
    setReadable(null)
    setIntegrity(null)
    if (!at || !at.change) return undefined
    loadChange(at, { readTracked: async (id) => (tracked = await readTracked(id)), now: Date.now() }).then(
      (out) => {
        if (!live) return
        const data = wholeStoryFrom(out.snapshot, tracked, { now: Date.now() })
        setState({ data, change: out.snapshot.change, project: projectOf(out.snapshot), error: null })
        /* Readable now is about a tracked Change's records; untracked work has none to look at. */
        if (!data.tracked || !data.change.id) {
          setReadable({ status: 'untracked' })
          setIntegrity({ status: 'untracked' })
          return
        }
        setReadable({ status: 'reading' })
        setIntegrity({ status: 'reading' })
        readRecords(data.change.id).then((r) => live && setReadable(r), () => live && setReadable({ status: 'failed', note: null }))
        readJson(API.changeIntegrity(data.change.id)).then(
          (doc) => live && setIntegrity({ status: 'ready', doc }),
          () => live && setIntegrity({ status: 'failed' }),
        )
      },
      (error) => live && setState({ data: null, error }),
    )
    return () => { live = false }
  }, [key]) // eslint-disable-line react-hooks/exhaustive-deps

  if (!at || !at.change) return <Failed what="No Change is open. Pick one from the Inbox." />
  if (state.error) return <Failed what={state.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!state.data) return <Reading />
  return <PortedScreen Logic={Live} Template={Template} at={at} project={state.project} data={state.data} extra={{ openCommand, readable, integrity, frame: { lanes: lanes.data, change: state.change } }} />
}
