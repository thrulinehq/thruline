/* NOT-APPROVAL-WORDS: `ready` here is an integration recipe's own, not a Change's readiness (RUN-LINK-TRUTH-R5 census). */
import React from 'react'
import { IntegrationSupport } from './integration-support.jsx'
import { GuidedChecks } from './integration-workflow.jsx'
import { RuffRepair } from './integration-repair.jsx'
import { NativeRepair } from './integration-native-repair.jsx'
import { AgentRepair } from './integration-agent-repair.jsx'
import { ReviewOrchestrator } from './integration-review.jsx'
import { NativeFacts, FactValue } from './integration-native-facts.jsx'
import { readIntegration } from './integration-read.js'
import { AI_REVIEW_AGENT_RECIPES } from './ai-review-ui.js'
import catalog from '../../../spec/integrations/catalog.json'
import qualification from '../../../spec/integrations/qualification/status.json'
import { groupPacks, integrationView, toolStatus, quoteArgument, executionProblem, aiReviewOffered, offeredPacks, offeredWorkflow } from './integration-model.js'

export function IntegrationWorkspace({ session, onCopy }) {
  const [data, setData] = React.useState(null)
  const [workflowReading, setWorkflowReading] = React.useState(null)
  const [error, setError] = React.useState('')
  const [loading, setLoading] = React.useState(false)
  const [revision, setRevision] = React.useState(0)
  const [selected, setSelected] = React.useState(null)
  const [query, setQuery] = React.useState('')
  const sessionId = session.id || session.key
  const [reviewSession, setReviewSession] = React.useState(null)
  React.useEffect(() => { setReviewSession(null) }, [sessionId])
  const id = reviewSession || sessionId
  React.useEffect(() => {
    const controller = new AbortController()
    setData(null); setError(''); setLoading(true)
    readIntegration(`/api/integrations?session=${encodeURIComponent(id)}`, { signal: controller.signal })
      .then(body => integrationView(body, id))
      .then(value => { if (!controller.signal.aborted) { setData(value); setWorkflowReading({ session: id, workflow: value.workflow }) } })
      .catch(e => { if (!controller.signal.aborted) setError(e.message) })
      .finally(() => { if (!controller.signal.aborted) setLoading(false) })
    return () => controller.abort()
  }, [id, revision])
  const current = data?.session === id ? data : null
  // Keep same-project action state mounted during a fresh result reading.
  // Results themselves stay cleared until the server rechecks their inputs.
  // AI-REVIEW-OUT-OF-LAUNCH: only an explicit `ai_review: true` from the
  // server lets a PR-Agent recipe, source or control onto this screen.
  const reading = current?.workflow || (workflowReading?.session === id ? workflowReading.workflow : null)
  const aiReview = aiReviewOffered(reading)
  const workflow = offeredWorkflow(reading)
  const packs = offeredPacks(current?.catalog.projects || catalog.projects, reading)
  const groups = groupPacks(packs)
  const active = packs.find(p => p.id === selected)
  const runs = current?.results?.runs || []
  const visible = runs.filter(r => !active || r.record.plan.pack === active.id)
  return <section className="ws-integration-workspace">
    <div className="ws-hq-kicker">VERIFY · TOOL EVIDENCE</div>
    <h2>One place to check the work</h2>
    <p>Agree on what should work. Choose a tool that can measure it. Follow the result back to its captured run.</p>
    <ReviewOrchestrator sessionId={id} aiReview={aiReview} onSelectProject={setReviewSession} onRefresh={() => setRevision(n => n + 1)} />
    <div className="ws-hq-inline-actions"><button onClick={() => setRevision(n => n + 1)} disabled={loading}>{loading ? 'Reading tool evidence…' : 'Refresh tool evidence'}</button><span className="ws-hq-muted">{packs.length} sources · {groups.length} areas</span></div>
    {error ? <p role="alert" className="ws-integration-error">{error} The source catalog below is still available; no stored results are being shown.</p> : current?.results_error ? <p role="alert" className="ws-integration-error">Current results could not be established: {current.results_error}</p> : null}
    <div className="ws-integration-results"><h3>{active ? `${active.id} results` : 'Captured tool results'}</h3>
      {loading ? <p role="status">Checking the current work against stored receipts…</p> : !error && !current?.results_error && !visible.length ? <p>No integration run is recorded{active ? ' for this tool' : ' for this project'}. That says nothing about whether checks ran elsewhere.</p> : null}
      {visible[0] ? <IntegrationResult key={visible[0].binding_id} row={visible[0]} onCopy={onCopy} /> : null}
      <GuidedChecks key={id} sessionId={id} workflow={workflow} onRefresh={() => setRevision(n => n + 1)} />
      {workflow?.recipes?.some(recipe => recipe.id === 'ruff' && recipe.ready) ? <RuffRepair key={`repair-${id}`} sessionId={id} configuration={workflow.ruff_repair_configuration} onRefresh={() => setRevision(n => n + 1)} /> : null}
      {workflow?.recipes?.filter(recipe => recipe.ready && ['clippy', 'knip', 'openrewrite', 'semgrep', 'opengrep', 'osv-scanner'].includes(recipe.id)).map(recipe => <NativeRepair key={`native-repair-${id}-${recipe.id}`} sessionId={id} recipe={recipe.id} onRefresh={() => setRevision(n => n + 1)} />)}
      {workflow?.recipes?.filter(recipe => recipe.ready && ['aider', 'openhands-sdk', ...AI_REVIEW_AGENT_RECIPES, 'mini-swe-agent'].includes(recipe.id)).map(recipe => <AgentRepair key={`agent-repair-${id}-${recipe.id}`} sessionId={id} recipe={recipe.id} receipts={runs} onRefresh={() => setRevision(n => n + 1)} />)}
      {visible.length > 1 ? <ReceiptHistory rows={visible.slice(1)} onCopy={onCopy} /> : null}
      {current?.results?.coverage ? <p className="ws-hq-muted">{current.results.coverage}</p> : null}
      {current?.presentation?.inline_artifact_payloads_omitted ? <p className="ws-hq-muted">{current.presentation.scope}</p> : null}
    </div>
    {current?.project_profile ? <details><summary>Suggested checks for this project</summary><p>{current.project_profile.manifests.join(', ') || 'No supported root manifest found'}</p><div className="ws-hq-inline-actions">{current.project_profile.suggested_packs.map(pack => <button key={pack} onClick={() => { setSelected(pack); setQuery(pack) }}>{pack}</button>)}</div><p className="ws-hq-muted">{current.project_profile.basis}</p></details> : null}
    <label className="ws-integration-search">Find a capability<input value={query} onChange={e => setQuery(e.target.value)} placeholder="Architecture, memory, browser tests…" /></label>
    <div className="ws-integration-groups">{groups.map(group => {
      const matched = group.packs.filter(p => `${p.id} ${p.contribution} ${p.feature_name}`.toLowerCase().includes(query.toLowerCase()))
      return matched.length ? <details key={group.id} open={!!query || group.id === active?.feature}><summary>{group.name}<small>{matched.length} {matched.length === 1 ? 'source' : 'sources'}</small></summary>{matched.map(p => <button key={p.id} className="ws-integration-pack" aria-pressed={selected === p.id} onClick={() => setSelected(selected === p.id ? null : p.id)}><strong>{p.id}</strong><span>{p.contribution}</span><small>{p.readiness || 'Report reader · qualification pending'}</small></button>)}</details> : null
    })}</div>
    {active ? <article className="ws-integration-detail"><button className="ws-text-button" onClick={() => setSelected(null)}>← All tool evidence</button><h3>{active.id}</h3><p>{active.contribution}</p><p className="ws-hq-muted">{active.limits}</p>
      <h4>How this can help</h4><p>{active.candidate_check_ids.join(' · ')} are proposed check mappings. A tool result counts only for the criterion actually accepted for that run.</p>
      <QualificationEvidence pack={active} onCopy={onCopy} />
      {active.format ? <p>{workflow?.recipes?.some(r => r.id === active.id) ? 'Use the guided requirement above to prepare and run this check. Setup problems are explained there.' : 'A report reader exists for this source. A qualified, provisioned execution recipe is still needed; importing a report does not establish captured execution.'}</p> : <p>This source informs the native evidence engine. It is not an external scanner to install or run.</p>}
      <details><summary>Source, license and implementation boundary</summary><dl><dt>Upstream</dt><dd><a href={`https://github.com/${active.repository}/tree/${active.commit}`} target="_blank" rel="noreferrer">{active.repository}</a></dd><dt>Exact source commit</dt><dd><code>{active.commit}</code></dd><dt>Declared main license</dt><dd>{active.declared_main_license} · per-file and distribution review remains separate.</dd><dt>Missing source dependencies</dt><dd>{active.submodules} submodule references; payloads were not included in the supplied archive.</dd><dt>Qualification</dt><dd>New adapter findings remain advisory. No claim of production detection precision.</dd><dt>Report contract</dt><dd>{active.format || 'Native design reference'}</dd></dl></details>
    </article> : null}

  </section>
}

