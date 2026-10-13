import React from 'react'
import { buildReview } from './review-model.js'

const fmt = (n) => Number.isFinite(n) ? n.toLocaleString('en-US') : '—'

export function concernIncludesNode(concern, node) {
  return !!node && (concern.node === node || !!concern.eventRefs?.some(e => e.node === node))
}

export function investigationFor(concern) {
  if (concern.kind === 'failure') return {
    label: 'Recorded failure',
    meaning: 'Check whether the failure was expected, recovered from, or left unresolved. A later success only resolves it if it checks the same work.',
    next: 'Inspect the failed result and surrounding commands. Then find a matching retry or check on the final project state.',
    closes: 'A linked recovery or passing check for the same command and scope, or an explanation supported by the surrounding record.',
  }
  if (concern.kind === 'promise') return {
    label: 'Unmatched follow-up',
    meaning: 'This may be unfinished work or an offer that still depends on a decision. The source wording and any later completion matter.',
    next: 'Read the commitment in context, confirm whether it was accepted or still needed, and locate any later completion.',
    closes: 'A matching completion, an explicit decision to drop the follow-up, or a named dependency and owner.',
  }
  if (concern.derivedCompletion) return {
    label: 'Derived review · rules',
    meaning: 'Changes were recorded, but completion could not be established. The final-message excerpt is context, not an explicit completion claim.',
    next: 'Compare the changed files with the original request and locate checks that cover the final project state.',
    closes: 'Evidence of completed scope and matching checks, or an explicit account of what remains unfinished.',
  }
  if (concern.kind === 'finding') return {
    label: 'Detector finding',
    meaning: 'The detector raised a specific condition. Review its scope, the original requirement, and any counterevidence before deciding its impact.',
    next: concern.ask,
    closes: 'Source evidence establishing the impact, or a correction followed by a check of the affected scope.',
  }
  return {
    label: concern.provenanceLabel || (concern.kind === 'request' ? 'Request check' : 'Review item'),
    meaning: concern.detail,
    next: concern.ask || 'Inspect the source and identify what evidence would establish the outcome.',
    closes: 'Evidence tied to this exact request or claim and the project state being reviewed.',
  }
}

function EvidenceSource({ session, concern, selected, onInspect }) {
  const selectedEvent = concern.eventRefs?.find(e => e.node === selected)
  const seq = selectedEvent?.seq ?? concern.seq
  const quote = selectedEvent?.text ?? concern.quote
  const index = seq == null ? -1 : (session.events || []).findIndex(e => e.seq === seq)
  const nearby = index < 0 ? [] : session.events.slice(Math.max(0, index - 1), index + 2)
  return <div className="ws-analysis-source">
    {quote ? <blockquote><span>{concern.derivedCompletion ? 'Final message · context for the derived review' : concern.kind === 'failure' ? `Recorded result${seq == null ? '' : ` · event ${seq}`}` : 'Quoted session text'}</span>{quote}</blockquote> : null}
    {concern.provenanceDetail ? <p>{concern.provenanceDetail}</p> : null}
    {concern.reasons?.length ? <details><summary>Detector reasoning</summary><ul>{concern.reasons.map((r, i) => <li key={i}>{r}</li>)}</ul></details> : null}
    {concern.evidence?.length ? <details><summary>Linked evidence references</summary><pre>{concern.evidence.join('\n')}</pre></details> : null}
    {nearby.length ? <details><summary>Surrounding events · recorded order</summary><ol>{nearby.map(e => <li key={e.seq}><button type="button" aria-current={e.seq === seq ? 'true' : undefined} onClick={() => onInspect(`ev${e.seq}`)}>Event {e.seq}<span>{e.readText || e.text}</span></button></li>)}</ol></details> : null}
    {concern.eventRefs?.length > 1 ? <details><summary>All {concern.eventRefs.length} failed results</summary><div className="ws-analysis-event-links">{concern.eventRefs.map(e => <button type="button" key={e.seq} aria-pressed={selected === e.node} onClick={() => onInspect(e.node)}>Event {e.seq} · {e.text}</button>)}</div></details> : null}
    <details><summary>Source & provenance</summary><pre>{typeof concern.source === 'string' ? concern.source : JSON.stringify(concern.source, null, 2)}</pre>{concern.verifier ? <p>Verifier: {concern.verifier}</p> : null}</details>
  </div>
}

