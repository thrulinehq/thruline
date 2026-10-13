import React from 'react'
import { AI_REVIEW_NAMES, AI_REVIEW_SUGGESTION_SUMMARY } from './ai-review-ui.js'

const NAMES = { aider: 'Aider', 'openhands-sdk': 'OpenHands', ...AI_REVIEW_NAMES, 'mini-swe-agent': 'mini-swe-agent' }
const STATES = {
  previewed: 'Native proposal ready to inspect',
  verified_for_named_checks: 'The frozen independent checks passed',
  verification_failed: 'Independent checks did not establish this repair',
  resume_incomplete: 'Native continuation did not complete the unchanged-source verification task',
  applied_verified_patch: 'Verified source bytes applied',
  rejected: 'Proposal action rejected',
  unavailable: 'Independent repair setup is missing',
  failed: 'Native repair action failed',
  cancelled: 'Native repair action cancelled',
  timedout: 'Native repair action timed out',
  resourcelimit: 'Native repair action exceeded its resource budget',
}

export function AgentRepair({ sessionId, recipe, receipts = [], onRefresh }) {
  const [selected, setSelected] = React.useState('')
  const [current, setCurrent] = React.useState(null)
  const [history, setHistory] = React.useState([])
  const [busy, setBusy] = React.useState('')
  const [error, setError] = React.useState('')
  const mounted = React.useRef(true)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  React.useEffect(() => { setSelected(''); setCurrent(null); setHistory([]); setError('') }, [sessionId, recipe])
  const candidates = receipts.filter(row => row.record?.plan?.pack === recipe && row.record?.report?.facts?.advisory_only)
  const binding = selected || candidates[0]?.binding_id || ''

  async function act(action, digest = '') {
    if (action !== 'cancel') setBusy(action)
    setError('')
    try {
      const response = await fetch('/api/integrations/action', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session: sessionId, recipe, action: action === 'cancel' ? 'cancel' : `agent-repair-${action}`, digest }),
      })
      const data = await response.json()
      if (!response.ok) throw new Error(data.error || 'This action did not finish. Its prior evidence remains available.')
      if (!mounted.current) return
      if (action === 'cancel') setError(data.next || 'Cancellation requested. Wait for the recorded terminal state.')
      else if (action === 'list') setHistory(data.repairs || [])
      else {
        setCurrent(data)
        setHistory(previous => [data, ...previous])
        if (action === 'apply') onRefresh?.()
      }
    } catch (failure) { if (mounted.current) setError(failure.message) }
    finally { if (mounted.current && action !== 'cancel') setBusy('') }
  }

  if (!NAMES[recipe]) return null
  return <section className="ws-integration-detail" aria-label={`${NAMES[recipe]} proposal actions`}>
    <h3>{NAMES[recipe]} proposal and independent checks</h3>
    <p>Choose a captured native receipt from this project. Inspect its exact source changes, run the unchanged behavior checks, and apply only the verified bytes. Native submission and model summaries remain advisory.</p>
    {candidates.length ? <label>Native source receipt <select value={binding} disabled={!!busy} onChange={event => setSelected(event.target.value)}>{candidates.map(row => <option key={row.binding_id} value={row.binding_id}>{row.binding_id.slice(0, 12)} · {row.assessment?.state || 'unknown'} · {row.record.report.facts.agent_termination || 'native status unavailable'}</option>)}</select></label> : <p>No captured native receipt is available here. Run this project's accepted local agent check to create one.</p>}
    <div className="ws-hq-inline-actions">
      <button disabled={!!busy || !binding} onClick={() => act('preview', binding)}>Preview native proposal</button>
      {recipe === 'openhands-sdk' ? <button disabled={!!busy || !binding} onClick={() => act('resume', binding)}>Resume saved conversation</button> : null}
      <button disabled={!!busy} onClick={() => act('list')}>Load preserved agent repairs</button>
    </div>
    {busy ? <p role="status">{busy === 'resume' ? 'Loading the actual saved conversation and running its tools in an isolated stage…' : busy === 'verify' ? 'Checking the original failure and the proposal with the protected behavior oracle…' : busy === 'apply' ? 'Rechecking the source base and applying the verified bytes…' : 'Loading the native proposal and its evidence…'} <button onClick={() => act('cancel')}>Cancel agent repair</button></p> : null}
    {error ? <p role="alert">{error}</p> : null}
    {current ? <article aria-label="Current agent repair">
      <h4>{STATES[current.state] || current.state || 'Native repair evidence'}</h4>
      {current.error ? <p role="alert">{current.error}</p> : null}
      {current.changed_files?.length ? <p>Changed source: {current.changed_files.join(', ')}</p> : <p>No changed source has been established for this attempt.</p>}
      {current.resume ? <p>Conversation {current.resume.conversation_id} continued from persisted native events. Native exit: {current.resume.exit_code}. Prior artifacts unchanged: {current.resume.prior_artifacts_unchanged ? 'confirmed' : 'not established'}.</p> : null}
      {current.native_suggestion && AI_REVIEW_SUGGESTION_SUMMARY ? <details><summary>{AI_REVIEW_SUGGESTION_SUMMARY}</summary><pre>{JSON.stringify(current.native_suggestion, null, 2)}</pre></details> : null}
      {current.diff ? <details open><summary>Exact source diff</summary><pre>{current.diff}</pre></details> : null}
      {current.verification ? <section aria-label="Independent agent repair verification"><h5>Frozen behavior checks</h5><p>Original: {current.verification.before?.report?.passed ?? 'unknown'} / {current.verification.before?.report?.total ?? 'unknown'}. Proposal: {current.verification.after?.report?.passed ?? 'unknown'} / {current.verification.after?.report?.total ?? 'unknown'}. Regressions: {current.verification.regressions?.join(', ') || 'none reported'}.</p><details><summary>Actual oracle commands and case results</summary><pre>{JSON.stringify(current.verification, null, 2)}</pre></details></section> : null}
      {current.state === 'previewed' ? <button disabled={!!busy} onClick={() => act('verify', current.digest)}>Verify native proposal independently</button> : null}
      {current.state === 'verified_for_named_checks' ? <button disabled={!!busy} onClick={() => act('apply', current.digest)}>Apply independently verified proposal</button> : null}
      <details><summary>Source binding, native artifacts and attempt history</summary><pre>{JSON.stringify(current, null, 2)}</pre></details>
    </article> : null}
    {history.length ? <details><summary>Preserved agent repair outcomes ({history.length})</summary>{history.map((row, index) => <p key={`${row.binding_id || row.digest || row.attempt}-${index}`}><button disabled={!!busy} onClick={() => setCurrent(row)}>{STATES[row.state] || row.state} · {row.digest?.slice(0, 12) || 'Setup attempt'}</button></p>)}</details> : null}
  </section>
}
