import React from 'react'
import { AI_REVIEW_SCOPE_NOTE } from './ai-review-ui.js'

export function ReviewOrchestrator({ sessionId, aiReview = false, onSelectProject, onRefresh }) {
  const [projects, setProjects] = React.useState([])
  const [mode, setMode] = React.useState('full')
  const [plan, setPlan] = React.useState(null)
  const [history, setHistory] = React.useState([])
  const [selected, setSelected] = React.useState('')
  const [busy, setBusy] = React.useState('')
  const [error, setError] = React.useState('')
  const [message, setMessage] = React.useState('')
  const [exported, setExported] = React.useState(null)
  const mounted = React.useRef(true)
  const continueQueue = React.useRef(false)
  const currentSession = React.useRef(sessionId)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false; continueQueue.current = false } }, [])
  React.useEffect(() => {
    currentSession.current = sessionId; continueQueue.current = false
    setPlan(null); setHistory([]); setSelected(''); setExported(null); setError(''); setMode('full')
    let cancelled = false
    fetch('/api/review-projects').then(async response => {
      const data = await response.json()
      if (!response.ok) throw new Error(data.error || 'The registered projects could not be read.')
      if (!cancelled) setProjects(data.projects || [])
    }).catch(e => { if (!cancelled) setError(e.message) })
    readHistory(sessionId).catch(e => { if (!cancelled) setError(e.message) })
    return () => { cancelled = true }
  }, [sessionId])
  React.useEffect(() => {
    if (busy !== 'running') return undefined
    const timer = setInterval(() => { readHistory(sessionId).catch(() => {}) }, 1500)
    return () => clearInterval(timer)
  }, [busy, sessionId])

  async function request(action, digest = '', session = sessionId) {
    const response = await fetch('/api/integrations/action', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ session, action: `review-${action}`, recipe: mode, digest }) })
    const data = await response.json()
    if (!response.ok) throw new Error(data.error || 'This review action did not complete. Refresh the preserved history before retrying.')
    return data
  }
  async function readHistory(session = sessionId) {
    const data = await request('status', '', session)
    if (!mounted.current || currentSession.current !== session) return
    setHistory(data.reviews || [])
    if (data.integrity_errors?.length) setError(`Some review history failed integrity checks: ${data.integrity_errors.map(e => `${e.review}: ${e.error}`).join('; ')}`)
  }
  function updateJob(job) {
    setHistory(rows => [...rows.filter(row => row.id !== job.id), job])
    setSelected(job.id)
  }
  async function action(name, digest = '') {
    setError(''); setMessage(''); setBusy(name)
    const session = sessionId
    try {
      const data = await request(name, digest, session)
      if (!mounted.current || currentSession.current !== session) return
      if (name === 'preview') setPlan(data)
      else if (name === 'export') {
        setExported(data)
        setMessage(data.manifest?.complete ? 'The archive contains the review transitions, actual receipts and native attachments, including failed and repair attempts.' : 'The archive was created with explicit missing-attachment gaps. It is an incomplete export.')
      } else {
        updateJob(data); setPlan(null); onRefresh?.()
        setMessage(name === 'accept' ? 'All displayed checks were accepted together. The queue is ready to run.' : 'The interrupted or failed attempts remain in history; queued checks can resume.')
      }
    } catch (e) { if (mounted.current && currentSession.current === session) setError(e.message) }
    finally { if (mounted.current && currentSession.current === session) setBusy('') }
  }
  async function runQueue(job) {
    const session = sessionId
    continueQueue.current = true; setBusy('running'); setError(''); setMessage('')
    try {
      let result = job
      while (continueQueue.current && result.checks.some(check => check.state === 'queued')) {
        result = await request('step', result.id, session)
        if (!mounted.current || currentSession.current !== session) return
        updateJob(result); onRefresh?.()
        if (result.checks.some(check => check.state === 'cancelled' || check.state === 'running')) continueQueue.current = false
      }
      if (mounted.current && currentSession.current === session) setMessage(continueQueue.current ? 'The selected queue has ended. Review each captured outcome and the unresolved scope gaps.' : 'The review is paused. Preserved queued checks can resume after the running check ends.')
    } catch (e) {
      if (mounted.current && currentSession.current === session) { setError(e.message); await readHistory(session).catch(() => {}) }
    } finally {
      continueQueue.current = false
      if (mounted.current && currentSession.current === session) setBusy('')
    }
  }
  async function cancel() {
    continueQueue.current = false
    try {
      const response = await fetch('/api/integrations/action', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ session: sessionId, action: 'cancel', recipe: mode, digest: '' }) })
      const data = await response.json()
      if (!response.ok) throw new Error(data.error || 'Cancellation could not be requested.')
      setMessage('Cancellation requested. The running check must produce a terminal receipt; the remaining queue stays preserved.')
    } catch (e) { setError(e.message) }
  }
  const registered = projects.find(project => project.session === sessionId)
  const job = history.find(row => row.id === selected) || history.at(-1)
  const running = busy === 'running' || job?.queue_state === 'running'
  return <section className="ws-guided-checks" aria-label="Full and change review">
    <h3>Review a project</h3>
    <p>Discover the actual source and available checks, accept the complete plan, then follow one native check at a time. Missing setup and unsupported scope remain visible throughout.</p>
    <label>Project <select aria-label="Review project" value={sessionId} disabled={!!busy} onChange={event => onSelectProject?.(event.target.value)}>
      {!projects.some(project => project.session === sessionId) ? <option value={sessionId}>Current recorded project</option> : null}
      {projects.map(project => <option value={project.session} key={project.session}>{project.label}</option>)}
    </select></label>
    {registered ? <p>{registered.root}</p> : null}
    <label>Review scope <select aria-label="Review scope" value={mode} disabled={!!busy} onChange={event => { setMode(event.target.value); setPlan(null) }}>
      <option value="full">Full repository review</option><option value="change" disabled={!registered?.change_review_available}>Change review against registered base</option>
    </select></label>
    {/* AI-REVIEW-OUT-OF-LAUNCH: "Change review against registered base" is "Review my change"; without AI review it is the deterministic checks only. */}
    {mode === 'change' ? <p>Registered base: {registered?.base_commit}. Full-context scanners retain their accepted scope{aiReview ? AI_REVIEW_SCOPE_NOTE : ''}.</p> : null}
    <div className="ws-hq-inline-actions"><button disabled={!!busy} onClick={() => action('preview')}>Discover source and preview review</button><button disabled={!!busy} onClick={() => readHistory().catch(e => setError(e.message))}>Refresh review history</button></div>
    {plan ? <article className="ws-integration-detail" aria-label="Complete review plan">
      <h4>{plan.mode === 'change' ? 'Change review plan' : 'Full repository review plan'}</h4>
      <Inventory inventory={plan.inventory} />
      <p>{plan.checks.length} native checks are ready. {plan.gaps.length} applicable capabilities remain uncovered.</p>
      {plan.checks.map(check => <details key={check.recipe}><summary>{check.recipe} · ready to accept</summary><p>{check.review.plan.criterion.requirement}</p><p>{check.review.plan.criterion.success}</p><p>Execution: {check.execution_scope}. Budget: {check.review.plan.timeout_seconds}s, {check.review.plan.memory_mb} MiB.</p><pre>{JSON.stringify(check.review.plan, null, 2)}</pre></details>)}
      <Gaps gaps={plan.gaps} />
      <details><summary>Capabilities not applicable to discovered source · {plan.not_applicable.length}</summary>{plan.not_applicable.map(row => <p key={row.recipe}>{row.recipe}: {row.reason}. {row.qualification}.</p>)}</details>
      <p>All displayed native plans are accepted as one requirement revision. A finished queue does not establish that the whole product meets its requirements.</p>
      <button disabled={!!busy || !plan.checks.length} onClick={() => action('accept', plan.digest)}>Accept complete review plan</button> <button disabled={!!busy} onClick={() => setPlan(null)}>Close plan</button>
    </article> : null}
    {history.length ? <label>Recorded review <select aria-label="Recorded review" disabled={!!busy} value={job?.id || ''} onChange={event => { setSelected(event.target.value); setExported(null) }}>{history.map(row => <option key={row.id} value={row.id}>{row.created_at} · {row.mode} · {row.queue_state}</option>)}</select></label> : null}
    {job ? <article className="ws-integration-detail" aria-label="Review queue">
      <h4>{job.mode} review · {job.queue_state.replaceAll('_', ' ')}</h4>
      <p>Review {job.id}. All checks share accepted revision {job.accepted_revision}.</p>
      <Gaps gaps={job.plan.gaps} />
      {job.interrupted_process ? <p role="alert">The process recorded as running has ended without a terminal queue event. Its earlier transition remains preserved. Retry records this interruption before resuming.</p> : null}
      <ol>{job.checks.map(check => <li key={check.recipe}><strong>{check.recipe}</strong> · {check.state.replaceAll('_', ' ')}{check.attempts.length ? <details><summary>{check.attempts.length} preserved attempt(s)</summary>{check.attempts.map((attempt, index) => <div key={index}><p>{attempt.completed_at || attempt.at} · {attempt.outcome}</p>{attempt.error ? <p role="alert">{attempt.error}</p> : null}{attempt.assessment ? <p>{attempt.assessment.word}: {attempt.assessment.explanation}</p> : null}{attempt.binding_id ? <p>Native receipt: <code>{attempt.binding_id}</code></p> : null}<details><summary>Exact attempt evidence</summary><pre>{JSON.stringify(attempt, null, 2)}</pre></details></div>)}</details> : null}</li>)}</ol>
      <div className="ws-hq-inline-actions">
        <button disabled={!!busy || running || !job.checks.some(check => check.state === 'queued')} onClick={() => runQueue(job)}>Run or resume accepted review</button>
        <button disabled={!!busy || (running && !job.interrupted_process) || !job.checks.some(check => !['queued', 'completed'].includes(check.state))} onClick={() => action('retry', job.id)}>Retry failed or interrupted checks</button>
        <button disabled={!!busy} onClick={() => action('export', job.id)}>Export review and native evidence</button>
        {running && !job.interrupted_process ? <button onClick={cancel}>Cancel running check and pause queue</button> : null}
      </div>
      <Triage triage={job.triage} />
    </article> : null}
    {exported ? <p><a href={`/api/reviews/export?session=${encodeURIComponent(sessionId)}&export=${exported.export_id}`} download={`thruline-review-${exported.export_id}.zip`}>Download preserved evidence archive</a> · SHA-256 {exported.archive_sha256}. {exported.manifest?.missing_attachments?.length || 0} missing attachments.</p> : null}
    {busy ? <p role="status">{busy === 'running' ? 'The current native check is running. The next check starts only after its receipt is captured.' : `${busy} review…`}</p> : null}
    {message ? <p role="status">{message}</p> : null}{error ? <p className="ws-integration-error" role="alert">{error}</p> : null}
  </section>
}

