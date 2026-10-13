import React from 'react'
import integrationCoverage from '../../../spec/integrations/coverage.json'
import LocalVerification from './local-verification.jsx'
import { IntegrationWorkspace } from './integrations.jsx'
import { catalog, catalogRows, RESULT_STATES, BINDINGS, assessResult } from './check-contract.js'

export function ResultBadge({ status }) {
  const glyph = { pass: '✓', fail: '×', stale: '↻', blocked: '⊘', unknown: '?', reported: '“', queued: '◷', running: '●', waived: '◇', not_applicable: '—' }[status] || '?'
  return <span className={`ws-result ws-result-${status}`} title={RESULT_STATES[status]?.[1]}><span aria-hidden="true">{glyph}</span> {RESULT_STATES[status]?.[0] || 'Not established'}</span>
}

export function CheckCatalog({ session, onInspect, onCopy, initialSurface }) {
  const [surface, setSurface] = React.useState(initialSurface || (session.localVerification ? 'local' : 'checks'))
  const [coverage, setCoverage] = React.useState(session.localData?.coverage || null)
  React.useEffect(() => { if (session.localData?.coverage) return; let active = true; fetch('/api/verification', { cache: 'no-store' }).then(r => r.ok ? r.json() : null).then(d => { if (active) setCoverage(d?.coverage || null) }).catch(() => {}); return () => { active = false } }, [])
  const [query, setQuery] = React.useState('')
  const [family, setFamily] = React.useState('all')
  const [selected, setSelected] = React.useState(null)
  const [tab, setTab] = React.useState('explanation')
  const [filter, setFilter] = React.useState('all')
  const [plan, setPlan] = React.useState([])
  const storageKey = `thruline-review-plan:${session.project || session.key}`
  React.useEffect(() => {
    try { const stored = JSON.parse(localStorage.getItem(storageKey) || '[]'); setPlan(Array.isArray(stored) ? stored.filter(id => catalog.checks.some(c => c.id === id)) : []) } catch { setPlan([]) }
  }, [storageKey])
  const rows = React.useMemo(() => catalogRows(session).map(row => {
    const implementation = coverage?.checks?.find(c => c.id === row.id)
    return { ...row, implementation, availability: implementation ? implementation.availability === 'implemented' ? `Implemented · ${implementation.validation_status === 'validated' ? 'validated' : 'validation pending'} · run through Local checks` : 'Specification only · no executable entry point' : row.availability }
  }), [session, coverage])
  const visible = rows.filter(r => (family === 'all' || r.family === family) && (filter === 'all' || filter === 'planned' ? filter !== 'planned' || plan.includes(r.id) : r.assessment.status === filter) && `${r.id} ${r.name} ${r.trigger} ${r.evidence}`.toLowerCase().includes(query.toLowerCase()))
  const active = rows.find(r => r.id === selected)
  function togglePlan(id) {
    const next = plan.includes(id) ? plan.filter(x => x !== id) : [...plan, id]
    try { localStorage.setItem(storageKey, JSON.stringify(next)) } catch { /* in-memory plan remains usable */ }
    setPlan(next)
  }
  if (active) {
    const r = active.result
    return <section className="ws-check-detail">
      <button className="ws-text-button" onClick={() => setSelected(null)}>← All verification checks</button>
      <div className="ws-hq-kicker">{active.id} · {catalog.families.find(f => f.id === active.family)?.name}</div>
      <h2>{active.name}</h2>
      <ResultBadge status={active.assessment.status} />
      <p className="ws-hq-muted">{active.availability}. A specification describes a check; it does not mean the check has run.</p>
      {active.implementation ? <details><summary>Implementation, dependencies and validation</summary><p>{active.implementation.implementation_scope}</p><p>Entry point: {active.implementation.executable_entry_point || 'Not implemented'}</p><p>Dependencies: {active.implementation.dependencies}</p><p>Implementation order: {active.implementation.implementation_order}</p><p>Validation: {active.implementation.validation_status}</p><pre>{JSON.stringify(active.implementation.validation_evidence, null, 2)}</pre></details> : null}
      <div className="ws-hq-subtabs" role="group" aria-label="Check details">{[['explanation', 'Meaning'], ['evidence', 'Evidence'], ['history', 'History'], ['spec', 'Full specification']].map(([key, label]) => <button key={key} aria-pressed={tab === key} onClick={() => setTab(key)}>{label}</button>)}</div>
      {tab === 'explanation' ? <>
        <CoverageNote checkId={active.id} />
        <h3>What we want to establish</h3><p>{active.passCriterion}</p>
        <h3>What we know now</h3><ul>{active.assessment.reasons.map((reason, i) => <li key={i}>{reason}</li>)}</ul>
        <h3>What needs to happen next</h3><p>{r ? 'Resolve the evidence gaps or invalidated inputs below. Keep the original result and record a new check of the affected scope.' : 'Agree on the project requirement and a measurable success criterion. Then use an implemented, validated verifier to collect the required evidence.'}</p>
        <details><summary>Evidence this check needs</summary><p>{active.evidence}</p></details>
        <button onClick={() => togglePlan(active.id)} aria-pressed={plan.includes(active.id)}>{plan.includes(active.id) ? 'Remove from review plan' : 'Add to review plan'}</button>
        <p className="ws-hq-muted">A review plan is saved in this browser. Adding a check does not accept a requirement or run a scanner.</p>
      </> : tab === 'evidence' ? <>
        <h3>Accepted requirement</h3><p>{r?.acceptance?.criterion || 'No accepted, observable success criterion is linked to this check.'}</p>
        {r?.acceptance ? <dl><dt>Accepted by</dt><dd>{r.acceptance.actor || 'Not recorded'}</dd><dt>Revision</dt><dd>{r.acceptance.revision || 'Not recorded'}</dd><dt>Acceptance event</dt><dd>{r.acceptance.event || 'Not recorded'}</dd></dl> : null}
        <h3>Does the evidence still apply?</h3>
        <div className="ws-binding-list">{BINDINGS.map(([key, label, detail]) => {
          const old = r?.bindings?.[key], now = session.verificationContext?.bindings?.[key]
          return <details key={key}><summary><span>{label}</span><span>{!old || !now ? '? Not established' : old === now ? '= Matches' : '↻ Changed'}</span></summary><p>{detail}</p><dl><dt>When checked</dt><dd><code>{old || 'Not identified'}</code></dd><dt>Current reading</dt><dd><code>{now || 'Not identified'}</code></dd></dl></details>
        })}</div>
        <h3>Who checked, and how?</h3><dl><dt>Collector</dt><dd>{r?.trust?.collector || 'Not identified'}</dd><dt>Trust boundary</dt><dd>{r?.trust?.boundary || 'Not established'}</dd><dt>Verifier</dt><dd>{r?.oracle?.identity || 'Not identified'}</dd><dt>Authority</dt><dd>{r?.oracle?.authority || 'Not established'}</dd><dt>Coverage</dt><dd>{r?.coverage?.complete === true ? 'Producer reports complete coverage for this scope' : 'Required coverage is not established'}</dd><dt>Interpretation</dt><dd>{r?.parserFidelity || 'Not recorded'}</dd></dl>
        <h3>Original sources</h3>{r?.evidence?.length ? r.evidence.map((e, i) => <div key={i} className="ws-source-row">{e.node && session.nodes.some(n => n.id === e.node) ? <button onClick={() => onInspect(e.node)}>{e.label || 'Open source in graph'} →</button> : <span>{e.label || 'Evidence reference · no graph link supplied'}</span>}<code>{e.digest} · {e.range}</code></div>) : <p>No source receipt is linked. The agent's own summary cannot fill this gap.</p>}
      </> : tab === 'history' ? <>
        <p>Earlier observations stay intact. A later check or an invalidation is a separate entry.</p>
        {!active.history.length ? <p>No results have been recorded for this check.</p> : active.history.map((old, i) => <article key={old.id || i} className="ws-history-entry"><p>Original observation: {RESULT_STATES[old.status]?.[0] || old.status}</p><ResultBadge status={assessResult(old, session.verificationContext).status} /><p>{old.completedAt || old.startedAt || 'Time not recorded'}</p><p>{old.reason || 'Original observation retained. The badge shows whether that evidence applies to this reading.'}</p><details><summary>Original record</summary><pre>{JSON.stringify(old, null, 2)}</pre></details></article>)}
        {active.assessment.invalidations.map(i => <article key={i.key} className="ws-history-entry"><ResultBadge status="stale" /><p>{i.reason}</p><details><summary>Changed identifiers</summary><code>{i.before} → {i.after}</code></details></article>)}
      </> : <>
        <dl>{[['When it applies', active.trigger], ['Method', active.method], ['Priority', active.priority + ' · implementation sequence, not finding severity'], ['Estimated cost', active.cost], ['Gate', active.gate], ['Required evidence', active.evidence], ['Pass criterion', active.passCriterion], ['Fail criterion', active.failCriterion], ['Unknown and limits', active.limits], ['Validation pair', active.validation]].map(([k, v]) => <React.Fragment key={k}><dt>{k}</dt><dd>{v}</dd></React.Fragment>)}</dl>
        <details><summary>Common evidence contract · all required fields</summary><pre>{catalog.contract}</pre></details>
      </>}
    </section>
  }
  const switcher = <div className="ws-verify-switch"><button aria-pressed={surface === 'local'} onClick={() => setSurface('local')}>Local checks</button><button aria-pressed={surface === 'checks'} onClick={() => setSurface('checks')}>134-check catalog</button><button aria-pressed={surface === 'tools'} onClick={() => setSurface('tools')}>Tool evidence</button></div>
  if (surface === 'local') return <>{switcher}<LocalVerification session={session} onInspect={onInspect} /></>
  if (surface === 'tools') return <>{switcher}<IntegrationWorkspace session={session} onCopy={onCopy} /></>
  const recorded = rows.filter(r => r.history.length)
  return <section className="ws-check-catalog">
    {switcher}
    <div className="ws-hq-kicker">VERIFY · REQUIREMENTS AND EVIDENCE</div>
    <h2>What would count as done?</h2>
    {coverage ? <p><strong>{coverage.implemented} / {coverage.total} scoped native implementations ({coverage.implementation_percent.toFixed(1)}%)</strong> · {coverage.validated} with retained historical validation · {coverage.specification_only} without a native implementation. Tool capabilities support additional portions. Current candidate qualification and runtime results are separate.</p> : <p>Implementation registry has not been read. Availability is unknown.</p>}
    <p>Choose the checks relevant to this project. Each one explains the requirement, the proof it needs, and what would invalidate a result.</p>
    <details><summary>Recorded check receipts · {(session.checks || []).length}</summary><p>These are captured runs. A receipt becomes a catalog result only when its accepted criterion, verifier, scope, and current inputs are explicitly linked.</p>{(session.checks || []).length ? session.checks.map(c => <button className="ws-source-link" key={c.id} onClick={() => onInspect(c.node)} disabled={!c.node}>{c.exit === 0 ? 'A recorded run finished successfully' : c.exit == null ? 'Run outcome is incomplete' : 'A recorded run did not pass'}<small>{c.bound ? 'Linked to the final code in this reading' : 'Final-code binding is not established'} · Open receipt →</small></button>) : <p>No check receipt was captured in this reading. This does not mean no tests ran.</p>}</details>
    <div className="ws-catalog-counts"><strong>{catalog.checks.length}</strong> check specifications <span>·</span> {catalog.families.length} families <span>·</span> {recorded.length} with results</div>
    <input aria-label="Search verification catalog" placeholder="Search memory, queues, security, tests…" value={query} onChange={e => setQuery(e.target.value)} />
    <div className="ws-catalog-filters"><select aria-label="Check family" value={family} onChange={e => setFamily(e.target.value)}><option value="all">All 15 families</option>{catalog.families.map(f => <option key={f.id} value={f.id}>{f.name} · {f.count}</option>)}</select><select aria-label="Check result filter" value={filter} onChange={e => setFilter(e.target.value)}><option value="all">All checks</option><option value="planned">My review plan ({plan.length})</option>{Object.entries(RESULT_STATES).map(([k, [label]]) => <option key={k} value={k}>{label}</option>)}</select></div>
    <p className="ws-hq-muted">{visible.length} checks shown. Missing results are not failures. The catalog does not claim that 134 scanners are implemented.</p>
    <div className="ws-catalog-list">{visible.map(c => <button key={c.id} onClick={() => { setSelected(c.id); setTab('explanation') }}><span className="ws-catalog-id">{c.id}{plan.includes(c.id) ? ' · selected' : ''}</span><strong>{c.name}</strong><span>{c.history.length ? <ResultBadge status={c.assessment.status} /> : 'No result recorded'}</span><small>{c.availability}</small></button>)}</div>
    {!visible.length ? <p>No checks match these filters. Change the family, state, or search phrase.</p> : null}
    {plan.length ? <button onClick={() => onCopy(JSON.stringify({ kind: 'thruline.review-plan', version: 1, project: session.project, checks: plan, acceptance: 'not established by this plan', source: catalog.source, sourceSha256: catalog.sourceSha256 }, null, 2), 'the review plan')}>Copy review plan · {plan.length} checks</button> : null}
    <details><summary>What the result states mean</summary><dl>{Object.entries(RESULT_STATES).map(([k, [label, meaning]]) => <React.Fragment key={k}><dt>{label} <code>{k}</code></dt><dd>{meaning}</dd></React.Fragment>)}</dl></details>
    <details><summary>Evidence contract and automatic invalidation</summary><p>A relevant change to code, dependencies, environment, requirements, verifier, adapter, data, or policy invalidates an older result. Unidentified inputs prevent a current pass.</p><pre>{catalog.contract}</pre></details>
    <details><summary>Research source and references</summary><p>{catalog.source} · research cutoff {catalog.version}</p><pre>{catalog.references}</pre><code>{catalog.sourceSha256}</code></details>
  </section>
}

function CoverageNote({ checkId }) {
  const row = integrationCoverage.checks.find(c => c.id === checkId)
  if (!row) return null
  const capabilities = integrationCoverage.capabilities.filter(c => row.capabilities.includes(c.id))
  return <details className="ws-integration-detail"><summary>What is implemented for {checkId}?</summary><p>{integrationCoverage.claim}</p><dl>{[['Command capture', row.capture], ['Report interpretation', row.interpretation], ['Freshness tracking', row.freshness], ['Full requirement verification', row.requirement_verification], ['Enabled use', row.enabled]].map(([k,v]) => <React.Fragment key={k}><dt>{k}</dt><dd>{v}</dd></React.Fragment>)}</dl>{capabilities.map(c => <article key={c.id}><h4>{c.id.replaceAll('-', ' ')}</h4><p>{c.scope}</p><p className="ws-hq-muted">{c.qualification}</p></article>)}<p>{row.pending}</p></details>
}
