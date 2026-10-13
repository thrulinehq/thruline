import React from 'react'
import {
  FINDING_CATEGORIES,
  MIND_LOOPBACK,
  MIND_MODEL,
  MIND_MODEL_DIGEST,
  RELATION_LABELS,
  candidateCoverage,
  findingGroupId,
  mindAvailabilityPresentation,
} from '../lib/mind.js'
import {
  artifactDetailId,
  recordedArtifactRegionId,
  recordedDetailId,
  sessionCategorySupport,
  unresolvedArtifactWindow,
} from '../lib/category-support.js'
import { receiptRegionId, receiptWindow } from '../lib/receipt-window.js'
import { languageShadowProjection } from '../lib/language-shadow.js'
import {
  ENTITY_ASSOCIATION_PAGE,
  entityAssociationProjection,
  entityAssociationWindow,
} from '../lib/entity-associations.js'
import { workMap } from '../lib/work-map.js'
import { countPhrase } from '../lib/voice.js'

const CATEGORY_LABELS = Object.freeze({
  happened: 'What happened',
  missing: 'What is missing',
  lagging: 'What may be lagging',
  contradictory: 'What is contradictory',
})

const LANGUAGE_CATEGORY_LABELS = Object.freeze({
  completed_claim: 'Completed claim',
  proposal: 'Proposal',
  finding: 'Finding',
  micro_intent: 'Micro-intent',
  guidance: 'Guidance',
  procedure: 'Procedure',
  question: 'Question',
  narration: 'Narration',
  uncertain: 'Uncertain',
})
const LANGUAGE_SPEECH_LABELS = Object.freeze({
  none: 'None explicit',
  proposal: 'Proposal',
  decision: 'Decision',
  refusal: 'Refusal',
  deferral: 'Deferral',
})
const LANGUAGE_ROW_PAGE = 8

function RelationLabel({ kind, detail }) {
  return (
    <span className={`wm-edge-label ${kind}`} data-relation={RELATION_LABELS[kind]}>
      <strong>{RELATION_LABELS[kind]}</strong>
      {detail ? <span>{detail}</span> : null}
    </span>
  )
}

function sessionFacts(session) {
  const facts = [session.date].filter(Boolean)
  if (session.msgs != null) facts.push(countPhrase(session.msgs, 'message', 'messages'))
  return facts.join(' · ')
}

function ReceiptDoor({ anchor, label, onSelect }) {
  if (!anchor?.eventId) return <span className="wm-muted">No source event was exported for this item</span>
  if (!anchor.nodeId) {
    return (
      <span className="wm-anchor">
        <span>{label}</span>
        <span className="wm-muted">An exact claim receipt is unavailable</span>
      </span>
    )
  }
  return (
    <span className="wm-anchor">
      <span className="wm-anchor-label">
        <RelationLabel kind="exact_match" />
        <span>{label}</span>
      </span>
      <button type="button" className="wm-receipt" onClick={() => onSelect(anchor.nodeId)}>
        Open exact receipt
      </button>
    </span>
  )
}

function ExpandableText({ preview, label = 'Show all exported text' }) {
  const [open, setOpen] = React.useState(false)
  return (
    <>
      <p className="wm-record-text">{open ? preview.fullText : preview.text}</p>
      {preview.truncated ? (
        <button type="button" className="wm-expand" onClick={() => setOpen((value) => !value)} aria-expanded={open}>
          {open ? 'Show less' : label}
        </button>
      ) : null}
    </>
  )
}

function FileDisplay({ presentation }) {
  return (
    <span className={`wm-file${presentation?.outside ? ' outside' : ''}`}>
      <span>{presentation?.label || 'Workspace relation unproven'}</span>
      <code>{presentation?.display || 'No file path exported'}</code>
    </span>
  )
}

function OpeningAsk({ item, onSelect }) {
  const [open, setOpen] = React.useState(false)
  const text = open ? item.text.fullText : item.text.text
  return (
    <article className="wm-item wm-ask">
      <header className="wm-item-head">
        <span className="wm-kind">{item.excerpt ? 'Opening ask excerpt' : 'Opening ask'}</span>
        {item.continuation ? <span className="wm-muted">Continues an earlier run</span> : null}
      </header>
      <p className="wm-record-text">{text}</p>
      {item.text.truncated ? (
        <button type="button" className="wm-expand" onClick={() => setOpen((value) => !value)} aria-expanded={open}>
          {open ? 'Show less' : 'Show all exported text'}
        </button>
      ) : null}
      {item.excerpt ? <span className="wm-muted">The export ends this excerpt here; it does not include additional source bytes.</span> : null}
      <ReceiptDoor anchor={item.anchor} label="Exact opening event" onSelect={onSelect} />
    </article>
  )
}

