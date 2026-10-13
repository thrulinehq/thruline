import React from 'react'
import MacShell from './mac/shell.jsx'

/* MOCK TOOLING — not a product route.
 *
 * States the real corpus cannot show today must still be reviewable: the
 * attention register, the empty cabinet, the annotating pill, and the
 * recovery affordance (RA5 directive 5 — shadow-first, mock allowed here with
 * unmistakably fake names). So the gallery feeds MacShell hand-written index
 * documents and marks every frame as tooling.
 *
 * Containment: the only import of this module is guarded by
 * `import.meta.env.VITE_MOCK_TOOLING`, which is undefined in an ordinary
 * build, so the bundler removes the branch and the whole module with it. The
 * gate `the state gallery is absent from a production build` asserts that by
 * grepping the built bundle. Nothing here is a record, and nothing here is
 * reachable from a screen a person opens to read their own project.
 *
 * The sentences below are the voice authority's, copied because a mock has no
 * exporter to ask; they are never a second source of truth. */

const ROW = (name, chips, chipLine, needing, items) => ({
  fixture: `mock-${name}`,
  name,
  source: 'claude_code',
  chips,
  chip_line: chipLine,
  recency: 'Aug 4',
  attention: {
    headline: needing ? `${name} — 1 thing worth a look. Everything else checks out.` : `${name} — everything checks out.`,
    needing_attention: needing,
    items,
  },
})

const CALM = {
  contract_version: 2,
  kind: 'index',
  generated_at: '2026-08-04T00:00:00Z',
  totals: {
    projects: 3,
    needing_attention: 1,
    headline: '1 thing worth a look across 3 projects. Everything else checks out.',
    newest_record: '2026-08-04',
  },
  projects: [
    ROW(
      'MOCK-payments-demo',
      [
        { word: 'contradicted', label: 'Contradicted', count: 1 },
        { word: 'supported', label: 'Supported', count: 40 },
      ],
      '1 contradicted · 40 supported',
      1,
      [
        {
          id: 'mock_contradiction',
          kind: 'contradiction',
          sentence: 'It said 33/33 tests passed; the run shows 31 passed, 2 failed.',
          action: 'Look at the receipt.',
        },
      ],
    ),
    ROW('MOCK-orbit-demo', [{ word: 'supported', label: 'Supported', count: 41 }], '41 supported', 0, []),
    ROW('MOCK-ledger-demo', [{ word: 'noted', label: 'Noted', count: 28 }], '28 noted', 0, []),
  ],
}

const EMPTY = {
  contract_version: 2,
  kind: 'index',
  generated_at: '2026-08-04T00:00:00Z',
  totals: {
    projects: 0,
    needing_attention: 0,
    headline: 'No records read yet.',
    subhead:
      'Point ThruLine at the records your coding agents already leave behind, and it will tell you what was actually done.',
  },
  projects: [],
}

/* Design v3's recovery surfaces (RA6): incident, pipeline, metrics, likely
 * causes, packet, contract, approval gate — ALL shadow classes, so the whole
 * screen lives here on unmistakably fake data until RB2+ graduate them. The
 * strings below are the design's, quoted as a mock of a future surface; none
 * may appear in a product file, and the pin checks. */
