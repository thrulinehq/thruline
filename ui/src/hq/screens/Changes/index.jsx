/* Changes on a live address (C30 R-NAV, `g c`): every Change and thread of
 * work across every project on this machine, drawn with the Inbox's own
 * template and rows, one block per project.
 *
 * Reads only: /api/hq/projects, then /api/hq/projects/<id> for every project
 * that has records (four at a time, newest first), and for each, as the
 * Inbox does, its open Changes' readiness (data/inbox-ready.js; one
 * /api/changes for the whole pass), so a ready Change is its approve row here
 * too (C46 §1.1); each block is drawn when both have arrived. Then
 * /api/storage and /api/lanes. The selected row's `h` writes one Inbox hide
 * act through the same address the Inbox uses. */
import React, { useEffect, useMemo, useRef, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen } from '../../screen.jsx'
import { withKeys } from '../../keys/registry.js'
import { API, readJson, useResource, useStorage } from '../../data/api.js'
import { useProjects } from '../../data/use-project.js'
import { LANES, readingOf, withFrame } from '../../data/project.js'
import { readReadyChanges } from '../../data/inbox-ready.js'
import { keepYourMove, readOnce } from '../../data/your-move.js'
import { changesData, projectsToRead } from './from-projects.js'
import { Quiet } from '../Inbox/index.jsx'
import { INBOX_HIDES_ADDRESS, writeInboxHide } from '../../data/inbox-hide.js'

// the list keys (C30 R-KEYS): each project's selected row handles j k ↵ . ⌘↵ y h e
const Live = withKeys(withFrame(Logic))

/** How many project views are read at once. */
export const AT_ONCE = 4

/**
 * Read every listed project's view, `atOnce` at a time, calling `onView(id, doc)`
 * as each arrives (`{error}` when it could not be read). Resolves when all are read.
 * `get` is the reader (readJson), a seam for tests.
 */
export async function readViews(ids, onView, { get = readJson, atOnce = AT_ONCE, live = () => true } = {}) {
  let next = 0
  const worker = async () => {
    while (next < ids.length && live()) {
      const id = ids[next++]
      let doc
      try {
        doc = await get(API.project(id))
      } catch (e) {
        doc = { error: e && e.message ? e.message : String(e) }
      }
      if (live()) onView(id, doc)
    }
  }
  await Promise.all(Array.from({ length: Math.min(atOnce, ids.length) }, worker))
}

/** Each project's view and its readiness reading (data/project.js `readingOf`), as they arrive.
 *  The readings are kept for every other screen's "your move" (data/your-move.js). */
function useViews(projects) {
  const ids = projectsToRead(projects).map((p) => p.id)
  const key = ids.join(',')
  const [views, setViews] = useState({})
  const [readings, setReadings] = useState({})
  const [hides, setHides] = useState({})
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    if (!ids.length) return undefined
    let live = true
    setViews({})
    setReadings({})
    setHides({})
    // one pass's reads: one /api/changes for every project, and the join reuses each view already read here
    const read = {}
    const once = readOnce(readJson)
    const get = (url) => (url in read ? Promise.resolve(read[url]) : once(url))
    readViews(ids, (id, doc) => {
      setViews((v) => Object.assign({}, v, { [id]: doc }))
      if (doc.error) return
      read[API.project(id)] = doc
      // An unread hide record cannot be counted as an empty one.
      Promise.all([get(INBOX_HIDES_ADDRESS(id)), readReadyChanges(doc, { get })]).then(([hideDoc, r]) => {
        const reading = readingOf(doc, r, hideDoc)
        if (!live || !reading) return
        keepYourMove(reading)
        setHides((x) => Object.assign({}, x, { [id]: hideDoc }))
        setReadings((x) => Object.assign({}, x, { [id]: reading }))
      }, () => {
        if (live) setViews((x) => Object.assign({}, x, { [id]: { error: 'could not read hidden Inbox rows' } }))
      })
    }, { live: () => live })
    return () => { live = false }
  }, [key, revision]) // eslint-disable-line react-hooks/exhaustive-deps
  return { views, readings, hides, refresh: () => setRevision((n) => n + 1) }
}

export default function Screen({ at, openCommand, keysMode }) {
  const list = useProjects()
  const storage = useStorage()
  const lanes = useResource(LANES, { every: 30000 })
  const { views, readings, hides, refresh } = useViews(list.data)
  const pendingHide = useRef(false)
  const [hideNote, setHideNote] = useState(null)
  const hideRow = (project, row) => {
    if (!project || pendingHide.current) return false
    pendingHide.current = true
    setHideNote(null)
    writeInboxHide(project, row).then(refresh, (problem) => {
      setHideNote(problem && problem.message ? problem.message : 'ThruLine could not hide that row. Reload Changes to try again.')
    }).finally(() => { pendingHide.current = false })
    return undefined
  }
  const wanted = at && at.project
  const data = useMemo(() => (list.data ? changesData({ projects: list.data, views, readings, hides, storage: storage.data, now: new Date(), current: wanted }) : null), [list.data, views, readings, hides, storage.data, wanted])
  if (list.error && !list.data) {
    return (
      <Quiet role="alert">
        ThruLine could not read the record just now. Check that <code style={{ font: "13px 'Geist Mono', monospace", color: '#EDEDEF' }}>thruline ui</code> is still running, then reload this page.
      </Quiet>
    )
  }
  if (!data) return <Quiet>Reading the record…</Quiet>
  return <PortedScreen Logic={Live} Template={Template} at={{ ...(at || {}), project: data.currentProject || wanted }} data={hideNote ? { ...data, note: hideNote } : data} extra={{ openCommand, keysMode, frame: { lanes: lanes.data }, hideRow }} />
}