function WorkCluster({ item, onSelect }) {
  return (
    <article id={`wm-${item.key}`} className="wm-item wm-cluster">
      <header className="wm-item-head">
        <span className="wm-kind">Work cluster</span>
        <span className="wm-count">{countPhrase(item.edits, 'recorded edit', 'recorded edits')}</span>
      </header>
      <FileDisplay presentation={item.filePresentation} />
      <div className="wm-anchor-pair">
        <ReceiptDoor anchor={item.first} label="Exact first event" onSelect={onSelect} />
        <ReceiptDoor anchor={item.last} label="Exact last event" onSelect={onSelect} />
      </div>
      {item.artifactMatches.length > 0 ? (
        <p className="wm-relation">
          <RelationLabel kind="exact_match" detail="File-path equality" />
          <span>This cluster links to {countPhrase(item.artifactMatches.length, 'output record', 'output records')}.</span>
        </p>
      ) : null}
      <div className="wm-interpretation">{item.interpretationLabel}</div>
    </article>
  )
}

function Boundary({ item, onSelect }) {
  return (
    <article className="wm-item wm-boundary">
      <header className="wm-item-head">
        <span className="wm-kind">Recorded commit boundary</span>
      </header>
      <ExpandableText preview={item.message} />
      <ReceiptDoor anchor={item.anchor} label="Exact boundary event" onSelect={onSelect} />
    </article>
  )
}

function Tail({ item, onSelect }) {
  return (
    <article className="wm-item wm-tail">
      <header className="wm-item-head">
        <span className="wm-kind">Uncommitted tail</span>
        <span className="wm-count">{countPhrase(item.edits, 'recorded edit', 'recorded edits')}</span>
      </header>
      {item.filePresentations.length > 0 ? (
        <ul className="wm-file-list">
          {item.filePresentations.map((presentation, index) => (
            <li key={`${item.key}:file:${index}`}><FileDisplay presentation={presentation} /></li>
          ))}
        </ul>
      ) : <span className="wm-muted">No file paths were exported for this tail</span>}
      <ReceiptDoor anchor={item.anchor} label="Exact tail event" onSelect={onSelect} />
      <div className="wm-interpretation">{item.interpretationLabel}</div>
    </article>
  )
}

function Artifact({ item, onSelect }) {
  const relationTotal = item.exactPath.clusterMatches.length + item.exactPath.evidenceMatches.length
  return (
    <article id={artifactDetailId(item.key)} className="wm-item wm-artifact">
      <header className="wm-item-head">
        <span className="wm-kind">Artifact / output</span>
        <span className="wm-count">{countPhrase(item.edits, 'recorded edit', 'recorded edits')}</span>
      </header>
      <FileDisplay presentation={item.filePresentation} />
      {item.sessions.length > 0 ? <span className="wm-muted">Recorded in {countPhrase(item.sessions.length, 'run', 'runs')}</span> : null}
      {relationTotal > 0 ? (
        <div className="wm-branch">
          <RelationLabel kind="exact_match" detail="File-path equality" />
          {item.exactPath.clusterMatches.map((match) => (
            <a key={match.key} href={`#wm-${match.key}`}>Matching work cluster</a>
          ))}
          {item.exactPath.evidenceMatches.map((match) => (
            <button key={`${match.nodeId}:${match.evidenceIndex}`} type="button" onClick={() => onSelect(match.nodeId)}>
              Matching receipt evidence
            </button>
          ))}
        </div>
      ) : <span className="wm-muted">No exact file-path relation was measured</span>}
    </article>
  )
}

function CitationBranch({ row, onSelect }) {
  const [expanded, setExpanded] = React.useState(false)
  const window = receiptWindow(row.receipts, expanded)
  const regionId = receiptRegionId(row.id)

  function toggleReceipts(event) {
    event.currentTarget.focus()
    setExpanded((value) => !value)
  }

  return (
    <div className="wm-citation-branch">
      <RelationLabel kind="exact_match" detail="Server-supplied citation" />
      <div className="wm-cited-facts">
        {row.facts.map((fact) => (
          <blockquote key={fact.id} className="wm-cited-fact">
            <span>Cited exported fact</span>
            <p>{fact.text}</p>
          </blockquote>
        ))}
      </div>
      {row.receipts.length > 0 ? (
        <div className="wm-receipt-disclosure">
          <div id={regionId} className="wm-cited-receipts" aria-label={`Exact receipts supplied for this row · showing ${window.items.length} of ${window.total}`}>
            {window.items.map(({ receipt, visibleLabel, ariaLabel }) => (
              <button key={receipt.handle} type="button" className="wm-receipt wm-positioned-receipt" aria-label={ariaLabel} onClick={() => onSelect(receipt.node)}>
                <span>{visibleLabel}</span>
                <strong>Open exact receipt</strong>
              </button>
            ))}
          </div>
          {window.hasDisclosure ? (
            <button
              type="button"
              className="wm-expand wm-receipt-toggle"
              aria-expanded={window.expanded}
              aria-controls={regionId}
              onClick={toggleReceipts}
            >
              {window.controlLabel}
            </button>
          ) : null}
        </div>
      ) : <span className="wm-muted">No exact receipt was supplied for this cited row.</span>}
    </div>
  )
}

function ProposedRow({ row, type, onSelect }) {
  return (
    <article className="wm-model-row">
      <header>
        <RelationLabel kind="model_proposed" />
        <span>{type}</span>
      </header>
      <p>{row.text}</p>
      {row.action ? <span className="wm-action-class">Action class · {row.action.replaceAll('_', ' ')}</span> : null}
      <CitationBranch row={row} onSelect={onSelect} />
    </article>
  )
}

