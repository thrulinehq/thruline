import React from 'react'
import { ancestors, layout, NH, NW } from '../lib/layout.js'
import { MAC_GLYPH, bord, sv, svText, tint } from '../lib/macmap.js'
import { countPhrase } from '../lib/voice.js'
import { GhostAt, NibOnTrail } from './sprites.jsx'

/* The graph — the L1 hero (RA5, UX-DOCTRINE amendment by operator directive).
 *
 * Session-grouped lanes, work-item nodes, edges from the export's lineage
 * only. The canvas gamifies the look, never the information: every node is a
 * claim with a receipt one click right, every edge carries its exported
 * `kind`, and the cast renders where — and only where — its record sits. */

const stateGlyph = (s) => MAC_GLYPH[s] || MAC_GLYPH.unknown

export default function Graph({ fixture, selected, filters, search, onSelect }) {
  const g = React.useMemo(() => layout(fixture), [fixture])
  const [zoom, setZoom] = React.useState(0.62)
  const [pan, setPan] = React.useState({ x: 38, y: 20 })
  const [panning, setPanning] = React.useState(false)
  const canvasRef = React.useRef(null)

  const dimmed = React.useCallback(
    (n) => {
      if (filters.length && !filters.includes(n.state)) return true
      if (search.trim() && !`${n.label} ${n.state}`.toLowerCase().includes(search.trim().toLowerCase()))
        return true
      return false
    },
    [filters, search],
  )
  const anc = React.useMemo(
    () => (selected ? ancestors(g.edges, selected) : new Set()),
    [g.edges, selected],
  )

  React.useEffect(() => {
    if (!panning) return undefined
    const move = (e) => setPan((p) => ({ x: p.x + e.movementX, y: p.y + e.movementY }))
    const up = () => setPanning(false)
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
    return () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
  }, [panning])

  const onWheel = (e) => {
    if (!e.ctrlKey && !e.metaKey) return
    e.preventDefault()
    setZoom((z) => Math.max(0.3, Math.min(1.6, Math.round((z - e.deltaY * 0.002) * 1000) / 1000)))
  }

  return (
    <div style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column' }}>
      <div
        ref={canvasRef}
        onWheel={onWheel}
        onMouseDown={(e) => {
          if (e.button === 0 && e.target === e.currentTarget) setPanning(true)
        }}
        style={{ position: 'relative', flex: 1, minHeight: 0, overflow: 'hidden', cursor: panning ? 'grabbing' : 'grab' }}
      >
        <div
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            transformOrigin: '0 0',
            willChange: 'transform',
            transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          }}
        >
          {/* session bands */}
          {g.bands.map((b, i) => (
            <div
              key={b.id}
              style={{
                position: 'absolute',
                left: '-260px',
                width: `${g.size.w + 620}px`,
                borderRadius: '2px',
                background: i % 2 === 0 ? 'color-mix(in oklch, var(--text-1) 2.4%, transparent)' : 'transparent',
                top: `${b.top}px`,
                height: `${b.h}px`,
              }}
            >
              <span style={{ position: 'absolute', left: '268px', top: '9px', font: '500 11px/1 var(--mono)', letterSpacing: '.02em', color: 'var(--text-2)' }}>
                {b.label}
              </span>
            </div>
          ))}

          {/* edges — lineage from the export, nothing else. Cross-session
              edges dash; the tooltip names each edge's exported kind. */}
          <svg
            width={g.size.w}
            height={g.size.h}
            viewBox={`0 0 ${g.size.w} ${g.size.h}`}
            style={{ position: 'absolute', top: 0, left: 0, overflow: 'visible', pointerEvents: 'none' }}
          >
            <defs>
              <marker id="tl-ah" viewBox="0 0 8 8" refX="6.2" refY="4" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                <path d="M1 1 L6.5 4 L1 7 z" fill="currentColor" />
              </marker>
            </defs>
            {g.edges.map((e) => {
              const a = g.nodes.find((n) => n.id === e.from)
              const b = g.nodes.find((n) => n.id === e.to)
              const onPath = selected && (anc.has(e.from) || e.from === selected) && (anc.has(e.to) || e.to === selected)
              const visible = a && b && !dimmed(a) && !dimmed(b)
              return (
                <path
                  key={`${e.from}->${e.to}`}
                  d={e.d}
                  fill="none"
                  stroke={onPath ? 'var(--accent)' : 'var(--hairline-strong)'}
                  strokeWidth={onPath ? 1.8 : 1.5}
                  strokeDasharray={e.crossSession ? '5 5' : 'none'}
                  opacity={onPath ? (visible ? 0.95 : 0.12) : visible ? 0.55 : 0.08}
                  markerEnd="url(#tl-ah)"
                  style={{ color: onPath ? 'var(--accent)' : 'var(--hairline-strong)' }}
                >
                  <title>{`${e.kind} · derived from the record`}</title>
                </path>
              )
            })}

            {/* the cast, seated on records */}
            {g.trails.map((t) => (
              <NibOnTrail key={t.session} points={t.points} />
            ))}
            {g.nodes
              .filter((n) => n.state === 'contradicted' && !dimmed(n))
              .map((n) => (
                <GhostAt key={`snag-${n.id}`} who="snag" x={n.x + NW + 18} y={n.y + NH / 2} />
              ))}
            {g.nodes
              .filter((n) => n.state === 'stale' && !dimmed(n))
              .map((n) => (
                <GhostAt key={`dusk-${n.id}`} who="dusk" x={n.x + NW + 18} y={n.y + NH / 2} />
              ))}
          </svg>

          {/* nodes */}
          {g.nodes.map((n) => {
            const sel = selected === n.id
            const dim = dimmed(n)
            return (
              <div
                key={n.id}
                onClick={() => onSelect(n.id)}
                tabIndex={0}
                role="button"
                aria-label={n.label}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') onSelect(n.id)
                }}
                title={n.label}
                style={{
                  position: 'absolute',
                  boxSizing: 'border-box',
                  width: `${NW}px`,
                  height: `${NH}px`,
                  display: 'flex',
                  alignItems: 'center',
                  gap: '7px',
                  padding: '0 9px 0 12px',
                  borderRadius: 'var(--r-node)',
                  cursor: 'pointer',
                  userSelect: 'none',
                  background:
                    n.state === 'recorded'
                      ? 'color-mix(in oklch, var(--surface-2) 74%, transparent)'
                      : `color-mix(in oklch, ${sv(n.state)} 9%, var(--surface-2))`,
                  boxShadow: `inset 3px 0 0 ${sv(n.state)}, inset 0 0 0 ${sel ? '2px var(--accent)' : `1px ${n.state === 'contradicted' ? bord(n.state) : 'var(--hairline)'}`}${
                    sel ? ', 0 0 0 4px color-mix(in oklch, var(--accent) 16%, transparent), 0 8px 22px oklch(0 0 0 / .32)' : ''
                  }`,
                  opacity: dim ? 0.15 : n.state === 'recorded' ? 0.86 : 1,
                  transform: `translate(${n.x}px, ${n.y}px) scale(${sel ? 1.035 : 1})`,
                  transition: 'opacity var(--dur-2) ease, box-shadow var(--dur-2) ease',
                }}
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke={sv(n.state)} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" style={{ flex: 'none' }}>
                  <path d={stateGlyph(n.state)} />
                </svg>
                <span
                  style={{
                    flex: 1,
                    minWidth: 0,
                    font: '500 13px/1.2 var(--mono)',
                    letterSpacing: '-.005em',
                    color: n.state === 'recorded' ? 'var(--text-2)' : 'var(--text-1)',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {n.label}
                </span>
              </div>
            )
          })}
        </div>

        {/* zoom controls */}
        <div
          style={{
            position: 'absolute',
            left: '14px',
            bottom: '14px',
            zIndex: 4,
            display: 'flex',
            gap: '2px',
            padding: '3px',
            borderRadius: '9px',
            background: 'var(--glass-fill)',
            backdropFilter: 'blur(var(--blur-glass)) saturate(180%)',
            boxShadow: 'inset 0 0 0 1px var(--hairline-strong), 0 6px 18px oklch(0 0 0 / .28)',
          }}
        >
          <button type="button" onClick={() => setZoom((z) => Math.max(0.3, Math.round((z - 0.12) * 100) / 100))} aria-label="Zoom out" style={{ width: '26px', height: '24px', display: 'grid', placeItems: 'center', borderRadius: '6px', color: 'var(--text-2)' }}>
            −
          </button>
          <span style={{ minWidth: '40px', display: 'grid', placeItems: 'center', font: '500 11px/1 var(--mono)', color: 'var(--text-2)' }}>
            {Math.round(zoom * 100)}%
          </span>
          <button type="button" onClick={() => setZoom((z) => Math.min(1.6, Math.round((z + 0.12) * 100) / 100))} aria-label="Zoom in" style={{ width: '26px', height: '24px', display: 'grid', placeItems: 'center', borderRadius: '6px', color: 'var(--text-2)' }}>
            +
          </button>
          <button
            type="button"
            onClick={() => {
              setZoom(0.62)
              setPan({ x: 38, y: 20 })
            }}
            aria-label="Fit to view"
            style={{ width: '30px', height: '24px', display: 'grid', placeItems: 'center', borderRadius: '6px', color: 'var(--text-2)', font: '500 10px/1 var(--sans)' }}
          >
            Fit
          </button>
        </div>

        {/* minimap */}
        <div
          aria-hidden="true"
          style={{
            position: 'absolute',
            right: '14px',
            bottom: '14px',
            zIndex: 4,
            width: '166px',
            height: '132px',
            borderRadius: '9px',
            overflow: 'hidden',
            background: 'var(--glass-fill)',
            backdropFilter: 'blur(var(--blur-glass)) saturate(180%)',
            boxShadow: 'inset 0 0 0 1px var(--hairline-strong), 0 6px 18px oklch(0 0 0 / .28)',
          }}
        >
          <div style={{ position: 'absolute', inset: '7px' }}>
            {g.nodes.map((n) => (
              <span
                key={n.id}
                style={{
                  position: 'absolute',
                  height: '4px',
                  borderRadius: '2px',
                  background: sv(n.state),
                  opacity: dimmed(n) ? 0.18 : n.state === 'supported' ? 0.6 : 0.95,
                  left: `${((n.x / g.size.w) * 152).toFixed(1)}px`,
                  top: `${((n.y / g.size.h) * 118).toFixed(1)}px`,
                  width: `${((NW / g.size.w) * 152).toFixed(1)}px`,
                }}
              />
            ))}
          </div>
        </div>
      </div>

      {/* timeline — the sessions as segments, one tick per claim, in its own
          state colour. Display and selection label only; ticks are records. */}
      <Timeline g={g} />
    </div>
  )
}