function RecoveryOsMock() {
  const M = { padding: '10px 12px', borderRadius: '10px', background: 'var(--s1)', border: '1px solid var(--line)' }
  return (
    <div className="os-root" style={{ position: 'relative', inset: 'auto', height: '100%' }}>
      <div />
      <div className="os-scroll" style={{ overflow: 'auto', padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
        <h1 style={{ margin: 0, font: '600 20px/1.2 var(--sans)' }}>Mission Control — designed surface (mock)</h1>
        <div style={{ ...M, borderColor: 'var(--o)', display: 'flex', alignItems: 'center', gap: '12px' }}>
          <span style={{ font: '600 10px/1 var(--mono)', color: 'var(--o)' }}>P1 STALLED</span>
          <span style={{ flex: 1, font: '400 12.5px/1.4 var(--sans)' }}>MOCK-agent session #000 stopped making progress · no output for 06:42</span>
          <button type="button" disabled style={{ background: 'var(--s2)', color: 'var(--t3)', borderRadius: '7px', padding: '6px 10px' }}>Pause &amp; preserve</button>
          <button type="button" disabled style={{ background: 'var(--as)', color: 'var(--a)', borderRadius: '7px', padding: '6px 10px' }}>Recover now</button>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '10px' }}>
          {[['Recovery readiness', '92%'], ['Last heartbeat', '06:42'], ['Context pressure', '88%'], ['Changes at risk', '14 files']].map(([k, v]) => (
            <div key={k} style={M}><div style={{ font: '400 10px/1.3 var(--mono)', color: 'var(--t3)' }}>{k}</div><div style={{ font: '600 18px/1.2 var(--sans)' }}>{v}</div></div>
          ))}
        </div>
        <div style={M}>
          <div style={{ font: '600 11px/1.3 var(--sans)', marginBottom: '6px' }}>Likely causes — ranked hypotheses (mock)</div>
          {[['01', 'Tool process alive but silent', '94%'], ['02', 'Context pressure caused a planning loop', '76%'], ['03', 'Test waiting on an unavailable dependency', '68%']].map(([n, t, c]) => (
            <div key={n} style={{ display: 'flex', gap: '10px', font: '400 12px/1.6 var(--sans)', color: 'var(--t2)' }}>
              <span style={{ color: 'var(--t3)', font: '500 10px/1.6 var(--mono)' }}>{n}</span>
              <span style={{ flex: 1 }}>{t}</span><span style={{ color: 'var(--t3)' }}>{c}</span>
            </div>
          ))}
        </div>
        <div style={M}>
          <div style={{ font: '600 11px/1.3 var(--sans)', marginBottom: '4px' }}>Next session target (mock)</div>
          <p style={{ margin: 0, font: '400 11.5px/1.6 var(--mono)', color: 'var(--t3)' }}>
            MOCK-payments-demo · resume mode: diagnose first · no writes · launch awaits the operator — the approval gate is the design, not an option.
          </p>
        </div>
      </div>
    </div>
  )
}

const STATES = [
  ['ATTENTION', () => <MacShell key="attn" index={CALM} />],
  ['EMPTY', () => <MacShell key="empty" index={EMPTY} />],
  ['RECOVERY OS (RB2+ shadow)', () => <RecoveryOsMock key="rec" />],
]

export default function Gallery() {
  const [which, setWhich] = React.useState(() => {
    const want = new URLSearchParams(location.search).get('state')
    const i = STATES.findIndex(([label]) => label.toLowerCase().startsWith((want || '').toLowerCase()))
    return i >= 0 ? i : 0
  })
  const [, render] = STATES[which]
  return (
    <div style={{ minHeight: '100vh' }}>
      {/* The permanent demo-data banner (C7 Part 4). Sticky, unconditional,
          no dismiss control. The state buttons beside it switch which mock is
          shown; none of them can turn the banner off. */}
      <div
        data-demo-banner="gallery"
        style={{
          position: 'sticky',
          top: 0,
          zIndex: 40,
          display: 'flex',
          alignItems: 'center',
          gap: '12px',
          padding: '9px 16px',
          background: '#ff7ad9',
          color: '#1a0716',
          font: '400 11px/1.5 var(--mono)',
        }}
      >
        <strong>MOCK TOOLING — placeholder data, not a product route.</strong>
        <span style={{ flex: 1 }} />
        {STATES.map(([label], i) => (
          <button
            key={label}
            type="button"
            onClick={() => setWhich(i)}
            style={{
              font: 'inherit',
              padding: '3px 9px',
              borderRadius: '2px',
              border: '1px solid #1a071640',
              background: i === which ? '#1a0716' : 'transparent',
              color: i === which ? '#ff7ad9' : '#1a0716',
              cursor: 'pointer',
            }}
          >
            {label}
          </button>
        ))}
      </div>
      <div style={{ outline: '2px dashed #ff7ad9', outlineOffset: '-2px', height: 'calc(100vh - 40px)' }}>{render()}</div>
    </div>
  )
}
