import React from 'react'
import { countPhrase } from '../lib/voice.js'

/* The distilled altitude (RB7 Layers 1–2) — candidate vocabulary only.
 *
 * Everything here renders `fixture.candidates` — deterministic structures the
 * engine derived from rows, each carrying its event ids. The proxy rule
 * (UX-DOCTRINE §RB7) governs every string: a tail is "edits after the last
 * commit — unresolved," an ask is an "opening ask," a file's fate is "exists"
 * or "unresolved." The semantic words belong to Layer 3, whose slot renders
 * present-and-honestly-empty below until proof, an operator attestation, or a
 * shadow proposal fills it. The pin walks this file for the banned words. */

const SLOT_EMPTY = 'unlabeled — awaiting interpretation'

const CARD = {
  borderRadius: 'var(--r-card)',
  background: 'color-mix(in oklch, var(--surface-1) 78%, transparent)',
  border: '1px solid var(--hairline)',
  padding: '13px 15px',
}
const H = {
  margin: 0,
  font: '590 10px/1 var(--sans)',
  letterSpacing: '.08em',
  textTransform: 'uppercase',
  color: 'var(--text-2)',
}

function Slot() {
  return (
    <span style={{ font: '400 10.5px/1.4 var(--mono)', color: 'var(--text-3)', fontStyle: 'italic' }}>
      {SLOT_EMPTY}
    </span>
  )
}

function Anchor({ id }) {
  return (
    <span title="the event this stands on" style={{ font: '400 9.5px/1.3 var(--mono)', color: 'var(--text-3)' }}>
      {String(id).slice(0, 18)}…
    </span>
  )
}

