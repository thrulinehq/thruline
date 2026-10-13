/* The Inbox's "Decision broken" group and its preview (C29).
 *
 * A decision is a rule the person set for the project, in their own words;
 * the engine checks it against exact versions (docs/decisions/API.md). The
 * canvas has no artboard for it, so this group is drawn in the canvas's own
 * row language, style for style from its "Needs a check" group (template.jsx),
 * and its state chip is the spec's one coral ✕ ("the record shows the
 * opposite", DESIGN-SPEC §state words). It sits in two slots of the ported
 * template; the gallery's DEMO has no decisions, so the artboard is unchanged.
 *
 * Words come only from ui/src/hq/data/decisions.js. A person's waiver or
 * retirement is confirmed by the person, never by a click in this page (C28):
 * the preview offers the exact terminal commands to copy and, when the list
 * says the native transport is available, "Confirm on this Mac" (C35,
 * ../decision-confirm.jsx), which opens a separate window on this Mac. */
import React from 'react'
import { fmtCount } from '../../data/count.js'
import { DecisionAct } from '../decision-confirm.jsx'

const MONO = "'Geist Mono', monospace"
const CORAL = { glyph: '#FF6369', text: '#FF8A8F', border: '1px solid rgba(255,99,105,.7)', fill: 'rgba(255,99,105,.08)' }

const places = (n) => `${fmtCount(n)} ${n === 1 ? 'place' : 'places'}`

/** The words one row and its preview show, from one model row. */
export function decisionText(row) {
  const first = row.firstFinding
  return {
    title: row.words,
    version: row.version,
    aria: `Decision broken on ${row.version}: ${row.words}`,
    meta: `${row.severityWords} · ${places(row.findingCount)}`,
    where: first ? first.where : null,
    message: first ? first.message : null,
    more: row.findingCount > 1 ? ` and ${places(row.findingCount - 1)} more` : '',
    sentence: row.sentence,
  }
}

function Chip() {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: '6px', height: '24px', padding: '0 10px', border: CORAL.border, background: CORAL.fill, borderRadius: '999px', color: CORAL.text, fontSize: '13px', fontWeight: '500', whiteSpace: 'nowrap' }}>
      <span aria-hidden="true" style={{ fontSize: '12px', color: CORAL.glyph }}>✕</span>Decision broken
    </span>
  )
}

