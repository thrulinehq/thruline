import React from 'react'
import { LOCAL_STATES, readLocalVerification } from './local-verification-model.js'

async function request(body) {
  if (!body) return readLocalVerification()
  const res = await fetch('/api/verification', body ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body), cache: 'no-store' } : { cache: 'no-store' })
  const data = await res.json()
  if (!res.ok) throw new Error(data.detail || data.error || `HTTP ${res.status}`)
  return data
}
export function LocalBadge({ state }) {
  const [glyph, label] = LOCAL_STATES[state] || LOCAL_STATES.unknown
  return <span className={`ws-result ws-result-${state}`}><span aria-hidden="true">{glyph}</span> {label}</span>
}
function NativeScope({ declaration }) {
  const plan = declaration?.pack
  const graph = plan?.graph || 'python_imports'
  return <div className="ws-hq-muted">
    {plan?.kind === 'architecture' ? <><p>{graph === 'rust_packages' ? 'Rust Cargo package declarations, including workspace aliases and target, dev, build and optional dependencies. Requires Python 3.11+ to read Cargo TOML. Rust module imports and resolved lockfile versions are outside this graph.' : 'Python static module imports under the selected interpreter grammar. Dynamic loading, unresolved imports and unsupported source leave closure incomplete.'}</p><p>Policy: {plan.policy?.id || 'not configured'} · source root: {plan.root} · at most 512 local nodes and 8192 edges. Explicit exclusions and external boundaries limit what a pass means.</p></> : null}
    {plan?.kind === 'suite' ? <p>Python unittest discovery for one accepted directory/pattern and 1–256 test IDs. Missing tests and unapproved skips cannot support a suite pass.</p> : null}
    {plan?.kind === 'resource' ? <p>Python cycle workload with sampled process-tree RSS on the local OS. Short peaks may be missed. No hard RAM ceiling or indefinite boundedness is established.</p> : null}
    {plan?.kind === 'completion' ? <p>Current accepted criteria and suitable captured child receipts only. Completion narration and compilation alone do not establish behavior.</p> : null}
    {!plan ? <p>Only the literal accepted command and up to 32 declared JSON examples are measured. A successful compiler command establishes compilation, not behavioral correctness.</p> : null}
    <p>Development runtime: macOS arm64; Python adapters exercised with Python 3.13.7. Other runtime combinations require their own qualification. The exact interpreter, inputs and verifier are bound in the receipt.</p>
  </div>
}
function ScopeDetails({ declaration, manifest }) {
  const files = Object.entries(manifest?.files || {})
  return <details><summary>What this result covers · {files.length || 'no captured'} input files</summary>
    <NativeScope declaration={declaration} />
    <p>Declared scope: {(declaration?.scope || []).join(', ') || 'not established'}</p>
    <p>{declaration?.environment_description || 'No environment declaration is available.'}</p>
    <p>{declaration?.environment_closed ? 'The accepted local dependency boundary is declared complete. This does not establish coverage of undeclared dependencies or services.' : 'Dependency scope is incomplete. Broader claims remain unknown.'}</p>
    <p>Configuration, tests, lockfiles, the process environment, and verifier identity participate in freshness checks.</p>
    {files.map(([path, file]) => <div className="ws-scope-file" key={path}><strong>{file.role}</strong><code>{path}</code><small>{file.external ? 'Declared external input' : 'Project input'} · {file.sha256}</small></div>)}
    {!files.length ? <p>Run this accepted check to capture its complete input manifest.</p> : null}
  </details>
}
function ObservedDetail({ observation: o, openReceipt }) {
  const value = o.observed
  return <>
    {o.counts ? <><h3>Captured test counts</h3><p>{o.counts.passed} passed · {o.counts.failed} failed · {o.counts.skipped} skipped · {o.counts.xfail} expected failures or TODO · {o.counts.total} total.</p>{o.counts.passed + o.counts.failed === 0 ? <p>No passing or failing test case was exercised. These counts do not establish a passing suite.</p> : null}</> : null}
    {['thruline-architecture/3', 'thruline-architecture/4'].includes(value?.protocol) ? <>
      <h3>The architecture rule</h3><p>{value.plan.policy ? `Policy ${value.plan.policy.id} · ${value.modules.length} ${value.graph_type === 'rust_packages' ? 'Cargo packages' : 'Python modules'} · ${value.edges.length} declared dependencies` : 'No policy configured. This graph is evidence for review, not an architecture pass.'}</p>
      <p>{value.method}</p><p>Source root: <code>{value.plan.root}</code>. {value.graph_type === 'rust_packages' ? 'Dev, build, optional and target-conditional declarations are included; this is not a Rust module-import graph.' : 'Conditional imports count as static dependencies.'} External module internals and runtime behavior are outside this graph.</p>
      {(value.plan.policy?.forbidden || []).map(rule => <p key={rule.id}><strong>{rule.id}</strong>: <code>{rule.from}</code> must not import <code>{rule.to}</code>.</p>)}
      {(value.cycles || []).map((cycle,i) => <article key={i}><strong>Dependency cycle: {cycle.path.join(' → ')}</strong>{cycle.edges.map((edge,j) => <p key={j}><code>{edge.path}:{edge.line}</code> · {edge.source} → {edge.target}<small>{edge.text}</small></p>)}</article>)}
      {(value.issues || []).map((issue,i) => <p className="ws-review-note" key={i}>Unresolved: <code>{issue.path}:{issue.line}</code> · {issue.reason}</p>)}
      <details><summary>Captured dependencies · {value.edges.length}</summary>{value.edges.slice(0,100).map((edge,i) => <p key={i}><code>{edge.path}:{edge.line}</code> · {edge.source} → {edge.target}<small>{edge.text}{edge.implicit_package ? ' · package initialization' : ''}</small></p>)}{value.edges.length > 100 ? <p>First 100 shown. Every edge is available in the underlying JSON evidence.</p> : null}</details>
      <details><summary>Accepted layer mapping and cycle exemptions</summary><pre>{JSON.stringify(value.plan.policy,null,2)}</pre></details>
    </> : null}
    {value?.protocol === 'thruline-memory/3' ? <>
      <h3>The measured workload</h3><p>{value.plan.workload} · {value.plan.size} {value.plan.size_unit}</p>
      <p>{value.plan.repetitions} fresh processes · {value.plan.warmup_cycles} warmup cycles and {value.plan.cycles} measured cycles each.</p>
      <p>Accepted sampled peak: {(value.plan.sampled_peak_budget_bytes/1048576).toFixed(1)} MiB. Accepted increase after a completed cycle: {(value.plan.retained_growth_budget_bytes/1048576).toFixed(1)} MiB above the last warmup.</p>
      {(value.repetitions || []).map(rep => <article key={rep.index}><strong>Repetition {rep.index} · {rep.complete ? 'Completed' : 'Incomplete'}</strong><p>{(rep.duration_ms/1000).toFixed(2)} seconds · {rep.samples.length} RSS samples · worker PID {rep.worker_pid}</p><p>Highest sampled process-tree memory: {(Math.max(0,...rep.samples.map(s=>s.rss_bytes))/1048576).toFixed(1)} MiB.</p>{rep.phases.map((phase,i) => <p key={i}>{phase.kind} {phase.cycle < 0 ? '' : `cycle ${phase.cycle+1}`} · {(phase.rss_bytes/1048576).toFixed(1)} MiB resident · {phase.processes.length} observed processes</p>)}</article>)}
      {(value.issues || []).map((issue,i) => <p className="ws-review-note" key={i}>Incomplete: {issue}</p>)}
      <h3>What these numbers mean</h3><p>{value.boundary}</p><p>{value.method}</p><p>{value.process_scope}</p><p>OS high-water mark: not measured. Hard RAM limit: not enforced. A brief pass does not establish that memory stays bounded indefinitely.</p>
      <details><summary>Host and measurement environment</summary><pre>{JSON.stringify(value.environment,null,2)}</pre></details>
    </> : null}
    {value?.protocol === 'thruline-unittest/2' ? <details open><summary>Discovered and executed tests · {value.discovered.length}</summary><p>Suite: {value.directory} · Pattern: {value.pattern}</p>{value.rows.map(row => <p key={row.id}><strong>{row.state}</strong> · <code>{row.id}</code>{row.detail ? <small>{row.detail}</small> : null}</p>)}</details> : null}
    {value?.protocol === 'thruline-completion/2' ? <>{value.criteria.map(c => <article key={c.id}><strong>{c.description}</strong><p>Required evidence: {c.evidence_kind || 'not declared'}</p>{c.checks.map(k => <div key={k.check_id}><p>{k.check_id}: {!k.run_ref ? 'No attempt captured' : !k.suitable ? 'This kind of check cannot settle the criterion' : !k.current ? 'Needs a fresh run' : !k.admissible ? 'Evidence incomplete' : k.outcome}</p>{k.run_ref ? <button onClick={() => openReceipt(k.run_ref.namespace + k.run_ref.id)}>Open supporting receipt</button> : null}</div>)}</article>)}<p>{value.boundary}</p></> : null}
    {value?.artifact_digests ? <><h3>Artifacts produced from this source</h3>{Object.entries(value.artifact_digests).map(([path,digest]) => <p key={path}><code>{path}</code><small>SHA-256: {digest}</small></p>)}<p>Output paths began empty in the private execution copy. The original workspace artifact was not replaced; this result does not prove a deployed artifact matches.</p></> : null}
    {o.resources ? <details><summary>Execution resources · {(o.resources.elapsed_ms / 1000).toFixed(2)} seconds</summary><p>{o.resources.sampled_peak_rss_bytes != null ? `Highest sampled process memory: ${(o.resources.sampled_peak_rss_bytes / 1048576).toFixed(1)} MiB across ${o.resources.samples} samples.` : 'Memory observation is incomplete: no usable samples were captured.'}</p><p>{o.resources.scope}</p></details> : null}
    {o.command ? <details><summary>Actual process and working directory</summary><pre>{JSON.stringify(o.command, null, 2)}</pre></details> : null}
  </>
}
export default function LocalVerification({ session, onInspect, initialReceipt, receiptOnly = false }) {
  const [data, setData] = React.useState(session.localData || null)
  const [error, setError] = React.useState('')
  const [receipt, setReceipt] = React.useState(null)
  const [busy, setBusy] = React.useState(false)
  const [review, setReview] = React.useState(null)
  const mounted = React.useRef(true)
  const epoch = React.useRef(0)
  const reading = React.useRef(false)
  const read = React.useCallback(async () => {
    if (reading.current) return
    reading.current = true
    const token = ++epoch.current
    try {
      const value = await request()
      if (!mounted.current || token !== epoch.current) return
      setData(value); setError('')
      if (value.enabled) window.dispatchEvent(new CustomEvent('thruline-verification-update', { detail: value }))
    } catch (e) { if (mounted.current && token === epoch.current) setError(`Current evidence could not be read. Previous results are not confirmed current. ${e.message}`) } finally { reading.current = false }
  }, [])
  React.useEffect(() => { if (session.localData) { setData(session.localData); setError(session.localData.readError || '') } }, [session.localData])
  React.useEffect(() => { if (initialReceipt) openReceipt(initialReceipt) }, [initialReceipt])
  React.useEffect(() => {
    mounted.current = true
    read()
    // This follows only the configured local project; it neither connects to
    // an agent nor runs checks automatically.
    const timer = session.localVerification ? null : setInterval(read, 3000)
    const open = e => { if (e.detail?.run) openReceipt(e.detail.run) }
    window.addEventListener('thruline-open-local-receipt', open)
    return () => { mounted.current = false; epoch.current++; clearInterval(timer); window.removeEventListener('thruline-open-local-receipt', open) }
  }, [read])
  async function action(body) {
    setBusy(true); setError('')
    try { await request(body); setReview(null); await read() } catch (e) { setError(e.message) } finally { setBusy(false) }
  }
  async function openReceipt(run) {
    setBusy(true)
    try { const value = await request({ action: 'receipt', run }); if (mounted.current) setReceipt(value) } catch (e) { if (mounted.current) setError(e.message) } finally { if (mounted.current) setBusy(false) }
  }
  if (receiptOnly && !receipt) return <section className="ws-hq-section"><p role={error ? 'alert' : 'status'}>{error || 'Reading the captured receipt…'}</p>{error ? <button onClick={() => openReceipt(initialReceipt)}>Retry receipt</button> : null}</section>
  if (!data) return <section className="ws-hq-section"><h2>Local verification</h2><p role={error ? 'alert' : 'status'}>{error || 'Reading local check availability…'}</p><button onClick={read}>Retry</button></section>
  const count = data.coverage || {}
  if (!data.enabled) return <section className="ws-hq-section"><h2>Run checks on a local project</h2><p>{count.implemented || 0} executable checks of {count.total || 134} catalog targets. Choose the project explicitly when starting the local server.</p><pre>thruline ui --verification-project /path/to/project --exec-db /path/outside/project/exec.db</pre><p>Review and accept measurable requirements with <code>thruline acceptance</code>. Agent connections are not required.</p></section>
  const running = data.job?.running
  if (receipt) {
    const r = receipt.record
    const current = data.pack_results?.find(c => c.run_id === r.run_id)
    return <section className="ws-check-detail ws-local-verification">
      {!receiptOnly ? <button onClick={() => setReceipt(null)}>← Local checks</button> : null}
      <div className="ws-hq-kicker">INSPECTABLE RECEIPT · LOCAL CAPTURE</div><h2>{r.check.id}</h2>
      <LocalBadge state={error ? 'unknown' : current?.status || receipt.verification.state} />
      <p>{error || current?.explanation || receipt.verification.explanation}</p>
      <h3>What we agreed to check</h3>{r.criterion.map(c => <p key={c.id}>{c.description}</p>)}
      <h3>What actually happened</h3>
      {(r.finish?.pack?.observations || []).map(o => <article className="ws-history-entry" key={o.id}><strong>{o.description}</strong><p>{o.reason}</p>{o.expected != null ? <dl><dt>Expected</dt><dd><code>{JSON.stringify(o.expected)}</code></dd><dt>Observed</dt><dd><code>{JSON.stringify(o.observed)}</code></dd></dl> : null}{o.observed?.protocol !== 'thruline-completion/2' ? <p>Process exit: {o.exit ?? (o.signal ? `signal ${o.signal}` : 'not captured')} · Expected: {o.expected_exit}</p> : <p>Collector evaluation of recorded evidence; no agent statement or process success was substituted.</p>}<ObservedDetail observation={o} openReceipt={openReceipt}/></article>)}
      {!r.finish ? <p>The start was recorded. No finish was captured; completion is unknown.</p> : null}
      <ScopeDetails declaration={r.check.verification} manifest={r.start.inputs.scope_manifest}/>
      <h3>What remains uncertain</h3><p>This result covers its accepted scope and observations. It does not establish correctness of the entire project.</p>
      {(r.finish?.pack?.review_notes || []).map((note,i) => <p className="ws-review-note" key={i}>Review note: {note}</p>)}
      <h3>What to do next</h3><p>{current?.next_action || 'Inspect this historical evidence and run a fresh check when its inputs or required evidence change.'}</p>
      <h3>Does this evidence still apply?</h3><p>{current ? current.status === 'stale' ? 'The project changed after this run. Keep this receipt as history and run a fresh check.' : 'The current project reading is shown above.' : 'This is a historical attempt. A later attempt or requirement revision may supersede it.'}</p>
      <dl><dt>Examined code</dt><dd><code>{r.start.inputs.tree || 'not captured'}</code></dd><dt>Current code</dt><dd><code>{data.evaluation?.subject || 'not captured'}</code></dd><dt>Accepted by</dt><dd>{r.accepted_by} · local attribution</dd><dt>Captured</dt><dd>{r.started_at} → {r.finished_at || 'finish not captured'}</dd><dt>Origin</dt><dd>{r.origin}</dd><dt>Verifier</dt><dd>{r.finish?.pack?.verifier || r.rule_version}</dd></dl>
      <p>{r.trust}. A hash protects captured bytes; it does not make an agent's statement independent evidence.</p>
      <h3>Underlying evidence</h3>{receipt.outputs.map((o, i) => <details key={`${i}:${o.digest}`}><summary>{i % 2 === 0 ? 'Standard output' : 'Standard error'} · {o.metadata.bytes} bytes · {o.available ? 'integrity checked' : 'unavailable'}</summary><code>{o.digest}</code><pre>{o.available ? o.text || '(captured empty stream)' : 'The underlying evidence is missing. This receipt cannot establish a current pass.'}</pre></details>)}
      {(receipt.artifacts || []).map(a => <details key={a.path}><summary>Produced artifact · {a.path} · {a.available ? 'integrity checked' : 'unavailable'}</summary><p>{a.bytes ?? 'Unknown'} bytes · {a.digest}</p><pre>{a.preview ?? 'Binary artifact: no text preview.'}</pre><p>{a.preview_truncated ? 'Preview is limited to 64 KiB. The complete stored bytes were hashed.' : 'Complete artifact integrity is checked separately from this preview.'}</p><code>{a.stored_path}</code></details>)}
      <details><summary>Exact command, bindings, limits and rule replay</summary><pre>{JSON.stringify(r, null, 2)}</pre></details>
      <p>Deterministic replay: {receipt.verification.replay_agrees ? 'matches the stored outcome' : 'incomplete or not reproducible'}.</p>
      <code>{receipt.receipt_digest}</code>
      {(r.finish?.pack?.limitations || []).map((x, i) => <p className="ws-hq-muted" key={i}>{x}</p>)}
      <button onClick={() => { const blob = new Blob([JSON.stringify(receipt, null, 2)], { type: 'application/json' }); const url = URL.createObjectURL(blob); const a = document.createElement('a'); a.href = url; a.download = `${r.run_id}.json`; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000) }}>Download this receipt</button>
    </section>
  }
  return <section className="ws-hq-section ws-local-verification">
    <div className="ws-hq-kicker">LOCAL CHECKS · {count.implemented}/{count.total} IMPLEMENTED</div>
    <h2>Does the work meet the requirement?</h2>
    <p>{data.contract?.objective || 'No requirement has been accepted for this project yet.'}</p>
    <p className="ws-hq-muted">{data.location?.repo} · {count.validated} catalog IDs have retained historical development validation. This candidate’s qualification is separate.</p>
    {data.project_coverage ? <p className="ws-hq-next">{data.project_coverage.accepted_checks} accepted local checks · {data.project_coverage.executed} attempted · {data.project_coverage.passed} passed · {data.project_coverage.failed} failed · {data.project_coverage.stale} stale · {data.project_coverage.incomplete} incomplete · {data.project_coverage.not_run} not run. {data.project_coverage.explanation}</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    {data.job?.error ? <p role="alert">The attempt could not complete: {data.job.error}</p> : null}
    {data.pending_draft_change ? <p role="alert">The requirement draft changed. Review and accept its new digest before running.</p> : null}
    <button disabled={busy} onClick={read}>Refresh current evidence</button>
    {data.draft && (!data.accepted || data.pending_draft_change) ? <section className="ws-hq-next" aria-label="Review requirement draft">
      <h3>Review the requirement draft</h3>
      <p>{data.draft.objective}</p>
      <p>Revision {data.draft.revision} · {data.draft.checks.length} checks. Acceptance records these requirements locally. It does not run a command or mark the work complete.</p>
      {data.draft.criteria.map(criterion => <p key={criterion.id}><strong>{criterion.id}</strong> · {criterion.description} · Checks: {criterion.check_ids.join(', ')}{criterion.human_acceptance_required ? ' · Human review required' : ''}</p>)}
      {data.draft.checks.map(check => <details key={check.id}><summary>{check.id} · {check.timeout_seconds}s budget</summary><pre>{JSON.stringify(check.argv)}</pre><p>Working directory: {check.cwd}</p><NativeScope declaration={check.verification} /><p>Declared inputs: {check.verification?.scope?.join(', ') || 'Command inputs and repository snapshot'}</p><pre>{JSON.stringify(check.verification?.pack || check.verification?.cases || {}, null, 2)}</pre></details>)}
      <p>Protected inputs: {data.draft.protected_paths.join(', ') || 'None declared'}</p>
      <p>Only the exact reviewed draft will be accepted. A changed file or a superseded revision must be reviewed again.</p>
      <code>{data.draft_digest}</code>
      {data.draft_review_redacted ? <p role="alert">Sensitive content was masked in this draft. Remove literal credentials from the configuration before reviewing and accepting it here.</p> : null}
      <button disabled={busy || running || !!error || !data.draft_digest || data.draft_review_redacted} onClick={() => action({ action: 'accept', digest: data.draft_digest, ...(data.acceptance_hash ? { supersedes: data.acceptance_hash } : {}) })}>{data.accepted ? 'Accept reviewed revision' : 'Accept reviewed requirements'}</button>
    </section> : !data.accepted ? <p>Define observable examples and commands in <code>.thruline/acceptance.json</code>. The draft will appear here for review.</p> : null}
    {(data.pack_results || []).map(result => {
      const executing = running && data.job.check === result.id
      const check = data.contract.checks.find(c => c.id === result.id)
      return <article className="ws-local-check" key={result.id}><h3>{result.id}</h3><LocalBadge state={executing ? 'running' : error ? 'unknown' : result.status} /><p>{executing ? 'Capturing a new attempt. The result is not yet known.' : result.explanation}</p><p>Scope: {result.scope.join(', ')}</p>
        <ScopeDetails declaration={result.scope_declaration} manifest={result.covered_inputs}/>
        {(result.review_notes || []).map((note,i) => <p className="ws-review-note" key={i}>Review note: {note}</p>)}
        {result.next_action ? <p>Next: {result.next_action}</p> : null}
        <div className="ws-hq-inline-actions"><button disabled={busy || running || !!error || data.pending_draft_change} onClick={() => setReview(check)}>{result.run_id ? 'Review and rerun' : 'Review and run'}</button>{result.run_id ? <button disabled={busy} onClick={() => openReceipt(result.run_id)}>Inspect receipt</button> : null}{executing ? <button disabled={busy || data.job.cancellation_requested} onClick={() => action({ action: 'cancel' })}>{data.job.cancellation_requested ? 'Stopping capture…' : 'Cancel check'}</button> : null}</div>
        <details><summary>Catalog checks evaluated · {result.catalog_results.length}</summary>{result.catalog_results.map(c => <p key={c.check_id}><code>{c.check_id}</code> <LocalBadge state={error ? 'unknown' : c.status} /></p>)}</details>
          {review?.id === result.id ? <section className="ws-hq-next" aria-label="Review accepted check"><h3>Run this accepted check once</h3><p>{data.contract.criteria.filter(c => c.check_ids.includes(review.id)).map(c => c.description).join(' ')}</p><pre>{review.verification?.pack?.kind === 'completion' ? 'Evaluate the accepted criteria against captured check receipts.' : JSON.stringify(review.argv)}</pre>{review.verification?.pack ? <p>Accepted evaluator: {review.verification.pack.kind}. Its execution details will be recorded in the receipt.</p> : null}<NativeScope declaration={review.verification} /><p>Time budget: {review.timeout_seconds}s · Local execution · No RAM or network isolation.</p><p>Scheduled only when you choose Run. One check runs at a time per evidence store. Builds and longer checks are never started by refresh.</p><p>{review.verification?.pack?.kind === 'completion' ? 'This reads captured evidence and starts no project command. Cancellation or an exceeded budget leaves the evaluation incomplete.' : "Execution uses a private source copy, but commands still have your local user's permissions. Only the declared scope and observable results will be evaluated."}</p><div className="ws-hq-inline-actions"><button disabled={busy || running || !!error} onClick={() => action({ action: 'run', check: review.id, digest: data.contract_digest })}>Run accepted check</button><button onClick={() => setReview(null)}>Back</button></div></section> : null}
      </article>
    })}

    <details><summary>Run history · {(data.history || []).length} preserved receipts</summary>{(data.history || []).map(r => <button className="ws-source-link" key={r.record.run_id} onClick={() => openReceipt(r.record.run_id)}>{r.record.check.id} · {LOCAL_STATES[r.record.historical_state]?.[1] || 'Incomplete'}<small>{r.record.started_at} · Open original evidence →</small></button>)}</details>
    <details><summary>Evidence and execution boundaries</summary><p>Agent statements and imported reports remain reports. These results come from explicitly requested local capture. Relevant input changes invalidate current results; history is retained.</p><pre>{JSON.stringify(data.dependency_index, null, 2)}</pre><pre>{JSON.stringify(data.limits, null, 2)}</pre></details>
  </section>
}
