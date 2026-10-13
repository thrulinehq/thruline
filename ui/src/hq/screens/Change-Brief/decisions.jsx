/* The Change Brief's decision rows for the candidate version (C29).
 *
 * A decision is a rule the person set for the project, in their own words;
 * the engine checks it against exact versions (docs/decisions/API.md). The
 * canvas has no artboard for it, so the rows are drawn in the canvas's own
 * row and chip language, style for style from the Inbox's "Decision broken"
 * group (Inbox/decisions.jsx) and the readiness column they sit in. They fill
 * one slot of the ported template; the gallery's DEMO has no decisions, so
 * the artboard is unchanged.
 *
 * Every word comes from ui/src/hq/data/decisions.js (`decisionRow`): the
 * person's words, the state sentence, blocks or warns, the first place the
 * record shows a broken one. A result on another version than the one in
 * front of the person is said to be not checked on it. A server without the
 * route gives one quiet line; before the first answer, nothing.
 *
 * A proposed decision is not in force until the person accepts it and
 * confirms it themselves (C28). Its row offers the terminal command to copy
 * and, when the list says the native transport is available, "Confirm on this
 * Mac" (C35, ../decision-confirm.jsx). When the gate is not active, the
 * section says decisions do not block approving right now, with the server's
 * own reason. */
import React from 'react'
import { fmtCount } from '../../data/count.js'
import { gateNote, shortVersion } from '../../data/decisions.js'
import { DecisionAct } from '../decision-confirm.jsx'

const MONO = "'Geist Mono', monospace"

/* One glyph and word per state, in the canvas's state language: the spec's
 * one coral ✕ for "the record shows the opposite", ● for a check that holds on
 * a version, ○ for no proof, ◭ for a result the code has moved past. */
const CHIP = {
  broken: { glyph: '✕', word: 'Broken', color: '#FF8A8F', glyphColor: '#FF6369', border: '1px solid rgba(255,99,105,.7)', bg: 'rgba(255,99,105,.08)' },
  holds: { glyph: '●', word: 'Holds', color: '#EDEDEF', glyphColor: '#3DD68C', border: '1px solid rgba(61,214,140,.5)', bg: 'rgba(61,214,140,.08)' },
  unverifiable: { glyph: '○', word: 'No proof found', color: '#EDEDEF', glyphColor: '#A1A1AA', border: '1px dashed #82828C', bg: 'transparent' },
  stale: { glyph: '◭', word: 'Needs a re-check', color: '#F2C94C', glyphColor: '#F2C94C', border: '1px solid rgba(242,201,76,.6)', bg: 'repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px)' },
  not_checked: { glyph: '○', word: 'Not checked yet', color: '#A1A1AA', glyphColor: '#82828C', border: '1px dashed #82828C', bg: 'transparent' },
  not_applicable: { glyph: '▪', word: 'Does not apply', color: '#A1A1AA', glyphColor: '#A1A1AA', border: '1px solid #66666F', bg: 'transparent' },
  proposed: { glyph: '▪', word: 'Proposed', color: '#A1A1AA', glyphColor: '#A1A1AA', border: '1px solid #66666F', bg: 'transparent' },
}

/**
 * What the Brief draws for the decisions, from the one model. PURE.
 * @param {object|undefined} model  decisionsModel(...) (data/decisions.js)
 * @param {string|null} candidate   the full id of the version in front of the person
 * @returns {null | {quiet: string|null, gate: string|null, rows: object[]}}  null = draw nothing
 */
