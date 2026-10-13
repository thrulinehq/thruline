import React from 'react'
import { claimView } from '../lib/receipt.js'
import { MAC_GLYPH, bord, sv, svText, tint } from '../lib/macmap.js'
import { chipLabel } from '../lib/voice.js'

/* The inspector — L2's receipt, docked (RA5, design v2).
 *
 * Same selection (`claimView`), same discipline: the claim is the project's
 * own words in quotation marks; the first-level verdict is the voice
 * authority's sentence; the engine's wording and its model tag sit beneath it
 * labeled as engine detail — a language-tier annotation, never the verdict
 * itself, and no naked confidence stands in for truth.
 *
 * Actions: Copy receipt is real (the receipt, to the clipboard). The design's
 * "Mark resolved" and "Create task" buttons are NOT rendered — there is no
 * machinery behind them, and a button that toasts success over nothing would
 * be a fabricated receipt. Surfaced in A145. */

const H4 = {
  margin: 0,
  font: '590 10px/1 var(--sans)',
  letterSpacing: '.08em',
  textTransform: 'uppercase',
  color: 'var(--text-2)',
}

export default function Inspector({ fixture, nodeId, onClose, claimRef }) {
  const [copied, setCopied] = React.useState(false)
  const v = React.useMemo(() => claimView(fixture, nodeId), [fixture, nodeId])
  const [detail, setDetail] = React.useState(false)
  if (!v) return null

  const copyReceipt = () => {
    const payload = {
      claim: v.claimText,
      state: v.word,
      sentence: v.sentence,
      run: v.run,
      date: v.runDate,
      message: v.messageNo,
      recorded_at: v.ts,
      event: v.anchor,
      evidence: v.evidence.map((e) => ({ kind: e.kind, summary: e.summary })),
      ledger: v.ledgerLine,
    }
    navigator.clipboard?.writeText(JSON.stringify(payload, null, 2))
    setCopied(true)
    setTimeout(() => setCopied(false), 1600)
  }

  return (
    <aside
      role="complementary"
      aria-label="Receipt"
      className="tl-inspector"
    >
      <div className="tl-inspector-scroll tl-scroll">
        <div style={{ flex: 'none', display: 'flex', alignItems: 'center', gap: '8px', padding: '11px 12px 11px 14px', borderBottom: '1px solid var(--hairline)' }}>
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: '6px',
              height: '23px',
              padding: '0 9px',
              borderRadius: 'var(--r-chip)',
              font: '500 11.5px/1 var(--sans)',
              color: svText(v.state),
              background: tint(v.state),
              boxShadow: `inset 0 0 0 1px ${bord(v.state)}`,
            }}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
              <path d={MAC_GLYPH[v.state] || MAC_GLYPH.unknown} />
            </svg>
            {chipLabel(v.word)}
          </span>
          <button type="button" onClick={onClose} aria-label="Close inspector" title="Close  Esc" style={{ marginLeft: 'auto', width: '24px', height: '24px', display: 'grid', placeItems: 'center', borderRadius: '6px', color: 'var(--text-3)' }}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '14px', padding: '14px' }}>
          <section style={{ display: 'flex', flexDirection: 'column', gap: '7px' }}>
            <h4 style={H4}>Claim</h4>
            <blockquote
              ref={claimRef}
              style={{
                margin: 0,
                padding: '11px 12px',
                borderRadius: 'var(--r-card)',
                background: 'color-mix(in oklch, var(--surface-2) 72%, transparent)',
                boxShadow: 'inset 0 0 0 1px var(--hairline)',
                font: '400 12px/1.62 var(--mono)',
                color: 'var(--text-1)',
                whiteSpace: 'pre-wrap',
                overflowWrap: 'anywhere',
              }}
            >
              <span style={{ color: 'var(--text-3)' }}>“</span>
              {v.claimPresentation.text}
              <span style={{ color: 'var(--text-3)' }}>”</span>
            </blockquote>
            {v.claimPresentation.reduced ? (
              <span style={{ color: 'var(--text-3)', font: '400 10px/1.45 var(--mono)' }}>
                Display-safe wording; the exact exported claim is under Raw receipt details.
              </span>
            ) : null}
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap', font: '400 10.5px/1.4 var(--mono)', color: 'var(--text-2)' }}>
              <span>
                {v.run}
                {v.runDate ? ` · ${v.runDate}` : ''}
                {v.messageNo != null ? ` · message ${v.messageNo}` : ''}
              </span>
            </div>
            <p style={{ margin: 0, font: '400 12.5px/1.5 var(--sans)', color: 'var(--text-1)' }}>{v.sentence}</p>
            <p style={{ margin: 0, font: '400 11.5px/1.5 var(--sans)', color: 'var(--text-2)' }}>{v.story}</p>
          </section>

          <section style={{ display: 'flex', flexDirection: 'column', gap: '7px' }}>
            <h4 style={H4}>Evidence</h4>
            {v.evidence.length === 0 ? (
              /* Honest absence, rendered the way the design draws it: a dashed
                 box. The words are level-2 voice (`story`), not new prose. */
              <div style={{ padding: '12px', borderRadius: 'var(--r-card)', border: '1px dashed var(--hairline-strong)', font: '400 11.5px/1.6 var(--mono)', color: 'var(--text-2)' }}>
                {v.story}
              </div>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
                {v.evidence.map((ev) => (
                  <div key={ev.key} className="tl-evidence-card">
                    <strong>{ev.presentation.action}</strong>
                    {ev.presentation.summary ? <span>{ev.presentation.summary}</span> : null}
                    {ev.presentation.path ? (
                      <span className={ev.presentation.path.outside ? 'outside' : undefined}>
                        {ev.presentation.path.label}: {ev.presentation.path.display}
                      </span>
                    ) : null}
                    {ev.presentation.line ? <span>Line: {ev.presentation.line}</span> : null}
                  </div>
                ))}
              </div>
            )}
          </section>

          <section
            style={{
              display: 'flex',
              flexDirection: 'column',
              gap: '8px',
              padding: '11px 12px',
              borderRadius: 'var(--r-card)',
              background: v.state === 'contradicted' || v.state === 'stale' ? tint(v.state) : 'transparent',
              boxShadow: `inset 0 0 0 1px ${v.state === 'contradicted' || v.state === 'stale' ? bord(v.state) : 'var(--hairline)'}`,
            }}
          >
            <h4 style={H4}>Verdict</h4>
            <p className="tl-pretty" style={{ margin: 0, font: '500 12.5px/1.45 var(--sans)', color: v.state === 'contradicted' ? svText(v.state) : 'var(--text-1)' }}>
              {v.sentence}
            </p>
            {v.judged ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                <button type="button" onClick={() => setDetail((d) => !d)} style={{ alignSelf: 'flex-start', font: '400 10.5px/1 var(--mono)', color: 'var(--text-3)' }}>
                  {detail ? '▾' : '▸'} Show how this was judged
                </button>
                {detail ? (
                  <p style={{ margin: 0, font: '400 10.5px/1.5 var(--mono)', color: 'var(--text-2)' }}>
                    {v.judged}
                    {v.model ? ` · ${v.model}` : ''} · a language-tier annotation, never the verdict
                  </p>
                ) : null}
              </div>
            ) : null}
          </section>

          <section style={{ display: 'flex', alignItems: 'center', gap: '7px', padding: '9px 11px', borderRadius: 'var(--r-card)', background: 'color-mix(in oklch, var(--surface-2) 55%, transparent)', boxShadow: 'inset 0 0 0 1px var(--hairline)' }}>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke={v.ledger?.verified ? 'var(--state-supported)' : 'var(--state-contradicted)'} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" style={{ flex: 'none' }}>
              <path d="M21.801 10A10 10 0 1 1 17 3.335M9 11l3 3L22 4" />
            </svg>
            <span style={{ font: '400 11px/1.4 var(--mono)', color: 'var(--text-2)' }}>
              {v.ledgerLine}
              {v.ledger?.event_no != null ? ` · event ${v.ledger.event_no} of ${v.ledger.total}` : ''}
            </span>
          </section>

          <details className="tl-raw-receipt">
            <summary>Raw receipt details</summary>
            <pre>{JSON.stringify(v.rawReceiptDetails, null, 2)}</pre>
          </details>

          <div style={{ display: 'flex', gap: '6px' }}>
            <button
              type="button"
              onClick={copyReceipt}
              style={{ flex: '1 1 auto', height: '28px', padding: '0 12px', borderRadius: 'var(--r-control)', font: '500 12px/1 var(--sans)', color: 'var(--text-1)', background: 'var(--surface-2)', boxShadow: 'inset 0 0 0 1px var(--hairline-strong)' }}
            >
              {copied ? 'Receipt copied' : 'Copy receipt'}
            </button>
          </div>
        </div>
      </div>
    </aside>
  )
}
