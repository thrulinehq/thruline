import React from 'react'

import {
  REFRESH_ENDPOINTS,
  REFRESH_RECOVERY,
  refreshPresentation,
} from './lib/refresh-presentation.js'

/* The measured refresh remains a foreground-server action. This component
 * discovers only the fixed same-origin state endpoint, starts only the fixed
 * refresh endpoint, and never renders under the ordinary Vite dev server. */

const POLL_MS = 3000

export default function Refresh() {
  const [server, setServer] = React.useState(null)
  const [phase, setPhase] = React.useState('idle')
  const observedBusy = React.useRef(false)
  const view = refreshPresentation(server, phase)

  React.useEffect(() => {
    let live = true
    fetch(REFRESH_ENDPOINTS.state, { cache: 'no-store' })
      .then((response) => (response.ok ? response.json() : null))
      .then((state) => {
        if (!live || !state) return
        setServer(state)
        if (state.refreshing) {
          observedBusy.current = true
          setPhase('already_running')
        } else {
          setPhase('idle')
        }
      })
      .catch(() => {
        /* No state endpoint is the expected dev-server case: stay hidden. */
      })
    return () => {
      live = false
    }
  }, [])

  React.useEffect(() => {
    if (!['busy', 'already_running'].includes(phase)) return undefined
    let live = true
    const poll = async () => {
      try {
        const response = await fetch(REFRESH_ENDPOINTS.state, { cache: 'no-store' })
        if (!response.ok) {
          if (live) setPhase('connection_failure')
          return
        }
        const state = await response.json()
        if (!live) return
        setServer(state)
        if (state.refreshing) {
          observedBusy.current = true
          return
        }
        // A 202 can arrive just before the worker publishes `refreshing=true`.
        // A prior successful result is not completion until this page has seen
        // the new worker running at least once.
        if (!observedBusy.current) return
        setPhase(state.last_refresh?.ok === true ? 'complete' : 'refresh_failure')
      } catch {
        if (live) setPhase('connection_failure')
      }
    }
    const timer = setInterval(poll, POLL_MS)
    poll()
    return () => {
      live = false
      clearInterval(timer)
    }
  }, [phase])

  React.useEffect(() => {
    if (view.reloadEligible) window.location.reload()
  }, [view.reloadEligible])

  const start = async () => {
    observedBusy.current = false
    setPhase('starting')
    try {
      const response = await fetch(REFRESH_ENDPOINTS.start, { method: 'POST' })
      if (response.status === 409) {
        observedBusy.current = true
        setPhase('already_running')
        return
      }
      if (!response.ok) {
        setPhase('start_failure')
        return
      }
      setPhase('busy')
    } catch {
      setPhase('connection_failure')
    }
  }

  if (!view.visible) return null

  return (
    <section
      className={`os-refresh-region${view.error ? ' error' : ''}`}
      role="status"
      aria-live={view.error ? 'assertive' : 'polite'}
      aria-atomic="true"
    >
      <div className="os-refresh-copy">
        <strong>{view.title}</strong>
        <span>{view.message}</span>
      </div>
      <details className="os-refresh-recovery">
        <summary>Stopping or recovering</summary>
        <p>{REFRESH_RECOVERY}</p>
      </details>
      <button type="button" onClick={start} disabled={view.busy || view.reloadEligible}>
        {view.actionLabel}
      </button>
    </section>
  )
}