function RecordedHappenedSupport({ support }) {
  return (
    <section className="wm-category-source" aria-label="Recorded support at export">
      <header>
        <RelationLabel kind="recorded" detail="Exported candidate structure" />
        <h5>Recorded structure at export</h5>
      </header>
      <p>
        At export, this run groups {countPhrase(support.clusters, 'work cluster', 'work clusters')} and{' '}
        {countPhrase(support.boundaries, 'recorded commit boundary', 'recorded commit boundaries')}.
      </p>
      <a href={support.detailHref}>Open recorded candidate detail</a>
    </section>
  )
}

function RecordedMissingSupport({ session, support }) {
  const [expanded, setExpanded] = React.useState(false)
  const window = unresolvedArtifactWindow(support.artifacts, expanded)
  const regionId = recordedArtifactRegionId(session)

  function toggleArtifacts(event) {
    event.currentTarget.focus()
    setExpanded((value) => !value)
  }

  return (
    <section className="wm-category-source" aria-label="Recorded unresolved signals at export">
      <header>
        <RelationLabel kind="recorded" detail="Exact exported run membership" />
        <h5>Recorded unresolved signals at export</h5>
      </header>
      <p>
        At export, this run has {countPhrase(support.tails, 'uncommitted-tail candidate', 'uncommitted-tail candidates')} and{' '}
        {countPhrase(support.artifacts.length, 'project unresolved-artifact candidate', 'project unresolved-artifact candidates')} associated with this exact exported run.
      </p>
      <p className="wm-support-caution">These are recorded unresolved signals, not proof of missing work.</p>
      {support.tails > 0 ? <a href={support.detailHref}>Open recorded uncommitted-tail detail</a> : null}
      {window.total > 0 ? (
        <div className="wm-artifact-support-disclosure">
          <ol
            id={regionId}
            className="wm-artifact-support-list"
            aria-label={`Recorded unresolved artifact candidates · showing ${window.shown} of ${window.total}`}
          >
            {window.items.map((item) => (
              <li key={item.key}>
                <a href={item.href}>
                  <FileDisplay presentation={item.filePresentation} />
                </a>
                {item.exactPathRelation ? <RelationLabel kind="exact_match" detail="Exported file-path equality" /> : null}
              </li>
            ))}
          </ol>
          {window.hasDisclosure ? (
            <button
              type="button"
              className="wm-expand wm-artifact-support-toggle"
              aria-expanded={window.expanded}
              aria-controls={regionId}
              onClick={toggleArtifacts}
            >
              {window.controlLabel}
            </button>
          ) : null}
        </div>
      ) : <p className="wm-muted">No exact-session unresolved artifact candidate was exported.</p>}
    </section>
  )
}

function RecordedUnsupportedSupport({ category }) {
  const relation = category === 'lagging' ? 'lagging' : 'contradiction'
  return (
    <section className="wm-category-source" aria-label={`Recorded ${relation} relation at export`}>
      <header>
        <RelationLabel kind="recorded" detail="Exported candidate structure" />
        <h5>Recorded relation at export</h5>
      </header>
      <p>This candidate export represents no deterministic {relation} relation for this run. This is not proof of absence.</p>
    </section>
  )
}

function RecordedCategorySupport({ session, category, support }) {
  if (support.status !== 'ready') {
    return <p className="wm-mind-empty">Recorded support could not be scoped to exactly one exported run.</p>
  }
  if (category === 'happened') return <RecordedHappenedSupport support={support.happened} />
  if (category === 'missing') return <RecordedMissingSupport session={session} support={support.missing} />
  return <RecordedUnsupportedSupport category={category} />
}

function FindingGroup({ session, category, support, analysisAvailable, rows, onSelect }) {
  const headingId = findingGroupId(session, category)
  return (
    <section className="wm-finding-group" aria-labelledby={headingId}>
      <header>
        <h4 id={headingId}>{CATEGORY_LABELS[category]}</h4>
      </header>
      <RecordedCategorySupport session={session} category={category} support={support} />
      <section className="wm-category-model" aria-label="Model-proposed interpretation">
        <header>
          <RelationLabel kind="model_proposed" detail="Shadow interpretation" />
          <h5>Model-proposed interpretation</h5>
          <span>{analysisAvailable ? countPhrase(rows.length, 'surviving row', 'surviving rows') : 'Not analyzed locally'}</span>
        </header>
        {!analysisAvailable ? (
          <p className="wm-mind-empty">Not analyzed locally. No local-model row is shown for this category.</p>
        ) : rows.length > 0 ? rows.map((row) => (
          <ProposedRow key={row.id} row={row} type="Local-model finding at export" onSelect={onSelect} />
        )) : (
          <p className="wm-mind-empty">No valid local-model row survived for this category. This is not proof of absence.</p>
        )}
      </section>
    </section>
  )
}

function Abstention({ job }) {
  if (!job.abstained) return null
  return <p className="wm-abstention">The local model abstained for this job at export; no conclusion is substituted.</p>
}

