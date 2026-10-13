import React from 'react'
import {
  EVENT_PAGE_SIZE,
  decisionFlow,
  filterDecisionFlow,
  windowDecisionFlow,
} from '../lib/decision-flow.js'
import { MAC_GLYPH, bord, sv, svText, tint } from '../lib/macmap.js'
import { countPhrase } from '../lib/voice.js'

function sessionFacts(session) {
  const facts = [session.date].filter(Boolean)
  if (session.msgs != null) facts.push(countPhrase(session.msgs, 'message', 'messages'))
  return facts.join(' · ')
}

function evidenceText(count) {
  if (count == null) return 'Open receipt'
  if (count === 0) return 'Open receipt · no evidence recorded'
  return `Open receipt · ${countPhrase(count, 'evidence record', 'evidence records')}`
}

function EventCard({ node, selected, onSelect, selectedRef }) {
  const [expanded, setExpanded] = React.useState(false)
  const text = expanded ? node.preview.fullText : node.preview.text
  return (
    <article ref={selected ? selectedRef : null} className={`df-event${selected ? ' selected' : ''}`}>
      <span className="df-dot" aria-hidden="true" style={{ '--df-state': sv(node.state) }} />
      <div className="df-card" style={{ '--df-state': sv(node.state) }}>
        <button
          type="button"
          className="df-card-main"
          onClick={() => onSelect(node.id)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              onSelect(node.id)
            }
          }}
          aria-label={`Open receipt for ${node.preview.fullText}`}
          aria-pressed={selected}
        >
          <span className="df-state" style={{ color: svText(node.state), background: tint(node.state), boxShadow: `inset 0 0 0 1px ${bord(node.state)}` }}>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d={MAC_GLYPH[node.state] || MAC_GLYPH.unknown} />
            </svg>
            {node.statusLabel}
          </span>
          <span className="df-text">{text}</span>
          <span className="df-evidence">{evidenceText(node.evidenceCount)}</span>
        </button>
        {node.preview.truncated ? (
          <button
            type="button"
            className="df-expand"
            onClick={() => setExpanded((value) => !value)}
            aria-expanded={expanded}
          >
            {expanded ? 'Show less' : 'Show more'}
          </button>
        ) : null}
      </div>
    </article>
  )
}

export default function DecisionFlow({ fixture, selected, filters, search, onSelect }) {
  const flow = React.useMemo(() => decisionFlow(fixture), [fixture])
  const filtered = React.useMemo(
    () => filterDecisionFlow(flow, { filters, search }),
    [flow, filters, search],
  )
  const [limits, setLimits] = React.useState({})
  const visible = React.useMemo(
    () => windowDecisionFlow(filtered, { limits, selected }),
    [filtered, limits, selected],
  )
  const selectedRef = React.useRef(null)

  React.useEffect(() => {
    setLimits({})
  }, [fixture, filters, search])

  React.useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: 'center' })
  }, [selected, visible.mountedCount])

  const showNext = (sessionId) => {
    setLimits((current) => {
      const prior = current[sessionId]
      const next = prior === Infinity ? Infinity : (Number.isSafeInteger(prior) ? prior : EVENT_PAGE_SIZE) + EVENT_PAGE_SIZE
      return { ...current, [sessionId]: next }
    })
  }

  const showAll = (sessionId) => setLimits((current) => ({ ...current, [sessionId]: Infinity }))

  return (
    <div className="df-scroll tl-scroll">
      <div className="df-page">
        <header className="df-page-head">
          <div>
            <h1>All recorded events</h1>
            <p>Top to bottom · recorded order, not causality · every event retains its receipt</p>
          </div>
          <div className="df-coverage" role="status">
            {filtered.hiddenCount > 0
              ? `Showing ${filtered.shownCount} of ${filtered.totalCount} events`
              : countPhrase(filtered.totalCount, 'event', 'events')}
            {visible.remainingCount > 0 ? <span>{visible.mountedCount} currently loaded</span> : null}
          </div>
        </header>

        {flow.totalCount === 0 ? (
          <div className="df-empty">No events are available on the ordinary surface for this project.</div>
        ) : filtered.shownCount === 0 ? (
          <div className="df-empty">
            No recorded events match these filters. {countPhrase(flow.totalCount, 'event remains', 'events remain')} in the project record.
          </div>
        ) : (
          visible.groups.map((group) => {
            const facts = sessionFacts(group.session)
            return (
              <section key={group.session.id} className="df-session" aria-labelledby={`df-session-${group.session.id}`}>
                <header className="df-session-head">
                  <div>
                    <span>Recorded run</span>
                    <h2 id={`df-session-${group.session.id}`}>{group.session.label || 'Run without an exported label'}</h2>
                  </div>
                  <span>{facts || (group.listed ? 'No date or message count exported' : 'Not listed in the session index')}</span>
                </header>
                <div className="df-order-label">Recorded order · not causality</div>
                <div className="df-lane">
                  {group.nodes.length > 1 ? <span className="df-spine" aria-hidden="true" /> : null}
                  {group.nodes.map((node) => (
                    <EventCard key={node.id} node={node} selected={selected === node.id} onSelect={onSelect} selectedRef={selectedRef} />
                  ))}
                </div>
                {group.remainingCount > 0 ? (
                  <div className="df-more">
                    <span>{group.mountedCount} of {group.matchingCount} matching events loaded in this run</span>
                    <button type="button" onClick={() => showNext(group.session.id)}>
                      Show next {Math.min(EVENT_PAGE_SIZE, group.remainingCount)}
                    </button>
                    <button type="button" onClick={() => showAll(group.session.id)}>Show all {group.matchingCount}</button>
                  </div>
                ) : null}
              </section>
            )
          })
        )}
      </div>
    </div>
  )
}
