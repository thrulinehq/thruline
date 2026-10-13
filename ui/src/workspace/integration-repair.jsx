import React from 'react'

const LABELS = {
  previewed: 'Patch ready to inspect',
  verified_for_named_checks: 'Independent checks passed on this patch',
  applied_verified_patch: 'Verified patch applied',
  verification_failed: 'Independent checks failed',
  rejected: 'Patch rejected',
  unavailable: 'Repair setup needed',
  failed: 'Repair attempt failed',
  cancelled: 'Repair cancelled',
  timedout: 'Repair timed out',
  resourcelimit: 'Repair exceeded its resource budget',
}

export function RuffRepair({ sessionId, configuration, onRefresh }) {
  const [busy, setBusy] = React.useState('')
  const [repairs, setRepairs] = React.useState([])
  const [current, setCurrent] = React.useState(null)
  const [error, setError] = React.useState('')
  const previewLabel = { 'legacy-f401': 'Preview safe import repair', imports: 'Preview import sorting repair', format: 'Preview Python formatting repair', 'notebook-format': 'Preview notebook formatting repair' }[configuration?.mode] || 'Preview configured Python repair'
  const mounted = React.useRef(true)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  async function act(action, digest = '') {
    if (action !== 'cancel') setBusy(action)
    setError('')
    try {
      const response = await fetch('/api/integrations/action', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session: sessionId, action: action === 'cancel' ? action : `repair-${action}`, recipe: 'ruff', digest }),
      })
      const data = await response.json()
      if (!response.ok) throw new Error(data.error || 'The repair did not finish. Its earlier evidence remains available.')
      if (!mounted.current) return
      if (action === 'list' && data.repairs) setRepairs(data.repairs)
      else if (action === 'cancel') setError(data.next || 'Cancellation requested. Wait for the worker to record its terminal state.')
      else {
        setCurrent(data)
        if (data.digest) setRepairs(previous => [data, ...previous.filter(row => row.digest !== data.digest)])
        if (action === 'apply') onRefresh?.()
      }
    } catch (failure) { if (mounted.current) setError(failure.message) }
    finally { if (mounted.current && action !== 'cancel') setBusy('') }
  }
  return <section className="ws-integration-detail" aria-label="Verified Python repair">
    <h3>Repair selected Python source</h3>
    <p>Preview the configured Ruff repair in an isolated worktree. The default supports safe F401 imports; explicit setup can select import sorting, formatting or plain-Python notebook formatting. The unchanged independent behavior checks must pass before application.</p>
    {configuration?.mode ? <p>Configured native mode: {configuration.mode}. {Array.isArray(configuration.files) ? configuration.files.join(', ') : ''} This setup summary is revalidated before execution.</p> : null}
    <div className="ws-hq-inline-actions">
      <button disabled={!!busy} onClick={() => act('preview')}>{previewLabel}</button>
      <button disabled={!!busy} onClick={() => act('list')}>Load saved repairs</button>
    </div>
    {busy ? <p role="status">{busy === 'preview' ? 'Capturing the original diagnostic and staging a patch…' : busy === 'verify' ? 'Running the unchanged behavior oracle and original check against the patch…' : busy === 'apply' ? 'Checking the patch base and applying the verified diff…' : 'Loading preserved repair attempts…'} <button onClick={() => act('cancel')}>Cancel repair</button></p> : null}
    {error ? <p role="alert">{error}</p> : null}
    {current ? <article aria-label="Current repair attempt">
      <h4>{LABELS[current.state] || current.state || 'Repair evidence'}</h4>
      {current.error ? <p role="alert">{current.error}</p> : null}
      {current.mode ? <p>Selected repair: {current.mode}; native check: {current.selected_rule}.</p> : null}
      {current.changed_files?.length ? <p>Changed source: {current.changed_files.join(', ')}</p> : null}
      {current.diff ? <details open><summary>Actual proposed diff</summary><pre>{current.diff}</pre></details> : null}
      {current.withheld?.length ? <details><summary>{current.withheld.length} finding(s) withheld from this repair</summary><p>These findings require a different remedy or assumptions beyond the supported safe import fix.</p><pre>{JSON.stringify(current.withheld, null, 2)}</pre></details> : null}
      {current.verification ? <details open><summary>Independent verification</summary><p>Selected native check ({current.verification.selected_rule || 'F401'}): {(current.verification.detector_cleared ?? current.verification.detector_cleared_F401) ? 'clear on the changed stage' : 'still failing'}. Other findings remain separately visible.</p><pre>{JSON.stringify(current.verification, null, 2)}</pre></details> : null}
      {current.state === 'previewed' ? <button disabled={!!busy} onClick={() => act('verify', current.digest)}>Run independent checks</button> : null}
      {current.state === 'verified_for_named_checks' ? <button disabled={!!busy} onClick={() => act('apply', current.digest)}>Apply verified patch</button> : null}
      <details><summary>Receipt, source identities and attempt history</summary><pre>{JSON.stringify(current, null, 2)}</pre></details>
    </article> : null}
    {repairs.length ? <details><summary>Preserved repairs ({repairs.length})</summary>{repairs.map(row => <p key={row.digest || row.attempt || row.transport?.attempt}><button disabled={!!busy} onClick={() => setCurrent(row)}>{LABELS[row.state] || row.state} · {row.digest?.slice(0, 12) || 'Setup attempt'}</button></p>)}</details> : null}
  </section>
}