function QualificationEvidence({ pack, onCopy }) {
  const q = qualification.tools[pack.id]
  if (!q) return <p className="ws-hq-muted">Representative independent qualification is not established for this capability. Its readiness label describes implementation, not detection accuracy.</p>
  const c = q.counts
  return <details><summary>How much has this adapter been checked?</summary>
    <p><strong>Advisory · not a release gate.</strong> {q.independence}</p>
    <p>{c.clean} clean scopes · {c.violations} violations · {c.incomplete} incomplete scopes.</p>
    <p>False alarms: {c.clean ? `${c.false_positives} of ${c.clean} clean scopes` : 'no clean sample measured'}. Missed failures: {c.violations ? `${c.missed_failures} of ${c.violations} violations` : 'no violation sample measured'}.</p>
    <p>{c.incomplete ? `${c.incomplete_false_clears} false clears across ${c.incomplete} incomplete scopes.` : 'No independent incomplete-scope case in this selection.'} {q.boundary_cases} separate capture/decoder perturbations; {q.boundary_mismatches} mismatches. These are not additional scanner-accuracy samples.</p>
    <p>Freshness: {q.freshness.fresh} → edit → {q.freshness.after_edit}; previous receipt {q.freshness.receipt_preserved ? 'preserved' : 'preservation not established'}. Missing setup {q.setup.missing_refused ? 'refused' : 'not established'}; unsupported runtime {q.setup.unsupported_version_refused ? 'refused' : 'not established'}.</p>
    {q.cancellation ? <p>{q.cancellation.established ? `A check using the actual scanner was cancelled. It stayed incomplete; ${q.cancellation.remaining_observed_processes} observed child processes remained.` : 'Cancellation with the actual scanner is not established.'} {q.cancellation.limits} {q.cancellation.binding_id ? <button onClick={() => onCopy(`thruline --db ${quoteArgument(q.cancellation.database)} evidence --id ${quoteArgument(q.cancellation.binding_id)} --root ${quoteArgument(q.cancellation.root)} --json`, 'the cancelled check receipt command')}>Copy cancelled check receipt command</button> : null}</p> : null}
    <p><strong>Still uncertain:</strong> {q.pending}</p>
    <details><summary>Inspect the measured cases and their receipts</summary><p>These are retained local validation fixtures, separate from your project’s receipts.</p>{q.cases.map(row => <article key={row.id}><strong>{row.id}</strong><p>{row.scope}</p><p>Expected: {row.label}. Observed: {row.actual}. Class: {row.case_class.replaceAll('_', ' ')}.</p><p><a href={`https://github.com/${pack.repository}/blob/${pack.commit}/${row.oracle_source}`} target="_blank" rel="noreferrer">Upstream expectation: {row.oracle}</a></p><button onClick={() => onCopy(`thruline --db ${quoteArgument(row.database)} evidence --id ${quoteArgument(row.binding_id)} --root ${quoteArgument(row.root)} --json`, 'the validation receipt command')}>Copy this validation receipt command</button></article>)}</details>
  </details>
}
export function IntegrationResult({ row, onCopy }) {
  const s = toolStatus(row), record = row.record, report = record.report
  const problem = executionProblem(record)
  const [showReport, setShowReport] = React.useState(false)
  const [showReceipt, setShowReceipt] = React.useState(false)
  const [showExact, setShowExact] = React.useState(false)
  const [shown, setShown] = React.useState(100)
  return <article className="ws-integration-result"><div className={`ws-result ws-result-${({ passed: 'pass', failed: 'fail' })[s.state] || s.state}`}>{s.label}</div><h4>{record.plan.criterion.requirement}</h4><p><strong>What was checked:</strong> {record.plan.criterion.success}</p><p>{s.meaning}</p><IntegrationSupport support={record.plan.support} />
    {problem ? <p>{problem.why}</p> : null}
    {row.assessment?.next || problem ? <div className="ws-hq-next"><strong>What to do next</strong><p>{problem?.next || row.assessment.next}</p></div> : null}
    {row.assessment?.changed_inputs?.length ? <p>Changed inputs: {row.assessment.changed_inputs.join(', ')}.</p> : null}
    <p className="ws-hq-muted">{record.plan.pack} · {record.completed_at} · {record.duration_ms} ms · {report?.observations?.length || 0} reported observations</p>
    {report ? <NativeFacts facts={report.facts} /> : null}
    <details onToggle={e => setShowReport(e.currentTarget.open)}><summary>What the tool actually reported</summary>{showReport ? <>{report ? <><dl>{Object.entries(report.facts).map(([k, v]) => <React.Fragment key={k}><dt>{k.replaceAll('_', ' ')}</dt><dd>{k === 'scan_coverage' ? <CoverageSummary value={v} /> : <FactValue name={k} value={v} />}</dd></React.Fragment>)}</dl>{report.observations.slice(0, shown).map((o, i) => <blockquote key={i}><strong>{o.message}</strong>{o.path ? <p>{o.path}{o.line ? ` · line ${o.line}` : ''}</p> : null}<small>Report location: {o.pointer} · rule {o.rule}</small></blockquote>)}{report.observations.length > shown ? <button onClick={() => setShown(n => n + 100)}>Show more observations · {shown} of {report.observations.length} shown</button> : null}{report.limitations.map((l, i) => <p key={i}>{l}</p>)}</> : <><p>{record.decode_error || 'The output could not be read.'}</p>{record.stderr_excerpt ? <pre>{record.stderr_excerpt}</pre> : null}</>}</> : null}</details>
    <details onToggle={e => setShowReceipt(e.currentTarget.open)}><summary>Open receipt · execution and trust limits</summary>{showReceipt ? <><p>The receipt binds the accepted criterion, exact command, tool binary, inputs, and captured output. New adapters remain advisory until independently qualified.</p><dl><dt>Requirement</dt><dd>{record.plan.criterion.requirement}</dd><dt>Accepted by</dt><dd>{record.plan.criterion.accepted_by}</dd><dt>Input coverage</dt><dd>{record.admission.input_closure_complete ? 'Declared complete by the accepting operator' : 'Not fully identified'}</dd><dt>Receipt</dt><dd><code>{row.binding_id}</code></dd><dt>Tool process</dt><dd>{record.admission.run_state.replaceAll('_', ' ')} · exit {record.admission.exit_code ?? 'not available'}</dd>{record.isolation ? <><dt>Isolated execution</dt><dd>{record.isolation.cleanup} · {record.isolation.network || 'see exact receipt for container state'}</dd></> : null}<dt>Observed resources</dt><dd>{record.sampled_peak_rss_bytes == null ? 'RSS sample unavailable' : `${(record.sampled_peak_rss_bytes / 1048576).toFixed(1)} MiB sampled peak RSS`} · {record.stdout_bytes + record.stderr_bytes} output bytes</dd></dl>{record.limitations.map((l, i) => <p key={i}>{l}</p>)}<button onClick={() => onCopy(`thruline evidence --id ${quoteArgument(row.binding_id)} --root ${quoteArgument(record.plan.root)} --json`, 'the receipt command')}>Copy receipt command</button><details onToggle={e => setShowExact(e.currentTarget.open)}><summary>Exact execution and input identities</summary>{showExact ? <pre>{JSON.stringify(row, null, 2)}</pre> : null}</details></> : null}</details>
  </article>
}