/** The group: header, then one row per broken decision. */
export function DecisionGroup({ rows, d }) {
  if (!rows || !rows.length) return null
  return (
    <section aria-label="Decision broken" style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
      <div style={{ height: '24px', display: 'flex', alignItems: 'center', gap: '10px', padding: '0 4px' }}>
        <h2 style={{ margin: '0', fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>Decision broken</h2>
        <span style={{ font: `500 12px ${MONO}`, color: '#EDEDEF', fontVariantNumeric: 'tabular-nums' }}>{fmtCount(rows.length)}</span>
        <span style={{ fontSize: '12.5px', color: '#82828C' }}>a rule you set, and the record shows it broken</span>
      </div>
      <div role="list" style={{ display: 'flex', flexDirection: 'column', gap: '1px', background: '#1F1F23', border: '1px solid #1F1F23', borderRadius: '8px', overflow: 'hidden' }}>
        {rows.map((r) => {
          const t = r.t
          const ui = r.ui
          return (
            <div key={r.id} role="listitem" style={{ position: 'relative', background: ui.bg }}>
              <button type="button" className="tl-row" onClick={ui.pick} aria-current={ui.cur} aria-label={t.aria} style={{ position: 'absolute', top: '0', left: '0', right: '0', bottom: '0', width: '100%', padding: '0', background: 'transparent', border: '0', cursor: 'pointer' }} />
              <span aria-hidden="true" style={{ position: 'absolute', left: '0', top: '8px', bottom: '8px', width: '2px', borderRadius: '1px', background: ui.edge, boxShadow: ui.glow, pointerEvents: 'none' }} />
              <div style={{ position: 'relative', pointerEvents: 'none', display: 'grid', gridTemplateColumns: '228px minmax(0, 1fr) auto', gridTemplateAreas: "'state title act' 'meta reason reason'", columnGap: '16px', rowGap: d.rowGap, alignItems: 'center', padding: d.rowPad }}>
                <div style={{ gridArea: 'state', display: 'flex', alignItems: 'center', gap: '8px', minWidth: '0' }}>
                  <Chip />
                  <span style={{ font: `12.5px ${MONO}`, color: '#A1A1AA' }}>{t.version}</span>
                </div>
                <div style={{ gridArea: 'title', minWidth: '0', fontSize: '14px', fontWeight: '500', color: '#EDEDEF', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.title}</div>
                <div style={{ gridArea: 'act', display: 'flex', alignItems: 'center', gap: '10px', justifyContent: 'flex-end' }}>
                  <button type="button" className="tl-btn" onClick={ui.pick} style={{ pointerEvents: 'auto', height: '28px', boxSizing: 'border-box', padding: '0 12px', display: 'inline-flex', alignItems: 'center', gap: '8px', background: ui.btnBg, color: ui.btnFg, border: `1px solid ${ui.btnBd}`, borderRadius: '6px', fontSize: '13px', fontWeight: ui.btnW, whiteSpace: 'nowrap', cursor: 'pointer' }}>
                    See where
                  </button>
                </div>
                <div style={{ gridArea: 'meta', display: d.line2, alignItems: 'center', minWidth: '0', fontSize: '12.5px', color: '#A1A1AA', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.meta}</div>
                <div style={{ gridArea: 'reason', display: d.line2, alignItems: 'center', minWidth: '0', fontSize: '13px', color: '#A1A1AA', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                  {t.where ? <span style={{ fontFamily: MONO, fontSize: '12.5px', color: '#EDEDEF' }}>{t.where}</span> : null}
                  {t.message ? ` · ${t.message}` : null}
                  {t.more}
                </div>
              </div>
            </div>
          )
        })}
      </div>
    </section>
  )
}

/** The right pane for a selected broken decision: its words, where the
 *  record shows it broken, and how a person settles it. */
export function DecisionPreview({ row }) {
  if (!row) return null
  const t = decisionText(row)
  return (
    <div style={{ flex: '1 1 auto', minHeight: '0', overflow: 'auto', display: 'flex', flexDirection: 'column', gap: '14px', padding: '16px 20px 14px' }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <Chip />
          <span style={{ font: `12.5px ${MONO}`, color: '#A1A1AA' }}>{t.version}</span>
        </div>
        <h2 style={{ margin: '0', fontSize: '17px', lineHeight: '1.35', fontWeight: '500', color: '#EDEDEF', textWrap: 'pretty' }}>{t.title}</h2>
        <div style={{ fontSize: '12px', color: '#82828C' }}>
          Your rule{row.acceptedBy ? `, accepted by ${row.acceptedBy}` : ''} · {row.severityWords.toLowerCase()}
        </div>
      </div>
      <div style={{ fontSize: '15px', lineHeight: '1.45', color: '#EDEDEF' }}>{t.sentence}</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
        <div style={{ fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>Where the record shows it</div>
        <div role="list" style={{ display: 'flex', flexDirection: 'column', gap: '1px', background: '#1F1F23', border: '1px solid #1F1F23', borderRadius: '6px', overflow: 'hidden' }}>
          {row.findings.map((f) => (
            <div key={f.id} role="listitem" style={{ background: '#0A0A0B', padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: '3px' }}>
              <span style={{ font: `12.5px ${MONO}`, color: '#EDEDEF', overflowWrap: 'anywhere' }}>{f.where || 'a place the record does not name'}</span>
              {f.message ? <span style={{ fontSize: '12.5px', color: '#A1A1AA' }}>{f.message}</span> : null}
              {f.span ? <span style={{ font: `12px ${MONO}`, color: '#82828C' }}>{f.span}</span> : null}
            </div>
          ))}
        </div>
        {row.findings.length < row.findingCount ? (
          <div style={{ fontSize: '12px', color: '#82828C' }}>Showing {fmtCount(row.findings.length)} of {places(row.findingCount)}.</div>
        ) : null}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', border: '1px solid #2A2A30', borderRadius: '6px', padding: '12px' }}>
        <div style={{ fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>To settle it</div>
        <div style={{ fontSize: '13.5px', lineHeight: '1.45', color: '#EDEDEF' }}>Change the places it names, and the next check on a new version says whether it holds.</div>
        <DecisionAct key={row.id + ':waive'} row={row} action="waive" label="Or set it aside for this version" />
        <DecisionAct key={row.id + ':retire'} row={row} action="retire" label="Or retire the rule" />
        <div style={{ fontSize: '12px', lineHeight: '1.45', color: '#82828C' }}>
          Each command runs in your own terminal and asks you to confirm with <span style={{ fontFamily: MONO }}>{row.commands.confirm}</span>. HQ copies the command; it records nothing and runs nothing.
        </div>
      </div>
    </div>
  )
}
