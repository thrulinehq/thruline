import React from 'react'

const NAMES = { 'osv-scanner': 'selected npm dependency upgrade', semgrep: 'selected Python rule edit', opengrep: 'selected Opengrep Python rule edit', clippy: 'Rust return expression', knip: 'unused JavaScript export', openrewrite: 'selected native recipe' }
const LABELS = {
  previewed: 'Native patch ready to inspect',
  verified_for_named_checks: 'Named independent checks passed',
  applied_verified_patch: 'Verified native patch applied',
  verification_failed: 'Independent checks failed',
  rejected: 'Patch rejected', unavailable: 'Native repair setup needed',
  failed: 'Native repair attempt failed', cancelled: 'Native repair cancelled',
  timedout: 'Native repair timed out', resourcelimit: 'Native repair exceeded its budget',
}

export function NativeRepair({ sessionId, recipe, onRefresh }) {
  const [busy, setBusy] = React.useState('')
  const [repairs, setRepairs] = React.useState([])
  const [current, setCurrent] = React.useState(null)
  const [error, setError] = React.useState('')
  const active = React.useRef('')
  active.current = `${sessionId}:${recipe}`
  React.useEffect(() => {
    active.current = `${sessionId}:${recipe}`
    setRepairs([]); setCurrent(null); setError(''); setBusy('')
    return () => { active.current = '' }
  }, [sessionId, recipe])
  async function act(action, digest = '') {
    const context = active.current
    if (action !== 'cancel') setBusy(action)
    setError('')
    try {
      const response = await fetch('/api/integrations/action', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session: sessionId, recipe, digest, action: action === 'cancel' ? action : `native-repair-${action}` }),
      })
      const data = await response.json()
      if (active.current !== context) return
      if (!response.ok) throw new Error(data.error || 'The native repair did not finish. Earlier evidence remains available.')
      if (action === 'list') { setRepairs(data.repairs || []); setCurrent(data.repairs?.[0] || null) }
      else if (action === 'cancel') setError(data.next || 'Cancellation requested. Waiting for the terminal receipt.')
      else {
        setCurrent(data)
        if (data.digest) setRepairs(previous => [data, ...previous.filter(row => row.digest !== data.digest)])
        if (action === 'apply') onRefresh?.()
      }
    } catch (failure) { if (active.current === context) setError(failure.message) }
    finally { if (active.current === context && action !== 'cancel') setBusy('') }
  }
  if (!NAMES[recipe]) return null
  return <section className="ws-integration-detail" aria-label={`Verified ${recipe} repair`}>
    <h3>Repair a {NAMES[recipe]}</h3>
    <p>Capture the original check and preview the supported native edit in isolation. The protected behavior checks and original detector must pass before applying the selected patch.</p>
    {recipe === 'osv-scanner' ? <p>Experimental dependency guidance uses the selected advisory and package metadata snapshot. Review the lockfile upgrade and independent consumer checks before applying it.</p> : null}
    <div className="ws-hq-inline-actions">
      <button disabled={!!busy} onClick={() => act('preview')}>Preview native repair</button>
      <button disabled={!!busy} onClick={() => act('list')}>Load saved native repairs</button>
    </div>
    {busy ? <p role="status">{busy === 'verify' ? 'Checking the unchanged oracle against both versions and rerunning the original detector…' : busy === 'apply' ? 'Checking current source and applying the verified patch…' : busy === 'preview' ? 'Capturing the native finding and staging its supported edit…' : 'Loading preserved attempts…'} <button onClick={() => act('cancel')}>Cancel native repair</button></p> : null}
    {error ? <p role="alert">{error}</p> : null}
    {current ? <article aria-label="Current native repair attempt">
      <h4>{LABELS[current.state] || current.state || 'Native repair evidence'}</h4>
      {recipe === 'osv-scanner' && current.original_detector?.native?.data_scope ? <p>Advisory scope: {current.original_detector.native.data_scope}</p> : null}
      {current.error ? <p role="alert">{current.error}</p> : null}
      {current.changed_files?.length ? <p>Selected changes: {current.changed_files.join(', ')}</p> : null}
      {current.diff ? <details open><summary>Actual native patch</summary><pre>{current.diff}</pre></details> : null}
      {current.verification ? <details open><summary>Independent behavior and detector results</summary><p>Original detector: {current.verification.detector_cleared ? 'clear on the changed stage' : 'not cleared'}.</p><pre>{JSON.stringify(current.verification, null, 2)}</pre></details> : null}
      {current.state === 'previewed' ? <button disabled={!!busy} onClick={() => act('verify', current.digest)}>Verify native patch</button> : null}
      {current.state === 'verified_for_named_checks' ? <button disabled={!!busy} onClick={() => act('apply', current.digest)}>Apply verified native patch</button> : null}
      <details><summary>Scope, identities, native evidence and history</summary><pre>{JSON.stringify(current, null, 2)}</pre></details>
    </article> : null}
    {repairs.length ? <details><summary>Preserved native repairs ({repairs.length})</summary>{repairs.map(row => <p key={row.digest || row.attempt || row.transport?.attempt}><button disabled={!!busy} onClick={() => setCurrent(row)}>{LABELS[row.state] || row.state} · {row.digest?.slice(0, 12) || 'Setup attempt'}</button></p>)}</details> : null}
  </section>
}