export default function Distilled({ fixture, onEveryEvent }) {
  const c = fixture.candidates
  const sessions = fixture.sessions || []
  if (!c) {
    return (
      <div style={{ padding: '26px' }}>
        <p style={{ margin: 0, font: '400 12px/1.6 var(--mono)', color: 'var(--text-2)' }}>
          This reading predates the distilled layer. Refresh the reading to derive it.
        </p>
      </div>
    )
  }
  const bySession = (list) => {
    const m = new Map()
    for (const item of list) {
      if (!m.has(item.session_id)) m.set(item.session_id, [])
      m.get(item.session_id).push(item)
    }
    return m
  }
  const asks = new Map(c.opening_asks.map((a) => [a.session_id, a]))
  const clusters = bySession(c.clusters)
  const boundaries = bySession(c.boundaries)
  const tails = new Map(c.tails.map((t) => [t.session_id, t]))
  const existing = c.artifacts.filter((a) => a.fate === 'exists')
  const unresolved = c.artifacts.filter((a) => a.fate !== 'exists')

  return (
    <div className="tl-scroll" style={{ position: 'absolute', inset: 0, overflow: 'auto', padding: '20px 24px 40px' }}>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '12px', marginBottom: '14px' }}>
        <h1 style={{ margin: 0, font: '600 17px/1.2 var(--sans)', color: 'var(--text-1)' }}>What the record holds</h1>
        <span style={{ font: '400 11px/1.4 var(--mono)', color: 'var(--text-3)' }}>
          measured from the rows · every line traces to events
        </span>
        <span style={{ flex: 1 }} />
        <button
          type="button"
          onClick={onEveryEvent}
          style={{ font: '500 11.5px/1 var(--sans)', color: 'var(--text-1)', background: 'var(--surface-2)', boxShadow: 'inset 0 0 0 1px var(--hairline-strong)', borderRadius: 'var(--r-control)', padding: '7px 12px' }}
        >
          See every event
        </button>
      </div>

      {/* ── Definitive Output — per-file fate, cross-root by construction ── */}
      <section style={{ ...CARD, marginBottom: '14px' }}>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: '10px', marginBottom: '8px' }}>
          <h2 style={H}>What it produced</h2>
          <span style={{ font: '400 10.5px/1.4 var(--mono)', color: 'var(--text-3)' }}>
            {countPhrase(c.artifacts.length, 'file named in the transcripts', 'files named in the transcripts')} ·
            fates proven per file from the paths the transcripts themselves name
          </span>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
          <div style={{ minWidth: 0 }}>
            <p style={{ margin: '0 0 6px', font: '500 11px/1.4 var(--sans)', color: 'var(--state-supported)' }}>
              {countPhrase(existing.length, 'file exists on disk now', 'files exist on disk now')}
            </p>
            {existing.slice(0, 8).map((a) => (
              <div key={a.file} style={{ display: 'flex', gap: '8px', alignItems: 'baseline', padding: '2px 0' }}>
                <span style={{ flex: 1, minWidth: 0, font: '400 10.5px/1.5 var(--mono)', color: 'var(--text-2)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', direction: 'rtl', textAlign: 'left' }}>{a.file}</span>
                <span style={{ font: '400 10px/1.4 var(--mono)', color: 'var(--text-3)' }}>{countPhrase(a.edits, 'edit', 'edits')}</span>
              </div>
            ))}
            {existing.length > 8 ? (
              <p style={{ margin: '4px 0 0', font: '400 10px/1.4 var(--mono)', color: 'var(--text-3)' }}>{countPhrase(existing.length - 8, 'more file', 'more files')}</p>
            ) : null}
          </div>
          <div style={{ minWidth: 0 }}>
            <p style={{ margin: '0 0 6px', font: '500 11px/1.4 var(--sans)', color: 'var(--text-2)' }}>
              {countPhrase(unresolved.length, 'file not found at its recorded path — unresolved', 'files not found at their recorded paths — unresolved')}
            </p>
            {unresolved.slice(0, 8).map((a) => (
              <div key={a.file} style={{ display: 'flex', gap: '8px', alignItems: 'baseline', padding: '2px 0' }}>
                <span style={{ flex: 1, minWidth: 0, font: '400 10.5px/1.5 var(--mono)', color: 'var(--text-3)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', direction: 'rtl', textAlign: 'left' }}>{a.file}</span>
                <span style={{ font: '400 10px/1.4 var(--mono)', color: 'var(--text-3)' }}>{countPhrase(a.edits, 'edit', 'edits')}</span>
              </div>
            ))}
            {unresolved.length > 8 ? (
              <p style={{ margin: '4px 0 0', font: '400 10px/1.4 var(--mono)', color: 'var(--text-3)' }}>{countPhrase(unresolved.length - 8, 'more file', 'more files')}</p>
            ) : null}
          </div>
        </div>
      </section>

      {/* ── Per run: the ask, the work clusters, the boundary, the tail ── */}
      {sessions.map((s, i) => {
        const ask = asks.get(s.id)
        const cl = (clusters.get(s.id) || []).slice().sort((a, b) => b.edits - a.edits)
        const bd = boundaries.get(s.id) || []
        const tail = tails.get(s.id)
        return (
          <section key={s.id} style={{ ...CARD, marginBottom: '10px' }}>
            <div style={{ display: 'flex', alignItems: 'baseline', gap: '10px', marginBottom: '7px' }}>
              <h2 style={H}>{s.label || `Run ${i + 1}`}</h2>
              <span style={{ font: '400 10.5px/1.3 var(--mono)', color: 'var(--text-3)' }}>
                {s.date}
                {s.msgs != null ? ` · ${countPhrase(s.msgs, 'msg', 'msgs')}` : ''}
              </span>
            </div>
            {ask ? (
              <p style={{ margin: '0 0 8px', font: '400 12.5px/1.55 var(--sans)', color: 'var(--text-1)' }}>
                <span style={{ font: '590 9.5px/1 var(--sans)', letterSpacing: '.06em', textTransform: 'uppercase', color: 'var(--accent)', marginRight: '8px' }}>
                  Opening ask{ask.continuation ? ' · continues an earlier run' : ''}
                </span>
                “{ask.text}” <Anchor id={ask.event_id} />
              </p>
            ) : null}
            {cl.length > 0 ? (
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px', marginBottom: '7px' }}>
                {cl.slice(0, 6).map((k) => (
                  <span key={k.first_event_id} title={`${k.file} · from ${k.first_event_id} to ${k.last_event_id}`} style={{ font: '400 10.5px/1.4 var(--mono)', color: 'var(--text-2)', background: 'color-mix(in oklch, var(--surface-2) 70%, transparent)', boxShadow: 'inset 0 0 0 1px var(--hairline)', borderRadius: 'var(--r-chip)', padding: '4px 8px' }}>
                    {countPhrase(k.edits, 'edit', 'edits')} · {k.file.split('/').pop()}
                  </span>
                ))}
                {cl.length > 6 ? (
                  <span style={{ font: '400 10.5px/1.6 var(--mono)', color: 'var(--text-3)' }}>{countPhrase(cl.length - 6, 'more run of edits', 'more runs of edits')}</span>
                ) : null}
              </div>
            ) : null}
            <div style={{ display: 'flex', gap: '16px', alignItems: 'baseline', flexWrap: 'wrap' }}>
              {bd.length > 0 ? (
                <span style={{ font: '400 11px/1.5 var(--mono)', color: 'var(--state-supported)' }} title={bd.map((b) => b.event_id).join(' · ')}>
                  {countPhrase(bd.length, 'commit recorded', 'commits recorded')}
                  {bd.find((b) => b.message) ? ` — “${bd.find((b) => b.message).message}”` : ''}
                </span>
              ) : (
                <span style={{ font: '400 11px/1.5 var(--mono)', color: 'var(--text-3)' }}>no commit recorded in this run</span>
              )}
              {tail ? (
                <span style={{ font: '400 11px/1.5 var(--mono)', color: 'var(--state-pending)' }} title={tail.files.join('\n')}>
                  {countPhrase(tail.edits_after_last_boundary, 'edit after the last commit — unresolved', 'edits after the last commit — unresolved')}
                </span>
              ) : null}
              <Slot />
            </div>
          </section>
        )
      })}
    </div>
  )
}
