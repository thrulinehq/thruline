/* The live inputs the three W2-D screens (Done-Criteria, Approve-Sheet,
 * Repair-Loop) share: the ONE Change snapshot (`data/use-change.js`), and —
 * where a person tracks this work as a Change — that Change's own documents,
 * read through the existing `readChangeDocuments` and derived through the
 * existing `deriveChangeModel`, plus its run sheet (`GET …/run`). The snapshot
 * carries what every screen draws; the tracked documents carry what the writes
 * need (the Change id, the version the person saw, the run sheet's check and
 * permission, the coverage's unlinked run). Nothing here is re-derived.
 *
 * READS ONLY. Every address here is a GET.
 */
import React, { useEffect, useState } from 'react'
import { useChange } from '../../data/use-change.js'
import { API, readJson, useResource } from '../../data/api.js'

/** The tracked Change's id, when the snapshot is of a tracked Change. */
export function trackedIdOf(snapshot) {
  const c = snapshot && snapshot.change
  return c && c.id && c.threadId && c.id !== c.threadId ? c.id : null
}

/** One tracked Change's documents and model, or null when they cannot be read. */
export async function readTracked(id) {
  const [{ readChangeDocuments }, { deriveChangeModel }] = await Promise.all([
    import('../../../workspace/change-workspace.jsx'),
    import('../../../workspace/change-model.js'),
  ])
  const docs = await readChangeDocuments(id, { runs: true })
  if (!docs || docs.status !== 'ready') return null
  const model = deriveChangeModel(docs.change, docs.coverage, docs.applicability, docs.proposals, docs.work)
  let runSheet = null
  try { runSheet = await readJson(API.run(id)) } catch (_e) { runSheet = null }
  return { id, change: docs.change, proposals: docs.proposals, coverage: docs.coverage, model, runSheet }
}

/** `{snapshot, tracked, licence, error, reload}` for one live address. */
export function useLive(at, { licence = false } = {}) {
  const ch = useChange(at)
  const id = trackedIdOf(ch.data)
  const [tracked, setTracked] = useState({ id: null, value: null })
  const [tick, setTick] = useState(0)
  useEffect(() => {
    let live = true
    if (!id) { setTracked({ id: null, value: null }); return undefined }
    readTracked(id).then((value) => live && setTracked({ id, value }), () => live && setTracked({ id, value: null }))
    return () => { live = false }
  }, [id, tick])
  const lic = useResource(licence ? '/api/licence' : null)
  return {
    snapshot: ch.data,
    loading: ch.loading,
    error: ch.error,
    tracked: tracked.id === id ? tracked.value : null,
    /* true while the snapshot names a tracked Change whose documents are still being read */
    trackedLoading: Boolean(id) && tracked.id !== id,
    work: ch.work || [],
    licence: lic.data || null,
    reload: () => { if (ch.reload) ch.reload(); setTick((n) => n + 1); if (lic.reload) lic.reload() },
  }
}

/* Reading and a plain failure are said inside the app frame (frame-state.jsx), never as a page of their own. */
export { Reading, Failed as Said } from '../../frame-state.jsx'