function RejectedCount({ job }) {
  return (
    <span className="wm-rejected">
      {job.rejectedRows === 0
        ? 'No model rows were rejected by the grounded checks.'
        : `${countPhrase(job.rejectedRows, 'model row', 'model rows')} rejected by the grounded checks.`}
    </span>
  )
}

function Provenance({ analysis }) {
  const p = analysis.provenance
  return (
    <details className="wm-provenance">
      <summary>Local interpretation provenance</summary>
      <div className="wm-provenance-badges">
        <span>At export</span>
        <span>Local only</span>
        <span>No API key</span>
        <span>No usage cost</span>
        <span>Shadow — cannot change the record</span>
        <span>{analysis.cached ? 'Cache hit' : 'Fresh result'}</span>
      </div>
      <dl>
        <div><dt>Exported at</dt><dd>{p.generatedAt}</dd></div>
        <div><dt>Source digest</dt><dd><code>{p.sourceDigest}</code></dd></div>
        <div><dt>Output digest</dt><dd><code>{p.outputDigest}</code></dd></div>
        <div><dt>Local model</dt><dd>{p.model}</dd></div>
        <div><dt>Model digest</dt><dd><code>{p.modelDigest}</code></dd></div>
        <div><dt>Contract</dt><dd>{p.contractVersion}</dd></div>
        <div><dt>Prompt contract</dt><dd>{p.promptVersion}</dd></div>
        <div><dt>Schema contract</dt><dd>{p.schemaVersion}</dd></div>
      </dl>
    </details>
  )
}

function MindState({ status, run, id }) {
  if (run?.state === 'analyzing') return <p id={id} className="wm-mind-state" role="status">Analyzing this exported run with the fixed local model…</p>
  if (run?.state === 'success') return <p id={id} className="wm-mind-state available" role="status">A validated local interpretation is shown for this exported run.</p>
  if (run && run.state !== 'success') return <p id={id} className={`wm-mind-state ${run.state}`} role="alert">{run.message}</p>
  const availability = mindAvailabilityPresentation(status) || mindAvailabilityPresentation({ state: 'idle' })
  return (
    <p id={id} className={`wm-mind-state ${availability.state}`} role={availability.checking ? 'status' : undefined}>
      {availability.message}
    </p>
  )
}

function MindRecovery({ status, statusId, onCheckMind }) {
  const availability = mindAvailabilityPresentation(status)
  if (!availability?.showRecovery) return null
  return (
    <details className="wm-mind-recovery">
      <summary>Local analysis setup</summary>
      <div className="wm-mind-recovery-body">
        <p><strong>Optional local shadow interpretation.</strong> No cloud API, account, or API key is used.</p>
        <dl>
          <div><dt>Ollama</dt><dd>Already running by the operator on <code>{MIND_LOOPBACK}</code></dd></div>
          <div><dt>Fixed model</dt><dd><code>{MIND_MODEL}</code></dd></div>
          <div><dt>Expected digest</dt><dd><code>{MIND_MODEL_DIGEST}</code></dd></div>
        </dl>
        <p>ThruLine does not download, pull, start, update, or substitute models.</p>
        <p>Unavailable intentionally does not distinguish an absent service, absent model, or digest mismatch.</p>
        <button
          type="button"
          className="wm-recheck"
          disabled={availability.checking}
          aria-describedby={statusId}
          onClick={() => onCheckMind()}
        >
          {availability.checking ? 'Checking…' : 'Check again'}
        </button>
      </div>
    </details>
  )
}