export function briefDecisions(model, candidate) {
  if (!model || model.status === 'reading') return null
  if (model.status !== 'served') return { quiet: model.message || null, gate: null, rows: [] }
  const now = shortVersion(candidate)
  const rows = model.rows
    .filter((r) => r.lifecycle === 'accepted' || r.lifecycle === 'proposed')
    .map((r) => {
      const elsewhere = !!(r.versionId && candidate && r.versionId !== candidate)
      const said = now && r.sentence.indexOf('not checked on ' + now) >= 0
      const first = r.state === 'broken' && r.firstFinding ? r.firstFinding : null
      return {
        id: r.id,
        words: r.words,
        sentence: r.sentence,
        notHere: elsewhere && !said ? 'It is not checked on ' + now + '.' : null,
        chip: CHIP[r.state] || CHIP.not_checked,
        meta: r.lifecycle === 'proposed' ? (r.severity === 'blocks' ? 'Would block approval once accepted' : r.severity === 'warns' ? 'Would warn once accepted' : r.severityWords) : r.severityWords,
        proposal: r.lifecycle === 'proposed' ? r : null,
        where: first ? first.where : null,
        more: first && r.findingCount > 1 ? ' and ' + fmtCount(r.findingCount - 1) + (r.findingCount - 1 === 1 ? ' place' : ' places') + ' more' : '',
        waived: r.waivedSentence || null,
      }
    })
  if (!rows.length) return null
  return { quiet: null, gate: gateNote(model), rows }
}

function Chip({ chip }) {
  return (
    <span style={{ flex: '0 0 auto', display: 'inline-flex', alignItems: 'center', gap: '6px', height: '22px', boxSizing: 'border-box', padding: '0 8px', border: chip.border, background: chip.bg, borderRadius: '999px', color: chip.color, fontSize: '12.5px', fontWeight: '500', whiteSpace: 'nowrap' }}>
      <span aria-hidden="true" style={{ fontSize: '11px', color: chip.glyphColor }}>{chip.glyph}</span>
      {chip.word}
    </span>
  )
}

/** The rows, or the one quiet line, under the readiness lead. */
export function BriefDecisions({ view }) {
  if (!view) return null
  if (!view.rows.length) {
    return view.quiet ? <p style={{ margin: '0', paddingTop: '6px', fontSize: '12.5px', lineHeight: '18px', color: '#82828C' }}>{view.quiet}</p> : null
  }
  return (
    <section aria-label="Decisions you set, on this version" style={{ display: 'flex', flexDirection: 'column', gap: '6px', paddingTop: '6px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '10px', height: '20px' }}>
        <h2 style={{ margin: '0', fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>Decisions</h2>
        <span style={{ font: `500 12px ${MONO}`, color: '#EDEDEF', fontVariantNumeric: 'tabular-nums' }}>{fmtCount(view.rows.length)}</span>
        <span style={{ fontSize: '12.5px', color: '#82828C', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>rules you set, as the record shows them</span>
      </div>
      {view.gate ? <p style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#A1A1AA' }}>{view.gate}</p> : null}
      <div role="list" style={{ display: 'flex', flexDirection: 'column', gap: '1px', background: '#1F1F23', border: '1px solid #1F1F23', borderRadius: '8px', overflow: 'hidden' }}>
        {view.rows.map((r) => (
          <div key={r.id} role="listitem" style={{ background: '#0A0A0B', padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: '4px' }}>
            <div style={{ display: 'flex', alignItems: 'flex-start', gap: '8px' }}>
              <Chip chip={r.chip} />
              <span style={{ minWidth: '0', fontSize: '13.5px', lineHeight: '20px', fontWeight: '500', color: '#EDEDEF', textWrap: 'pretty' }}>{r.words}</span>
            </div>
            <div style={{ fontSize: '12.5px', lineHeight: '18px', color: '#A1A1AA' }}>
              {r.sentence}
              {r.notHere ? ' ' + r.notHere : null}
            </div>
            {r.waived ? <div style={{ fontSize: '12px', lineHeight: '17px', color: '#82828C' }}>{r.waived}</div> : null}
            <div style={{ fontSize: '12px', lineHeight: '17px', color: '#82828C', overflowWrap: 'anywhere' }}>
              {r.meta}
              {r.where ? ' · ' : null}
              {r.where ? <span style={{ fontFamily: MONO, color: '#EDEDEF' }}>{r.where}</span> : null}
              {r.more}
            </div>
            {r.proposal ? (
              <div style={{ paddingTop: '4px' }}>
                <DecisionAct key={r.id + ':accept'} row={r.proposal} action="accept" label="Accept it in your own terminal" />
              </div>
            ) : null}
          </div>
        ))}
      </div>
    </section>
  )
}