function Timeline({ g }) {
  const total = g.bands.reduce((a, b) => a + b.count, 0)
  return (
    <div
      style={{
        flex: 'none',
        height: '56px',
        display: 'flex',
        alignItems: 'center',
        gap: '12px',
        padding: '0 14px',
        borderTop: '1px solid var(--hairline)',
        background: 'color-mix(in oklch, var(--surface-1) 42%, transparent)',
      }}
    >
      <span style={{ flex: 'none', font: '590 9.5px/1 var(--sans)', letterSpacing: '.085em', textTransform: 'uppercase', color: 'var(--text-2)' }}>
        Timeline
      </span>
      <div style={{ position: 'relative', flex: 1, height: '26px', display: 'flex', gap: '3px' }}>
        {g.bands.map((b) => (
          <div
            key={b.id}
            title={b.label}
            style={{
              position: 'relative',
              height: '100%',
              borderRadius: '5px',
              overflow: 'hidden',
              background: 'color-mix(in oklch, var(--surface-2) 80%, transparent)',
              boxShadow: 'inset 0 0 0 1px var(--hairline)',
              flex: `${Math.max(1, b.count)} 1 0`,
            }}
          >
            <span style={{ position: 'absolute', left: '7px', top: '50%', transform: 'translateY(-50%)', font: '500 9.5px/1 var(--mono)', color: 'var(--text-2)', whiteSpace: 'nowrap' }}>
              {b.label.split(' · ')[0]}
            </span>
            <span style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', gap: '2px', padding: '0 6px', justifyContent: 'flex-end' }}>
              {g.nodes
                .filter((n) => n.session_id === b.id)
                .map((n) => (
                  <span key={n.id} style={{ width: '2px', height: '12px', borderRadius: '1px', background: sv(n.state), opacity: 0.85 }} />
                ))}
            </span>
          </div>
        ))}
      </div>
      <span style={{ flex: 'none', font: '500 10.5px/1 var(--mono)', color: 'var(--text-2)', minWidth: '110px', textAlign: 'right' }}>
        {countPhrase(total, 'claim', 'claims')} · {countPhrase(g.bands.length, 'run', 'runs')}
      </span>
    </div>
  )
}

/* Re-exported so the toolbar can colour chips without a second mapping. */
export { sv, svText, tint, bord }