function InterpretationStage({ group, map, mindStatus, run, onAnalyze, onCheckMind, onSelect }) {
  const coverage = candidateCoverage(map, group.session.id)
  const recordedSupport = sessionCategorySupport(map, group.session.id)
  const analysis = run?.state === 'success' ? run.analysis : null
  const analyzing = run?.state === 'analyzing'
  const analysisReady = mindStatus.state === 'available'
  const statusId = `${findingGroupId(group.session.id, 'happened')}-availability`
  return (
    <section className="wm-stage wm-interpret-stage" aria-labelledby={`wm-interpret-${group.session.id}`}>
      <header className="wm-stage-head">
        <div>
          <span className="wm-stage-relations">
            <RelationLabel kind="recorded" detail="Deterministic support" />
            <RelationLabel kind="model_proposed" detail="Optional shadow" />
          </span>
          <h3 id={`wm-interpret-${group.session.id}`}>Category support and local interpretation</h3>
          <p>Recorded candidate support comes first; optional model-proposed navigation cannot alter candidate or receipt truth.</p>
        </div>
        <button
          type="button"
          className="wm-analyze"
          disabled={!analysisReady || analyzing}
          aria-describedby={statusId}
          onClick={() => onAnalyze(group.session.id, coverage)}
        >
          {analyzing ? 'Analyzing…' : !analysisReady ? 'Analyze unavailable — check local model' : analysis ? 'Analyze this run again' : 'Analyze this exported run'}
        </button>
      </header>

      <div className="wm-analysis">
        <MindState status={mindStatus} run={run} id={statusId} />
        <MindRecovery status={mindStatus} statusId={statusId} onCheckMind={onCheckMind} />
        {analysis ? (
          <div className="wm-evidence-currency">
            <strong>At export</strong>
            <span>{analysis.provenance.generatedAt}</span>
            <code>{analysis.provenance.sourceDigest}</code>
          </div>
        ) : null}
        <p className="wm-trust-line">Local only · No API key · No usage cost · Shadow — cannot change the record</p>
        {!analysis ? (
          <div className="wm-not-analyzed">
            <strong>Not analyzed locally</strong>
            <p>Recorded support remains visible below; no local-model findings, decisions, or next steps are being shown for this run.</p>
            <span className="wm-model-fixed">Fixed local model · {MIND_MODEL}</span>
          </div>
        ) : null}
        <div className="wm-analysis-section">
          <div className="wm-analysis-heading">
            <div><h3>Four category branches</h3><p>Recorded support first; model-proposed interpretation stays separate and scoped at export.</p></div>
            {analysis ? <RejectedCount job={analysis.narration} /> : null}
          </div>
          {analysis ? <Abstention job={analysis.narration} /> : null}
          <div className="wm-finding-branches">
            {FINDING_CATEGORIES.map((category) => (
              <FindingGroup
                key={category}
                session={group.session.id}
                category={category}
                support={recordedSupport}
                analysisAvailable={Boolean(analysis)}
                rows={analysis?.categories[category] || []}
                onSelect={onSelect}
              />
            ))}
          </div>
        </div>

        {analysis ? (
          <div className="wm-analysis-section">
            <div className="wm-analysis-heading">
              <div><h3>Model-proposed · not a decision</h3><p>Suggestions only; the deterministic record remains unchanged.</p></div>
              <RejectedCount job={analysis.proposal} />
            </div>
            <Abstention job={analysis.proposal} />
            {analysis.decisions.length + analysis.nextSteps.length === 0 && !analysis.proposal.abstained ? (
              <p className="wm-mind-empty">No valid local-model proposal survived. This is not proof that no decision or next step exists.</p>
            ) : null}
            <div className="wm-proposal-list">
              {analysis.decisions.map((row) => <ProposedRow key={row.id} row={row} type="Model-proposed · not a decision" onSelect={onSelect} />)}
              {analysis.nextSteps.map((row) => <ProposedRow key={row.id} row={row} type="Model-proposed next step" onSelect={onSelect} />)}
            </div>
          </div>
        ) : null}
        {analysis ? <Provenance analysis={analysis} /> : null}
      </div>
    </section>
  )
}

function ArtifactGroup({ title, items, onSelect }) {
  if (items.length === 0) return null
  return (
    <section className="wm-artifact-group">
      <div className="wm-group-head">
        <h3>{title}</h3>
        <span>{countPhrase(items.length, 'artifact', 'artifacts')}</span>
      </div>
      <div className="wm-items">
        {items.map((item) => <Artifact key={item.key} item={item} onSelect={onSelect} />)}
      </div>
    </section>
  )
}

function RecordedSummary({ group, map }) {
  const coverage = candidateCoverage(map, group.session.id)
  const recordedTotal = coverage.work_clusters + coverage.checkpoints + coverage.uncommitted_tails
  return (
    <section className="wm-stage wm-recorded-summary" aria-label="Recorded run summary stage">
      <header className="wm-stage-head">
        <div>
          <RelationLabel kind="recorded" detail="Canonical export grouping" />
          <h3>Recorded run summary</h3>
          <p>The local interpretation action follows this compact inventory. Every recorded candidate stays available in the detail disclosure below.</p>
        </div>
      </header>
      <div className="wm-run-counts" aria-label="Recorded candidate counts for this run">
        <span>{countPhrase(coverage.work_clusters, 'work cluster', 'work clusters')}</span>
        <span>{countPhrase(coverage.checkpoints, 'commit boundary', 'commit boundaries')}</span>
        <span>{countPhrase(coverage.uncommitted_tails, 'uncommitted tail', 'uncommitted tails')}</span>
        <span>{countPhrase(coverage.artifacts, 'linked artifact candidate', 'linked artifact candidates')}</span>
        <span>{countPhrase(recordedTotal, 'recorded detail card', 'recorded detail cards')}</span>
      </div>
    </section>
  )
}

function LanguageShadowRow({ row, onSelect }) {
  return (
    <article className={`wm-language-row ${row.category}`}>
      <header>
        <span className="wm-language-category">Category · {LANGUAGE_CATEGORY_LABELS[row.category]}</span>
        <span>Speech act · {LANGUAGE_SPEECH_LABELS[row.speechAct]}</span>
      </header>
      {row.label ? <strong>{row.label}</strong> : <span className="wm-muted">No safe navigation label was exported.</span>}
      <span className="wm-language-attempts">
        {row.attempts === 0 ? 'No provider attempt recorded' : countPhrase(row.attempts, 'provider attempt', 'provider attempts')}
      </span>
      {row.receipt.available ? (
        <button type="button" className="wm-receipt" onClick={() => onSelect(row.receipt.nodeId)}>
          Open exact receipt
        </button>
      ) : <span className="wm-muted">Exact receipt unavailable</span>}
      <details className="wm-language-row-provenance">
        <summary>Row provenance</summary>
        <dl>
          <div><dt>Annotation receipt</dt><dd><code>{row.provenance.annotationId}</code></dd></div>
          <div><dt>Prompt digest</dt><dd><code>{row.provenance.promptDigest}</code></dd></div>
          <div><dt>Output digest</dt><dd><code>{row.provenance.outputDigest}</code></dd></div>
        </dl>
      </details>
    </article>
  )
}