function Inventory({ inventory }) {
  return <div><p>{inventory.file_count} source files · {inventory.bytes} bytes · HEAD {inventory.head}</p><p>{Object.entries(inventory.languages).map(([name, count]) => `${name}: ${count}`).join(' · ')}</p><p>Discovered build manifests: {inventory.build_manifests.join(', ') || 'none'}</p><p>Declared inventory exclusions: {inventory.declared_exclusions.join(', ')}.</p><p>{inventory.selection}</p>{inventory.ignored_scope ? <p>{inventory.ignored_scope.entry_count} Git-ignored entries, including {inventory.ignored_scope.collapsed_directory_count} collapsed directories. Files inside those directories were not counted or traversed.</p> : null}{inventory.change ? <details><summary>Changed source paths · {inventory.change.paths.length}</summary><p>Base {inventory.change.base_commit}</p>{inventory.change.paths.map(path => <p key={path}>{path}</p>)}</details> : null}<details><summary>Exact file identities, omissions and ignore policies</summary><pre>{JSON.stringify({ files: inventory.files, omitted: inventory.omitted, ignored_scope: inventory.ignored_scope, ignore_files: inventory.ignore_files }, null, 2)}</pre></details></div>
}
function Gaps({ gaps }) { return gaps?.length ? <details open><summary>Unresolved coverage gaps · {gaps.length}</summary>{gaps.map((gap, index) => <p key={index}>{gap.recipe ? `${gap.recipe}: ` : ''}{gap.reason}</p>)}</details> : <p>No setup gap was found for this selected inventory. Each native receipt still limits its own coverage.</p> }
function Triage({ triage }) {
  if (!triage) return null
  return <section aria-label="Grouped review findings"><h4>Source-linked triage</h4>{triage.error ? <p role="alert">{triage.error}</p> : null}<p>{triage.policy}</p><p>{triage.groups?.length || 0} groups from {triage.native_observation_count || 0} retained native observations.</p>{triage.groups?.map((group, index) => <details key={index}><summary>{group.rule} · {group.path}{group.line ? `:${group.line}` : ''} · {group.message}</summary><p>Engine: {group.engine}. {group.sources.length} native observation(s), no independent corroboration inferred.</p>{group.sources.map((source, sourceIndex) => <p key={sourceIndex}>{source.recipe} · receipt {source.binding_id} · native pointer {source.native_observation.pointer}</p>)}</details>)}</section>
}
