/* One project's decisions, as a hook: read through the one reader and model
 * (./decisions.js), re-read on the same rhythm as the screens' other reads,
 * and at once when a confirmation is recorded (`DECISIONS_CHANGED`, fired by
 * the "Confirm on this Mac" control after ThruLine names a receipt). A screen
 * with no project reads nothing. Never throws, never invents a verdict: a
 * server without the route gives the "not checked" model. */
import { useEffect, useState } from 'react'
import { DECISIONS_CHANGED, decisionsModel, readDecisions } from './decisions.js'

const REREAD_MS = 30_000

export function useDecisions(projectId, { fetchImpl, every = REREAD_MS } = {}) {
  const [model, setModel] = useState(() => decisionsModel(null))
  useEffect(() => {
    let live = true
    if (!projectId) {
      setModel(decisionsModel(null))
      return undefined
    }
    const read = () =>
      readDecisions(projectId, fetchImpl).then((r) => {
        if (live) setModel(decisionsModel(r))
      })
    read()
    const t = every ? setInterval(read, every) : null
    const on = typeof window !== 'undefined' && window.addEventListener ? window : null
    if (on) on.addEventListener(DECISIONS_CHANGED, read)
    return () => {
      live = false
      if (t) clearInterval(t)
      if (on) on.removeEventListener(DECISIONS_CHANGED, read)
    }
  }, [projectId, fetchImpl, every])
  return model
}