function LanguageShadowProvenance({ projection }) {
  const run = projection.run
  return (
    <details className="wm-language-provenance">
      <summary>Exported language shadow provenance</summary>
      <dl>
        <div><dt>Exported at</dt><dd>{run.createdAt}</dd></div>
        <div><dt>Model</dt><dd>{run.model}</dd></div>
        <div><dt>Model digest</dt><dd><code>{run.modelDigest}</code></dd></div>
        <div><dt>Prompt revision</dt><dd>{run.promptRevision}</dd></div>
        <div><dt>Extractor</dt><dd>{run.extractor}</dd></div>
        <div><dt>Run prompt digest</dt><dd><code>{run.promptDigest}</code></dd></div>
        <div><dt>Run output digest</dt><dd><code>{run.outputDigest}</code></dd></div>
      </dl>
    </details>
  )
}

function LanguageShadowStage({ projection, sessionId, showInvalid, onSelect }) {
  const [visibleRows, setVisibleRows] = React.useState(LANGUAGE_ROW_PAGE)

  if (projection.status === 'absent') return null
  if (projection.status === 'invalid') {
    if (!showInvalid) return null
    return (
      <section className="wm-stage wm-language-stage unavailable" aria-label="Exported language shadow unavailable">
        <header className="wm-stage-head">
          <div>
            <RelationLabel kind="model_proposed" detail="Unavailable" />
            <h3>Exported language shadow unavailable</h3>
            <p>The optional language block did not match the guarded export contract. No labels or receipt links are shown; the deterministic Work Map remains available.</p>
          </div>
        </header>
      </section>
    )
  }

  const session = projection.sessions.find((candidate) => candidate.sessionId === sessionId)
  if (!session) return null
  const rows = session.rows.slice(0, visibleRows)
  const coverage = projection.coverage
  const regionId = `${session.key}-rows`
  const remaining = session.rows.length - rows.length

  function showMore(event) {
    event.currentTarget.focus()
    setVisibleRows((count) => Math.min(session.rows.length, count + LANGUAGE_ROW_PAGE))
  }

  function showFewer(event) {
    event.currentTarget.focus()
    setVisibleRows(LANGUAGE_ROW_PAGE)
  }

  return (
    <section className="wm-stage wm-language-stage" aria-labelledby={`${session.key}-heading`}>
      <header className="wm-stage-head">
        <div>
          <RelationLabel kind="model_proposed" detail="Exported navigation only" />
          <h3 id={`${session.key}-heading`}>Exported language navigation</h3>
          <p className="wm-language-trust">Exported language shadow · at export · cannot change the record</p>
          <p>This is the optional classifier layer carried by the export, separate from deterministic candidate truth and the on-demand local interpretation below.</p>
        </div>
      </header>

      <div className="wm-language-coverage" aria-label="Exported language shadow coverage">
        <span>{countPhrase(session.rows.length, 'row in this run', 'rows in this run')}</span>
        <span>{countPhrase(coverage.items, 'candidate across the export', 'candidates across the export')}</span>
        <span>{coverage.classified} classified</span>
        <span>{coverage.uncertain} uncertain</span>
        <span>{coverage.labeled} safely labeled</span>
        <span>{countPhrase(coverage.acceptedClaims, 'accepted deterministic claim', 'accepted deterministic claims')}</span>
        <span>{countPhrase(coverage.attempts, 'provider attempt', 'provider attempts')}</span>
      </div>

      {session.rows.length > 0 ? (
        <div className="wm-language-disclosure">
          <div id={regionId} className="wm-language-rows" aria-label={`Exported language rows · showing ${rows.length} of ${session.rows.length}`}>
            {rows.map((row) => <LanguageShadowRow key={row.key} row={row} onSelect={onSelect} />)}
          </div>
          {remaining > 0 ? (
            <button type="button" className="wm-expand" aria-expanded={false} aria-controls={regionId} onClick={showMore}>
              Show next {Math.min(LANGUAGE_ROW_PAGE, remaining)} language rows
            </button>
          ) : session.rows.length > LANGUAGE_ROW_PAGE ? (
            <button type="button" className="wm-expand" aria-expanded={true} aria-controls={regionId} onClick={showFewer}>
              Show fewer language rows
            </button>
          ) : null}
        </div>
      ) : <p className="wm-muted">No language rows were exported for this run.</p>}

      <LanguageShadowProvenance projection={projection} />
    </section>
  )
}

function AssociationReceiptDoor({ receipt, associationPosition, associationTotal, onSelect }) {
  const position = `Receipt ${receipt.position} of 2`
  if (!receipt.available) {
    return (
      <span className="wm-association-receipt unavailable">
        <strong>{position}</strong>
        <span>Exact receipt unavailable</span>
      </span>
    )
  }
  return (
    <button
      type="button"
      className="wm-association-receipt"
      aria-label={`Association ${associationPosition} of ${associationTotal} · ${position} · Open exact receipt`}
      onClick={() => onSelect(receipt.nodeId)}
    >
      <strong>{position}</strong>
      <span>Open exact receipt</span>
    </button>
  )
}