export function EvidenceAnalysis({ session, reading, selected, riskNotice, onShowRisk, onInspect, onDetails }) {
  const review = React.useMemo(() => buildReview(session), [session])
  const linked = review.concerns.find(c => concernIncludesNode(c, selected))
  const [expanded, setExpanded] = React.useState(linked?.id || null)
  const [query, setQuery] = React.useState('')
  const [limit, setLimit] = React.useState(12)
  // Graph selection opens its own analysis. Unrelated evidence must never
  // inherit the explanation of the previously selected object.
  React.useEffect(() => { setExpanded(linked?.id || null) }, [selected, linked?.id])
  const concerns = review.concerns.filter(c => `${c.title} ${c.detail} ${c.quote}`.toLowerCase().includes(query.toLowerCase()))
  const node = (session.nodes || []).find(n => n.id === selected)
  const active = concerns.find(c => c.id === expanded)
  const investigation = active ? investigationFor(active) : null

  if (reading) return <div className="ws-analysis-body" role="status"><h2>Reading the evidence…</h2><p>The graph and its analysis will update together when this session is ready.</p></div>

  return <div className="ws-analysis-body">
    <section className="ws-analysis-intro">
      <div className="ws-analysis-kicker">ANALYSIS OF THIS READING</div>
      <h2>What the evidence supports</h2>
      <p>Follow a review point into the graph. Check the source, then decide what needs to happen.</p>
      <div className="ws-analysis-coverage-line">{fmt(review.counts.events)} events <span>·</span> {fmt(review.counts.checks)} check receipts <span>·</span> {fmt(review.counts.concerns)} review points</div>
    </section>

    {selected ? <section className="ws-analysis-selection"><div className="ws-analysis-kicker">SELECTED IN THE GRAPH</div><strong>{node?.readTitle || node?.title || selected}</strong><p>{linked ? 'A review point is linked to this source.' : 'No review point is linked to this object. This does not certify the work.'}</p><button type="button" onClick={onDetails}>Inspect exact record →</button></section> : null}

    <section className="ws-analysis-review" aria-labelledby="ws-analysis-points">
      <div className="ws-analysis-section-head"><h3 id="ws-analysis-points">Investigate</h3><span>{review.counts.concerns} review points</span></div>
      {review.concerns.length > 6 ? <input aria-label="Find a review point" placeholder="Find in analysis…" value={query} onChange={e => { setQuery(e.target.value); setLimit(12) }} /> : null}
      <div className="ws-analysis-items">{concerns.slice(0, limit).map(c => <button type="button" className="ws-analysis-item" key={c.id} aria-expanded={expanded === c.id} onClick={() => {
        setExpanded(expanded === c.id ? null : c.id)
        if (expanded !== c.id && c.node) onInspect(c.node)
      }}><span className={'ws-analysis-marker ws-analysis-marker-' + c.tone}>{c.kind === 'failure' ? '↳' : c.kind === 'promise' ? '◌' : '◇'}</span><span><strong>{c.title}</strong>{['promise', 'request'].includes(c.kind) && c.quote ? <em>{c.quote}</em> : null}<small>{investigationFor(c).label}{c.seq != null ? ` · event ${c.seq}` : ''}</small></span><span>{expanded === c.id ? '−' : '+'}</span></button>)}</div>
      {!concerns.length ? <p>{query ? 'No review point matches this phrase.' : 'No review points were raised. Explore the record and its coverage before concluding the work is complete.'}</p> : null}
      {concerns.length > limit ? <button type="button" onClick={() => setLimit(limit + 12)}>Show {Math.min(12, concerns.length - limit)} more</button> : null}
    </section>

    {active ? <section className="ws-analysis-detail" aria-label="Selected review point">
      <div className="ws-analysis-kicker">{investigation.label}</div><h3>{active.title}</h3>
      <h4>What is recorded</h4><p>{active.detail}</p>
      <EvidenceSource session={session} concern={active} selected={selected} onInspect={onInspect} />
      <h4>What it means</h4><p>{investigation.meaning}</p>
      <div className="ws-analysis-next"><h4>Suggested next step</h4><p>{investigation.next}</p><details><summary>What would resolve this</summary><p>{investigation.closes}</p></details></div>
      {active.node ? <button type="button" onClick={() => onInspect(active.node)}>Locate source in graph →</button> : null}
      <p className="ws-analysis-method">Suggestion based on record checks. No model judgment has been added.</p>
    </section> : review.firstPoint ? <section className="ws-analysis-start"><div className="ws-analysis-kicker">EARLIEST REVIEW ANCHOR</div><h3>{review.firstPoint.title}</h3><button type="button" onClick={() => onInspect(review.firstPoint.node)}>Locate event {review.firstPoint.seq} →</button><p>A place to begin investigating. This does not establish where drift began.</p></section> : null}

    {riskNotice ? <section className="ws-analysis-risk"><details><summary>{review.counts.riskyCommands} actions flagged for review</summary><p>{riskNotice.head}</p><p>{riskNotice.basis}</p><p>{riskNotice.clause}</p><p>A pattern match does not establish that harm occurred.</p><button type="button" onClick={onShowRisk}>Show flagged actions in graph →</button></details></section> : null}
    <details className="ws-analysis-limits"><summary>Coverage & limits of this reading</summary><ul>{review.coverage.map((line, i) => <li key={i}>{line}</li>)}</ul><p>Hidden intent, deception, hallucination, and wasted time are not established by these review points. Those conclusions need specific supporting evidence.</p><code>{session.key || session.id}</code></details>
  </div>
}
