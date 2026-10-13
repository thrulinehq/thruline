import React from 'react'
import { observationView } from './observation-model.js'

export function ObservationBar({ onUpdate }) {
  const [data, setData] = React.useState(null)
  const [error, setError] = React.useState('')
  const [open, setOpen] = React.useState(false)
  React.useEffect(() => {
    let live = true, busy = false
    async function read() {
      if (busy) return
      busy = true
      try {
        const response = await fetch('/api/observation', { cache: 'no-store', signal: AbortSignal.timeout(5000) })
        if (!response.ok) throw new Error(`HQ observation status unavailable (${response.status})`)
        const value = await response.json()
        if (live) { setData(value); setError(''); onUpdate?.(value) }
      } catch (e) { if (live) setError(`HQ is disconnected. Previous observation status is not current. ${e.message}`) }
      finally { busy = false }
    }
    read()
    const timer = setInterval(read, 2500)
    return () => { live = false; clearInterval(timer) }
  }, [])
  async function action(action) {
    try {
      const response = await fetch('/api/observation', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action }) })
      if (!response.ok) throw new Error(`Observation action failed (${response.status})`)
      setData(await response.json()); setError('')
    } catch (e) { setError(e.message) }
  }
  const view = observationView(data, error)
  if (!view.visible) return null
  return <div className="ws-observation" aria-label="Automatic observation">
    <button onClick={() => setOpen(!open)} aria-expanded={open}>{view.label}</button>
    <span>{view.summary}</span>
    {data?.latest_session ? <a href={`/sessions?session=${encodeURIComponent(data.latest_session)}#/trace`}>Open latest recorded session</a> : null}
    {data?.enabled ? <button onClick={() => action(data.state === 'paused' ? 'resume' : 'pause')}>{data.state === 'paused' ? 'Resume observation' : 'Pause observation'}</button> : null}
    {open ? <div className="ws-observation-details"><p>{error || data?.message}</p><p>{data?.boundary || 'No automatic capture is established by the verification results.'}</p><p>Project: {data?.project || 'not configured'} · Source: {data?.source_root || 'not configured'}</p><p>{data?.events_written || 0} new recorded events since this backend started. {data?.sessions?.length || 0} matching sessions. Closing the browser leaves the foreground backend observing. Stopping the backend stops observation; the coding agent continues.</p>{data?.last_observed_at ? <p>Last completed read: {new Date(data.last_observed_at * 1000).toLocaleString()}</p> : null}</div> : null}
  </div>
}
