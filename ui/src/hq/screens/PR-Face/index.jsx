/* On the pull request, on a live address (`#/changes/<id>/pr`): a PREVIEW of
 * the note for this Change's pull request, and of what the ThruLine Action
 * would post for its folder (C30 R-PR).
 *
 * Reads, all GETs: the ONE Change snapshot and the tracked Change's documents
 * and model through `Repair-Loop/live.js` `useLive` (read again every 20 s,
 * so the note follows the record), and `GET /api/changes/<id>/pr` through
 * `read.js` (the folder's branch and receipts, the CLI's check-run name and
 * badge words). `from-change.js` selects the facts, `markdown.js` writes the
 * note, and the artboard's own markup draws it.
 *
 * ThruLine posts nothing from here. Copy as Markdown puts `data.copy` on the
 * clipboard (copy.js). The one write is for work nobody tracked: "Track this
 * work as a Change", through the existing client `Repair-Loop/actions.js`
 * `trackThenReread` (`POST /api/first-use/change {project, thread}`, write
 * address 14) — never a note drawn without a Change.
 */
import React, { useEffect, useRef, useState } from 'react'
import Template from './template.jsx'
import Logic from './logic.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
import { useLive, trackedIdOf, Reading, Said } from '../Repair-Loop/live.js'
import { trackThenReread } from '../Repair-Loop/actions.js'
import { href } from '../../routes.js'
import { prFaceFrom, noteFacts } from './from-change.js'
import { usePrPreview, checkReceipt, PR_REREAD_MS } from './read.js'
import { copyMarkdown } from './copy.js'

export default function Screen({ at }) {
  const live = useLive(at)
  const trackedId = live.tracked ? live.tracked.id : null
  const pr = usePrPreview(trackedId)
  const [track, setTrack] = useState({ busy: false, said: null })
  const [receiptCheck, setReceiptCheck] = useState({ changeId: null, busy: false, error: null, checked: null })
  /* the first read on this page, for the edited-in-place panel */
  const first = useRef({ key: null, value: null })
  /* the tracked Change's documents are read again on the snapshot's clock */
  const reload = useRef(live.reload)
  reload.current = live.reload
  useEffect(() => {
    if (!trackedId) return undefined
    const t = setInterval(() => { if (reload.current) reload.current() }, PR_REREAD_MS)
    return () => clearInterval(t)
  }, [trackedId])

  if (!at || !at.change) return <Said what="No Change is open. Pick one from the Inbox." />
  if (live.error && !live.snapshot) return <Said what={live.error.message || 'ThruLine could not read this Change. Reload this page.'} />
  if (!live.snapshot || live.trackedLoading) return <Reading />
  if (trackedIdOf(live.snapshot) && !live.tracked) return <Said what="ThruLine could not read this Change’s own record, so there is no note to preview. Reload this page." />

  const now = Date.now()
  if (live.tracked && first.current.key !== live.tracked.id) {
    first.current = { key: live.tracked.id, value: { facts: noteFacts(live.snapshot, { now, tracked: live.tracked }), readAt: new Date(now).toISOString() } }
  }
  const activeCheck = receiptCheck.changeId === trackedId ? receiptCheck : null
  const data = prFaceFrom(live.snapshot, {
    now, tracked: live.tracked, pr, first: live.tracked ? first.current.value : null, track,
    checked: activeCheck?.checked, check: activeCheck,
  })
  /* the thread on screen, as the first-use route takes it */
  const where = { project: live.snapshot.project && live.snapshot.project.id, thread: live.snapshot.change && live.snapshot.change.threadId }
  const actions = {
    copy: (text) => copyMarkdown(text),
    check: () => {
      if (!trackedId || activeCheck?.busy || !pr?.doc?.receipts?.newest) return
      setReceiptCheck({ changeId: trackedId, busy: true, error: null, checked: null })
      checkReceipt(trackedId).then(
        (result) => setReceiptCheck({ changeId: trackedId, busy: false, error: null, checked: result.checked }),
        (error) => setReceiptCheck({ changeId: trackedId, busy: false, error: error?.message || 'The receipt could not be checked.', checked: null }),
      )
    },
    track: () => {
      if (track.busy) return
      setTrack({ busy: true, said: null })
      trackThenReread(where, {
        reread: live.reload,
        open: (id) => { if (typeof window !== 'undefined') window.location.hash = href('pr', { change: id, project: at.project }) },
      }).then((r) => setTrack({ busy: false, said: r.ok ? null : r.said }))
    },
  }
  return <PortedScreen Logic={Logic} Template={Template} at={at} project={projectOf(live.snapshot)} data={data} actions={actions} />
}