function ReceiptHistory({ rows, onCopy }) {
  const [open, setOpen] = React.useState(false)
  return <details onToggle={e => setOpen(e.currentTarget.open)}><summary>Earlier receipts · {rows.length} preserved</summary>{open ? rows.map(row => <IntegrationResult key={row.binding_id} row={row} onCopy={onCopy} />) : null}</details>
}

function CoverageSummary({ value }) {
  const counts = value?.counters
  if (counts) return <><p>Read {counts.processed} of {counts.total} project files. {counts.processed < counts.total ? 'The remaining files were outside this scan.' : 'Every file in this inventory was processed.'}</p>{value.configurationHints?.length ? <p>{value.configurationHints.length} configuration notes need review.</p> : null}<p className="ws-hq-muted">This inventory still depends on your entry points, plugins and exclusions.</p></>
  if (typeof value?.tested === 'number') return <p>Tested {value.tested} of {value.total} API operations; {value.skipped} skipped and {value.errored} errored.</p>
  if (typeof value?.scanned_files === 'number') return <><p>Read {value.scanned_files} files with {value.effective_rules} active rules.</p><p>{value.skipped_files} files and {value.skipped_rules} rules were skipped.</p><p className="ws-hq-muted">Counts describe the selected scan paths; they do not prove that every project behavior was tested.</p></>
  return <pre>{JSON.stringify(value, null, 2)}</pre>
}
