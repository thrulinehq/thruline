import React from 'react'
import { buildReview } from './review-model.js'
import { HQ_COPY } from './hq-copy.js'
import { readLocalVerification } from './local-verification-model.js'
import { integrationView } from './integration-model.js'
import { CHECK_GROUPS, CHECK_STATES, NATIVE_CHECKS, checkState, toolRows, recordedRows, simpleResults, resultsOverview, resultState, resultReport, checkName, applicableChecks } from './checks-panel-model.js'
import { C } from './tokens.js'
import { padUnderSticky } from './sticky-padding.js'

const LocalVerification = React.lazy(() => import('./local-verification.jsx'))
const IntegrationResult = React.lazy(() => import('./integrations.jsx').then(m => ({ default: m.IntegrationResult })))
const text = value => typeof value === 'string' ? value : JSON.stringify(value, null, 2)

export async function verificationAction(body) {
  const response = await fetch('/api/verification', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
  const value = await response.json()
  if (!response.ok) throw new Error(value.detail || value.error || `Check request failed (${response.status})`)
  return value
}

/**
 * The local verification reading, read only while the Checks surface is the one being read.
 *
 * `hq.jsx` mounts this panel in every mode so a running check sequence and the reader's chosen ordering
 * survive a tab round trip. That mount used to read and poll `/api/verification` from behind the Verdict
 * and Trace tabs (commit G ruling, "ChecksPanel polling under Verdict"; STEP0-AUDIT corrections table row
 * "narrator-path c1"). `active` gates the reading, not the mount: the panel keeps its state, and an
 * explicit `refresh()` — the button, and the run sequence's own waiting loop — still works either way.
 */
function useCheckReading(session, active) {
  const [data, setData] = React.useState(session.localData || null)
  const [error, setError] = React.useState(session.localData?.readError || '')
  const [refreshing, setRefreshing] = React.useState(false)
  const mounted = React.useRef(false)
  const pending = React.useRef(null)
  const refresh = React.useCallback(() => {
    if (pending.current) return pending.current
    setRefreshing(true)
    pending.current = readLocalVerification().then(value => {
      if (mounted.current) { setData(value); setError(''); if (value.enabled) window.dispatchEvent(new CustomEvent('thruline-verification-update', { detail: value })) }
      return value
    }).catch(e => { if (mounted.current) setError(e.message); throw e }).finally(() => { pending.current = null; if (mounted.current) setRefreshing(false) })
    return pending.current
  }, [])
  // Tied to the mount, not to the surface being active: a check sequence started here goes on refreshing
  // and reporting while the reader looks at another tab, and only an unmount stops it.
  React.useEffect(() => {
    mounted.current = true
    const update = e => { setData(e.detail); setError(e.detail.readError || '') }
    window.addEventListener('thruline-verification-update', update)
    return () => { mounted.current = false; window.removeEventListener('thruline-verification-update', update) }
  }, [])
  React.useEffect(() => {
    if (!active) return undefined
    refresh().catch(() => {})
    // The project workspace already has a shared poller. A captured-session
    // workspace needs one here to observe an explicitly configured local job,
    // and only while this surface is the one being read.
    const timer = session.localVerification ? null : setInterval(() => refresh().catch(() => {}), 3000)
    return () => clearInterval(timer)
  }, [active, refresh, session.localVerification])
  React.useEffect(() => { if (session.localData) { setData(session.localData); setError(session.localData.readError || '') } }, [session.localData])
  return { data, error, refreshing, refresh }
}

/** The tool evidence, read when the Checks surface opens and when it is refreshed — never from behind a tab. */
function useToolReading(session, active) {
  const [reading, setReading] = React.useState({ data: null, error: '', loading: true })
  const [revision, setRevision] = React.useState(0)
  const id = session.id || session.key
  React.useEffect(() => {
    // Before the reset below: an inactive surface neither reads nor discards what it last read.
    if (!active) return undefined
    const controller = new AbortController()
    setReading({ data: null, error: '', loading: true })
    fetch(`/api/integrations?session=${encodeURIComponent(id)}`, { cache: 'no-store', signal: controller.signal })
      .then(async r => { const d = await r.json(); if (!r.ok) throw new Error(d.error || 'Tool evidence is unavailable.'); return integrationView(d, id) })
      .then(data => { if (!controller.signal.aborted) setReading({ data, error: data.results_error || '', loading: false }) })
      .catch(e => { if (!controller.signal.aborted) setReading({ data: null, error: e.message, loading: false }) })
    return () => controller.abort()
  }, [id, revision, active])
  const refresh = React.useCallback(() => setRevision(n => n + 1), [])
  return { ...reading, refresh }
}

function Badge({ status }) {
  const s = checkState(status)
  return <span className="ws-checks-badge" style={{ color: s.color }}><span aria-hidden="true">{s.glyph}</span>{s.label}</span>
}

function CheckRow({ row, onReceipt, onReview, onInspect }) {
  const [open, setOpen] = React.useState(false)
  const s = checkState(row.status)
  const observations = row.record?.finish?.pack?.observations || []
  return <article className={`ws-checks-row ws-checks-row-${row.status}`}>
    <button className="ws-checks-row-toggle" aria-expanded={open} onClick={() => setOpen(v => !v)}>
      <span key={row.status} className={row.status === 'running' ? 'ws-checks-spinner' : 'ws-checks-glyph'} style={{ color: s.color }} aria-hidden="true">{row.status === 'running' ? '' : s.glyph}</span>
      <span className="ws-checks-row-copy"><strong>{row.status === 'running' ? row.running : row.title}</strong><span>{row.kind === 'native' ? `${row.check.id} · ${row.description}` : row.description}</span></span>
      <span className="ws-checks-row-meta"><span style={{ color: s.color }}>{s.label}</span><code>{row.id}</code></span>
    </button>
    {open ? <div className="ws-checks-row-detail">
      <h4>What is checked</h4><p>{row.criteria?.map(c => c.description).join(' ') || row.title}</p>
      <h4>What the record shows</h4><p>{row.description}</p>
      {row.kind === 'native' ? <p className="ws-checks-muted">This row: {s.label}. The run also evaluates {row.result?.catalog_results?.length || applicableChecks(row.check).length} evidence conditions, each with its own result.</p> : null}
      {observations.some(o => o.expected != null) ? <div className="ws-checks-table-wrap"><table><thead><tr><th>Example</th><th>Expected</th><th>Observed</th></tr></thead><tbody>{observations.filter(o => o.expected != null).map(o => <tr key={o.id}><td>{o.description}</td><td><code>{text(o.expected)}</code></td><td className={o.outcome === 'failed' ? 'ws-checks-mismatch' : ''}><code>{text(o.observed)}</code></td></tr>)}</tbody></table></div> : null}
      <h4>Scope of this result</h4><p>{row.scope || 'The recorded evidence does not identify a complete scope.'}</p>
      <p className="ws-checks-muted">{row.record?.finish?.pack?.limitations?.[0] || (row.kind === 'record' ? 'A captured command outcome alone does not establish an accepted requirement.' : 'This result covers the accepted condition and examined inputs. It does not establish correctness of the whole project.')}</p>
      {row.result?.next_action ? <><h4>What to do next</h4><p>{row.result.next_action}</p></> : null}
      <div className="ws-checks-actions">
        {row.kind === 'native' ? <button onClick={() => onReview(row.check.id)} disabled={row.status === 'running'}>{row.receipt ? 'Review and rerun' : 'Review and run'}</button> : null}
        {row.receipt ? <button onClick={() => onReceipt(row)}>Open receipt</button> : null}
        {row.node ? <button onClick={() => onInspect(row.node)}>Locate in graph</button> : null}
      </div>
      {row.receipt ? <code className="ws-checks-reference">{row.receipt}</code> : null}
    </div> : null}
  </article>
}

function SimpleResult({ row, onReceipt, onReview, onInspect }) {
  const state = resultState(row.status)
  const observations = (row.record?.finish?.pack?.observations || []).filter(o => Object.hasOwn(o, 'expected'))
  return <article className={`ws-checks-result ws-checks-row-${row.status}`} aria-label={`${row.title}: ${state.label}`}>
    <header><h3>{row.title}</h3><span className="ws-checks-badge" style={{ color: state.color }}><span key={row.status} className={row.status === 'running' ? 'ws-checks-spinner' : 'ws-checks-result-glyph'} aria-hidden="true">{row.status === 'running' ? '' : state.glyph}</span>{state.label}</span></header>
    <p className="ws-checks-result-explanation">{row.description}</p>
    <div className="ws-checks-actions">
      {row.receipt ? <button onClick={() => onReceipt(row)}>View evidence</button> : null}
      {row.kind === 'native' && row.status !== 'running' && row.status !== 'pass' ? <button onClick={() => onReview(row.check.id)}>{row.receipt ? 'Review and rerun' : 'Review and run'}</button> : null}
      {row.node ? <button onClick={() => onInspect(row.node)}>Locate in graph</button> : null}
    </div>
    <details className="ws-checks-result-details"><summary>Details and technical evidence</summary>
      <h4>Requirement</h4><p>{row.criteria?.map(c => c.description).join(' ') || row.title}</p>
      <h4>Recorded assessment</h4><p>{row.detail}</p>
      {observations.length ? <><p className="ws-checks-muted">Values captured in this receipt{row.status === 'stale' || row.status === 'running' ? ' from the earlier run' : ''}:</p><div className="ws-checks-table-wrap"><table><thead><tr><th>Example</th><th>Expected</th><th>Actual</th></tr></thead><tbody>{observations.map(o => <tr key={o.id}><td>{o.description}</td><td><code>{text(o.expected)}</code></td><td className={o.outcome === 'failed' ? 'ws-checks-mismatch' : ''}><code>{text(o.observed)}</code></td></tr>)}</tbody></table></div></> : null}
      <h4>Scope</h4><p>{row.scope || 'The recorded evidence does not identify a complete scope.'}</p>
      <h4>Next step</h4><p>{row.next}</p>
      {row.result?.next_action ? <p className="ws-checks-muted">From the verifier: {row.result.next_action}</p> : null}
      <p className="ws-checks-muted">{row.record?.finish?.pack?.limitations?.[0] || 'This result covers the examined inputs and conditions. Work outside this scope has not been verified here.'}</p>
      {row.evidence.length ? <div className="ws-checks-technical"><h4>{row.evidence.length} supporting evidence assessments</h4><p className="ws-checks-muted">These explain this one result. A successful command alone does not make its requirement pass.</p>{row.evidence.map(evidence => <CheckRow key={evidence.key} row={evidence} onReceipt={onReceipt} onReview={onReview} onInspect={onInspect}/>)}</div> : null}
      <code className="ws-checks-reference">Check: {row.id}{row.receipt ? ` · Receipt: ${row.receipt}` : ''}</code>
      {row.kind === 'native' && row.status === 'pass' ? <div className="ws-checks-actions"><button onClick={() => onReview(row.check.id)}>Review and rerun</button></div> : null}
    </details>
  </article>
}

function RecordedFinding({ concern, session, onInspect }) {
  return <div className="ws-checks-audit-row"><span aria-hidden="true" style={{ color: concern.tone === 'bad' ? C.coral : C.yellow }}>◇</span><div><strong>{concern.title}</strong><p>{concern.detail}</p>{concern.quote ? <blockquote>{concern.quote}</blockquote> : null}{concern.node && session.nodes?.some(n => n.id === concern.node) ? <button onClick={() => onInspect(concern.node)}>Open source in graph</button> : null}{concern.source ? <details><summary>Source reference</summary><code>{concern.source}</code></details> : null}</div></div>
}

function RecordedReview({ review, session, onInspect }) {
  return <section className="ws-checks-recorded"><h3>From recorded activity</h3><p>{review.summary}</p><p className="ws-checks-muted">These are review findings from this session, not new verification runs.</p>{review.concerns.slice(0, 2).map(concern => <RecordedFinding key={concern.id} concern={concern} session={session} onInspect={onInspect}/>)}<details className="ws-checks-audit"><summary>{review.concerns.length > 2 ? `${review.concerns.length - 2} more findings and reading limits` : 'Reading details and limits'}</summary>{review.concerns.slice(2).map(concern => <RecordedFinding key={concern.id} concern={concern} session={session} onInspect={onInspect}/>)}<p className="ws-checks-muted">{review.counts.events.toLocaleString()} events available in this reading. Activity alone does not establish completion.</p>{review.coverage?.map((line, i) => <p className="ws-checks-muted" key={i}>{line}</p>)}</details></section>
}

function ReceiptDialog({ row, session, data, onClose, onCopy, onInspect }) {
  const dialog = React.useRef(null)
  const head = React.useRef(null)
  React.useEffect(() => { const el = dialog.current; el.showModal(); return () => el.close() }, [])
  /* N13-FOCUS-2411: the header sticks in the dialog; the dialog keeps focus below it. */
  React.useLayoutEffect(() => padUnderSticky(head.current), [])
  return <dialog ref={dialog} className="ws-checks-dialog" aria-labelledby="ws-checks-receipt-title" onCancel={onClose} onClick={e => { if (e.target === dialog.current) onClose() }}>
    <header ref={head}><strong id="ws-checks-receipt-title">Check receipt</strong><code>{row.receipt}</code><button onClick={onClose} aria-label="Close receipt">×</button></header>
    <div className="ws-checks-dialog-content ws-hq-content">
      <React.Suspense fallback={<p role="status">Reading receipt…</p>}>
        {row.kind === 'native' ? <LocalVerification session={{ ...session, localData: data }} initialReceipt={row.receipt} onInspect={onInspect} receiptOnly /> : row.kind === 'tool' ? <IntegrationResult row={row.toolResult} onCopy={onCopy} /> : <><h2>{row.title}</h2><Badge status={row.status}/><p>{row.description}</p><p>{row.scope}</p><pre>{JSON.stringify(row.record, null, 2)}</pre>{row.node ? <button onClick={() => { onClose(); onInspect(row.node) }}>Locate in graph</button> : null}</>}
      </React.Suspense>
    </div>
    <footer><button onClick={() => onCopy(row.receipt, 'the receipt reference')}>Copy receipt reference</button><button onClick={onClose}>Close</button></footer>
  </dialog>
}

function RunPlan({ data, selected, setSelected, onRun, onClose, disabled }) {
  const checks = data.contract?.checks || []
  return <section className="ws-checks-plan" aria-label="Review accepted checks">
    <h3>Review the checks to run</h3>
    <p>One command runs at a time. Each can settle several evidence conditions. These are the commands and requirements already accepted for this project.</p>
    <p className="ws-checks-muted">{data.location?.repo}</p>
    {checks.map(check => <div className="ws-checks-plan-row" key={check.id}>
      <label><input type="checkbox" checked={selected.includes(check.id)} disabled={disabled} onChange={e => setSelected(ids => e.target.checked ? [...ids, check.id] : ids.filter(id => id !== check.id))}/><span><strong>{check.id}</strong><span>{data.contract.criteria.filter(c => c.check_ids.includes(check.id)).map(c => c.description).join(' ')}</span></span><code>{applicableChecks(check).length} conditions</code></label>
      <details><summary>Command, scope and limits</summary><pre>{check.verification?.pack?.kind === 'completion' ? 'Evaluate captured receipts against the accepted criteria.' : JSON.stringify(check.argv)}</pre><p>Working directory: {check.cwd} · Time budget: {check.timeout_seconds}s</p><p>Scope: {check.verification?.scope?.join(', ') || 'Accepted command inputs'}</p><pre>{JSON.stringify(check.verification?.pack || check.verification?.cases || {}, null, 2)}</pre></details>
    </div>)}
    <p className="ws-checks-muted">Local checks use a private source copy with your local user permissions. No hard RAM or network isolation. Unselected checks remain outstanding.</p>
    <div className="ws-checks-actions"><button className="ws-checks-primary" disabled={disabled || !selected.length} onClick={onRun}>Run {selected.length} selected {selected.length === 1 ? 'check' : 'checks'}</button><button onClick={onClose}>Back</button></div>
  </section>
}

export default function ChecksPanel({ session, active, onMode, onInspect, onCopy, selected: graphSelection, reading }) {
  const local = useCheckReading(session, active)
  const tools = useToolReading(session, active)
  const data = local.data
  // A captured session and the configured verification project need an exact
  // identity match. Never borrow another project's green results.
  const matchesProject = session.localVerification || (data?.location?.repo && session.project === data.location.repo)
  const projectData = matchesProject ? data : null
  const review = React.useMemo(() => buildReview(session), [session])
  const [plan, setPlan] = React.useState(false)
  const [selected, setSelected] = React.useState([])
  const [sort, setSort] = React.useState('worst')
  const [filter, setFilter] = React.useState('all')
  const [receipt, setReceipt] = React.useState(null)
  const [message, setMessage] = React.useState('')
  const [runError, setRunError] = React.useState('')
  const [batch, setBatch] = React.useState(null)
  const sequence = React.useRef(null)
  React.useEffect(() => () => { if (sequence.current) sequence.current.stopped = true }, [])
  // Opening this surface re-reads both sources: that is the two reading hooks above, which read when
  // `active` turns true. An explicit pair here would read the same evidence a second time per opening.
  React.useEffect(() => { if (!active) setReceipt(null) }, [active])
  const results = React.useMemo(() => simpleResults(projectData, local.error, [...toolRows(tools.data, tools.error), ...recordedRows(session)]), [projectData, local.error, tools.data, tools.error, session])
  const loading = Boolean(reading || !data && !local.error || tools.loading)
  const summary = resultsOverview(results, { loading, readError: local.error, toolsError: tools.error, enabled: projectData?.enabled })
  const running = projectData?.job?.running
  const checks = projectData?.contract?.checks || []
  const canRun = projectData?.accepted && !projectData.pending_draft_change && !local.error && !running && !batch
  const missing = NATIVE_CHECKS.filter(c => !results.some(r => r.evidence.some(e => e.id === c.id)))
  const visible = results.filter(r => filter === 'all' || (filter === 'attention' ? ['fail', 'blocked', 'unknown', 'stale'].includes(r.status) : r.status === filter))
  const ordered = sort === 'worst' ? [...visible].sort((a, b) => checkState(a.status).rank - checkState(b.status).rank) : visible
  const headNumber = running ? summary.counts.running || 1 : summary.attention || summary.counts.pass || summary.receipts

  function openPlan(id) {
    setRunError(''); setMessage('')
    if (!canRun) { onMode('verify'); return }
    setSelected(id ? [id] : checks.map(c => c.id)); setPlan(true)
  }
  async function runSelected() {
    if (!canRun || !selected.length || sequence.current) return
    const snapshot = { digest: projectData.contract_digest, repo: projectData.location.repo, ids: selected.slice() }
    // Completion accounting runs after its child checks, regardless of how the
    // operator selected the rows. The exact accepted digest is checked each time.
    snapshot.ids.sort((a, b) => Number(checks.find(c => c.id === a)?.verification?.pack?.kind === 'completion') - Number(checks.find(c => c.id === b)?.verification?.pack?.kind === 'completion'))
    const token = { stopped: false }
    sequence.current = token; setPlan(false); setRunError(''); setMessage(''); setBatch({ completed: 0, total: snapshot.ids.length })
    try {
      for (let index = 0; index < snapshot.ids.length && !token.stopped; index++) {
        const current = await local.refresh()
        if (token.stopped) break
        if (current.contract_digest !== snapshot.digest || current.location?.repo !== snapshot.repo || current.pending_draft_change || !current.accepted) throw new Error('The accepted requirements or project changed. Review them before continuing.')
        if (current.job?.running) throw new Error('Another check is running. Wait for its result before continuing.')
        const id = snapshot.ids[index]
        await verificationAction({ action: 'run', check: id, digest: snapshot.digest })
        if (token.stopped) { await verificationAction({ action: 'cancel' }); break }
        // Acknowledgment is not a successful result. Wait for a finished job
        // and its retained run ID before considering the next command.
        let completed
        do {
          await new Promise(resolve => setTimeout(resolve, 1000))
          completed = await local.refresh()
          if (token.stopped) break
        } while (completed.job?.running)
        if (token.stopped) break
        if (completed.job?.check !== id || completed.job?.error || !completed.job?.run_id) throw new Error(completed.job?.error || 'The check finish could not be matched to this attempt. Inspect the latest receipt before continuing.')
        setBatch({ completed: index + 1, total: snapshot.ids.length })
      }
      if (!token.stopped) setMessage('The selected checks finished. Results below retain their individual scopes and receipts.')
    } catch (e) { if (!token.stopped) setRunError(e.message) }
    finally { if (!token.stopped) setBatch(null); if (sequence.current === token) sequence.current = null }
  }
  async function stop() {
    if (sequence.current) sequence.current.stopped = true
    setBatch(null); setMessage('Cancellation requested. Remaining selected checks will not start. Partial receipts stay in history.')
    try { await verificationAction({ action: 'cancel' }); await local.refresh() } catch (e) { setRunError(e.message) }
  }
  function refresh() { local.refresh().catch(() => {}); tools.refresh() }

  return <div className="ws-checks-panel" hidden={!active}>
    <div className="ws-checks-scroll">
      <section className="ws-checks-hero">
        <div className="ws-checks-eyebrow">{loading ? 'Reading evidence' : running ? 'Running now' : 'Results at a glance'}</div>
        <div className="ws-checks-hero-grid">
          <div className="ws-checks-dial" style={{ background: loading ? C.surf2 : summary.dial }} aria-hidden="true"><div><strong style={{ color: checkState(summary.state).color }}>{loading ? '—' : headNumber}</strong><span>{running ? 'running' : summary.attention ? 'to review' : summary.counts.pass ? 'passed' : 'receipts'}</span></div></div>
          <div><h2>{summary.title}</h2><p>{summary.explanation}</p>
            {!loading ? <div className="ws-checks-tallies">{Object.keys(CHECK_STATES).filter(k => summary.counts[k]).map(k => <button key={k} aria-pressed={filter === k} onClick={() => setFilter(v => v === k ? 'all' : k)} style={{ color: resultState(k).color }}><span aria-hidden="true">{resultState(k).glyph}</span>{summary.counts[k]} {resultState(k).label.toLowerCase()}</button>)}</div> : null}
          </div>
        </div>
        {summary.next ? <div className="ws-checks-next"><strong>What to do next</strong><p>{summary.next}</p></div> : null}
        <div className="ws-checks-actions">
          {running || batch ? <button onClick={stop} disabled={projectData?.job?.cancellation_requested}>{projectData?.job?.cancellation_requested ? 'Stopping…' : 'Stop checks'}</button> : <button className="ws-checks-primary" onClick={() => openPlan()}>{canRun ? `Review ${checks.length} ${checks.length === 1 ? 'check' : 'checks'} to run` : 'Set up project checks'}</button>}
          <button onClick={refresh} disabled={local.refreshing}>{local.refreshing ? 'Refreshing…' : 'Refresh evidence'}</button>
          {results.length ? <button onClick={() => onCopy(resultReport(ordered, projectData?.location?.repo || session.project), 'the check report')}>Copy report</button> : null}
        </div>
        {matchesProject && data?.location?.repo ? <p className="ws-checks-project">{data.location.repo}</p> : null}
        {data?.enabled && !matchesProject ? <p className="ws-checks-muted">Local verification is configured for {data.location.repo}. <a href="/verification#/trace">Open that project’s checks</a>.</p> : null}
      </section>

      {local.error ? <p className="ws-checks-alert" role="alert">Current local evidence is unavailable: {local.error}</p> : null}
      {runError || projectData?.job?.error ? <p className="ws-checks-alert" role="alert">{runError || projectData.job.error}</p> : null}
      {message ? <p className="ws-checks-message" role="status">{message}</p> : null}
      {projectData?.pending_draft_change ? <p className="ws-checks-alert">The requirement draft changed. <button onClick={() => onMode('verify')}>Review the revised requirements</button></p> : null}
      {plan ? <RunPlan data={projectData} selected={selected} setSelected={setSelected} onRun={runSelected} onClose={() => setPlan(false)} disabled={!canRun} /> : null}

      {running ? <section className="ws-checks-process" role="status"><strong><span className="ws-checks-spinner" aria-hidden="true"/>{checkName({ id: projectData.job.check })}</strong><p>{checks.find(c => c.id === projectData.job.check)?.verification?.pack?.kind === 'completion' ? 'Checking whether the recorded evidence covers the requirements…' : 'Running the check and capturing its result…'}</p><div className="ws-checks-progress" aria-hidden="true"/><small>{batch ? `${batch.completed} of ${batch.total} selected checks finished · ` : ''}The result will appear when its receipt is ready.</small><details><summary>Running command</summary><code>{JSON.stringify(checks.find(c => c.id === projectData.job.check)?.argv || [])}</code></details></section> : null}

      {results.length ? <><div className="ws-checks-filters"><label>Show<select value={filter} onChange={e => setFilter(e.target.value)}><option value="all">All results</option><option value="attention">Needs attention</option>{Object.keys(CHECK_STATES).map(k => <option key={k} value={k}>{resultState(k).label}</option>)}</select></label><button aria-pressed={sort === 'worst'} onClick={() => setSort(s => s === 'worst' ? 'catalog' : 'worst')}>{sort === 'worst' ? 'Attention first' : 'Check order'}</button></div><section className="ws-checks-results" aria-label="Check results"><h3>{results.length} {results.length === 1 ? 'check' : 'checks'}<span>One result per check</span></h3>{ordered.map(row => <SimpleResult key={row.key} row={row} onReceipt={setReceipt} onReview={openPlan} onInspect={onInspect}/>)}</section></> : null}
      {results.length && !ordered.length ? <p className="ws-checks-message">No results match this filter. <button onClick={() => setFilter('all')}>Show all results</button></p> : null}
      {!session.localVerification && !reading ? <RecordedReview review={review} session={session} onInspect={onInspect}/> : null}
      {tools.error ? <p className="ws-checks-muted ws-checks-tool-note">Tool evidence could not be read. <button onClick={() => onMode('tools')}>Review tool setup</button></p> : null}
      <details className="ws-checks-unconfigured"><summary>Available check types and setup</summary>{missing.length ? <><p>{missing.length} native check types have no configured instance in this reading.</p>{CHECK_GROUPS.map(group => { const types = missing.filter(c => c.group === group.id); return types.length ? <div key={group.id}><h4>{group.title}</h4>{types.map(c => <div key={c.id}><span>{c.title}</span><code>{c.id}</code></div>)}</div> : null })}</> : null}<p>14 native check types · 134 catalog specifications. Available types are not completed checks.</p><div className="ws-checks-actions"><button onClick={() => onMode('verify')}>Local setup and full catalog</button><button onClick={() => onMode('tools')}>Tool checks and setup</button></div></details>
      <footer className="ws-checks-foot"><details><summary>Reading identity and result scope</summary><p>Results cover their accepted scope. Receipts preserve earlier attempts.</p><p>{session.localVerification ? 'Local project verification' : session.origin || 'Captured session evidence'} · {session.id || session.key}</p></details></footer>
    </div>
    <nav className="ws-checks-navigation" aria-label="Checks panel tools"><button onClick={() => onMode('verdict')}>{HQ_COPY.chrome.tabs.verdict}</button><button onClick={() => onMode('trace')}>{HQ_COPY.chrome.tabs.trace}</button></nav>
    {receipt ? <ReceiptDialog key={receipt.key} row={receipt} session={session} data={projectData} onClose={() => setReceipt(null)} onCopy={onCopy} onInspect={onInspect}/> : null}
  </div>
}