function EntityAssociationBridge({ row, total, onSelect }) {
  return (
    <article className="wm-association-bridge">
      <div className="wm-association-route">
        <AssociationReceiptDoor receipt={row.receipts[0]} associationPosition={row.position} associationTotal={total} onSelect={onSelect} />
        <span className="wm-association-connector" aria-hidden="true">
          <span />
          <strong>Operator-attested</strong>
          <span />
        </span>
        <AssociationReceiptDoor receipt={row.receipts[1]} associationPosition={row.position} associationTotal={total} onSelect={onSelect} />
      </div>
      <details className="wm-association-provenance">
        <summary>Decision provenance</summary>
        <dl>
          <div><dt>Operator decision</dt><dd>Associate</dd></div>
          <div><dt>Decided at</dt><dd>{row.decisionTime}</dd></div>
        </dl>
      </details>
    </article>
  )
}

function EntityAssociationStage({ projection, onSelect }) {
  const [visibleRows, setVisibleRows] = React.useState(ENTITY_ASSOCIATION_PAGE)
  if (projection.status === 'absent') return null

  if (projection.status === 'invalid') {
    return (
      <section className="wm-associations unavailable" aria-labelledby="wm-associations-heading">
        <header className="wm-session-head">
          <div>
            <span className="wm-eyebrow">Project-level exported navigation</span>
            <h2 id="wm-associations-heading">Operator-attested association</h2>
            <span className="wm-association-edge-label">Unavailable</span>
          </div>
        </header>
        <p className="wm-association-unavailable">The optional association block did not match the guarded export contract. No association or receipt links are shown; the deterministic Work Map remains available.</p>
      </section>
    )
  }

  const window = entityAssociationWindow(projection.rows, visibleRows)
  const regionId = 'wm-entity-association-rows'
  function showMore(event) {
    event.currentTarget.focus()
    setVisibleRows((count) => Math.min(projection.rows.length, count + ENTITY_ASSOCIATION_PAGE))
  }
  function showFewer(event) {
    event.currentTarget.focus()
    setVisibleRows(ENTITY_ASSOCIATION_PAGE)
  }

  return (
    <section className="wm-associations" aria-labelledby="wm-associations-heading">
      <header className="wm-session-head">
        <div>
          <span className="wm-eyebrow">Project-level exported navigation</span>
          <h2 id="wm-associations-heading">Operator-attested association</h2>
          <p className="wm-association-trust">Navigation only · not identity, merge, or causality</p>
        </div>
        <span>{countPhrase(projection.rows.length, 'association', 'associations')} · each links two exact subjects</span>
      </header>
      <div className="wm-association-body">
        <span className="wm-association-edge-label">Operator-attested</span>
        <div id={regionId} className="wm-association-list" aria-label={`Operator-attested associations · showing ${window.rows.length} of ${window.total}`}>
          {window.rows.map((row) => <EntityAssociationBridge key={row.key} row={row} total={window.total} onSelect={onSelect} />)}
        </div>
        {window.remaining > 0 ? (
          <button type="button" className="wm-expand wm-association-toggle" aria-expanded={false} aria-controls={regionId} onClick={showMore}>
            Show next {Math.min(ENTITY_ASSOCIATION_PAGE, window.remaining)} associations
          </button>
        ) : projection.rows.length > ENTITY_ASSOCIATION_PAGE ? (
          <button type="button" className="wm-expand wm-association-toggle" aria-expanded={true} aria-controls={regionId} onClick={showFewer}>
            Show fewer associations
          </button>
        ) : null}
      </div>
    </section>
  )
}

function RecordedDetail({ group, onSelect }) {
  const total = group.clusters.length + group.boundaries.length + group.tails.length
  return (
    <details id={recordedDetailId(group.session.id)} className="wm-stage wm-recorded-detail">
      <summary>
        <span>
          <RelationLabel kind="recorded" detail="Canonical export order" />
          <strong>Recorded candidate detail</strong>
        </span>
        <span>{countPhrase(total, 'card', 'cards')} · expand to inspect</span>
      </summary>
      <p>Work clusters, commit boundaries, and any unresolved tail remain in exported order.</p>
      <div className="wm-stage-cards">
        {group.clusters.map((item) => <WorkCluster key={item.key} item={item} onSelect={onSelect} />)}
        {group.boundaries.map((item) => <Boundary key={item.key} item={item} onSelect={onSelect} />)}
        {group.tails.map((item) => <Tail key={item.key} item={item} onSelect={onSelect} />)}
        {total === 0 ? <div className="wm-empty">No recorded work candidates were exported for this run.</div> : null}
      </div>
    </details>
  )
}

export default function WorkMap({
  fixture,
  mindStatus = { state: 'idle' },
  mindRuns = {},
  onAnalyze = () => {},
  onCheckMind = () => {},
  onSelect,
  onAllEvents,
  onDistilled,
}) {
  const map = React.useMemo(() => workMap(fixture), [fixture])
  const languageShadow = React.useMemo(() => languageShadowProjection(fixture), [fixture])
  const entityAssociations = React.useMemo(() => entityAssociationProjection(fixture), [fixture])

  if (map.status !== 'ready') {
    return (
      <div className="wm-scroll tl-scroll">
        <div className="wm-empty">This export does not contain the candidate structure needed for this view.</div>
      </div>
    )
  }

  const itemTotal = map.counts.openingAsks + map.counts.clusters + map.counts.boundaries + map.counts.tails + map.counts.artifacts
  return (
    <div className="wm-scroll tl-scroll">
      <div className="wm-page">
        <header className="wm-page-head">
          <div>
            <p className="wm-eyebrow">Deterministic candidate structure</p>
            <h1>What the record shows</h1>
            <p>Project → recorded runs → exported work structure → separate local interpretation. Lines group and navigate; they never claim causality.</p>
          </div>
          <div className="wm-actions">
            <button type="button" className="wm-primary-action" onClick={onAllEvents}>All events ({map.eventCount})</button>
            <button type="button" onClick={onDistilled}>Distilled reference</button>
          </div>
        </header>

        <div className="wm-summary" aria-label="Candidate coverage">
          <span>{countPhrase(map.counts.openingAsks, 'opening ask', 'opening asks')}</span>
          <span>{countPhrase(map.counts.clusters, 'work cluster', 'work clusters')}</span>
          <span>{countPhrase(map.counts.boundaries, 'commit boundary', 'commit boundaries')}</span>
          <span>{countPhrase(map.counts.tails, 'uncommitted tail', 'uncommitted tails')}</span>
          <span>{countPhrase(map.counts.artifacts, 'artifact', 'artifacts')}</span>
          <span>{countPhrase(itemTotal, 'candidate entry', 'candidate entries')} shown</span>
        </div>

        <div className="wm-legend" aria-label="Graph relation legend">
          <span>Relation labels</span>
          <RelationLabel kind="recorded" detail="Export order or grouping" />
          <RelationLabel kind="exact_match" detail="Server citation or measured equality" />
          <RelationLabel kind="model_proposed" detail="Shadow suggestion" />
        </div>

        {map.sessions.map((group, sessionIndex) => (
          <section key={group.session.id} className="wm-session" aria-labelledby={`wm-session-${group.session.id}`}>
            <header className="wm-session-head">
              <div>
                <span className="wm-eyebrow">Recorded run</span>
                <h2 id={`wm-session-${group.session.id}`}>{group.session.label || 'Run without an exported label'}</h2>
              </div>
              <span>{sessionFacts(group.session) || (group.listed ? 'No date or message count exported' : 'Not listed in the session index')}</span>
            </header>
            <div className="wm-session-graph">
              <section className="wm-stage wm-opening-stage" aria-label="Opening ask stage">
                <header className="wm-stage-head">
                  <div>
                    <RelationLabel kind="recorded" detail="Exported opening context" />
                    <h3>Opening ask</h3>
                  </div>
                </header>
                <div className="wm-stage-cards">
                  {group.openingAsks.map((item) => <OpeningAsk key={item.key} item={item} onSelect={onSelect} />)}
                  {group.openingAsks.length === 0 ? <div className="wm-empty">No opening ask candidate was exported for this run.</div> : null}
                </div>
              </section>

              <RecordedSummary group={group} map={map} />

              <LanguageShadowStage
                key={`language-shadow:${fixture.project?.id || 'project'}:${group.session.id}:${languageShadow.status === 'ready' ? languageShadow.run.outputDigest : languageShadow.status}`}
                projection={languageShadow}
                sessionId={group.session.id}
                showInvalid={sessionIndex === 0}
                onSelect={onSelect}
              />

              <InterpretationStage
                group={group}
                map={map}
                mindStatus={mindStatus}
                run={mindRuns[group.session.id]}
                onAnalyze={onAnalyze}
                onCheckMind={onCheckMind}
                onSelect={onSelect}
              />

              <RecordedDetail group={group} onSelect={onSelect} />
            </div>
          </section>
        ))}

        <EntityAssociationStage
          key={`entity-associations:${fixture.project?.id || 'project'}:${entityAssociations.status === 'ready' ? entityAssociations.identity : entityAssociations.status}`}
          projection={entityAssociations}
          onSelect={onSelect}
        />

        <section className="wm-outputs" aria-labelledby="wm-outputs-heading">
          <header className="wm-session-head">
            <div>
              <span className="wm-eyebrow">Project-level exported records</span>
              <h2 id="wm-outputs-heading">Artifacts and outputs</h2>
              <RelationLabel kind="recorded" detail="Project-level export grouping" />
            </div>
            <span>{countPhrase(map.counts.artifacts, 'artifact', 'artifacts')} · each shown once</span>
          </header>
          {map.counts.artifacts === 0 ? <div className="wm-empty">No artifact candidates were exported.</div> : null}
          <ArtifactGroup title="Present at export" items={map.artifacts.exists} onSelect={onSelect} />
          <ArtifactGroup title="Unresolved at export" items={map.artifacts.unresolved} onSelect={onSelect} />
          <ArtifactGroup title="Other recorded status" items={map.artifacts.other} onSelect={onSelect} />
        </section>
      </div>
    </div>
  )
}
