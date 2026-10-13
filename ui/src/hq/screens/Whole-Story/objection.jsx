/* The objections on the whole-story page (C30 R-LEDGER): the statements people
 * said were wrong, each with the verdict it already had, and what was said, who
 * said it and when — in the notes list's own language (notes.jsx).
 *
 * Read only: this page writes nothing. An objection is words beside a statement;
 * it is listed here, and never counted toward, or read by, anything the page
 * says is proven, ready or approved. The verdict drawn beside each one is the
 * statement's own, carried through untouched. The server masks every reason it
 * serves. It fills one slot of the ported template (`objectionRows`), empty in
 * the gallery and when nobody objected to anything, so the artboard is
 * unchanged. The export includes these same masked words and verdict. */
import React from 'react'
import { KEPT_BESIDE, SENTENCE_CHANGED } from '../../data/objections.js'

/**
 * @param {{objected: Array<{id: string, step: number|null, plain: string, verdictWord: string,
 *   objections: Array<{id: string, lead: string, reason: string, meta: string, matches: boolean}>}>}} props
 *   data/objections.js `objectedStatements`
 */
export function WholeStoryObjections({ objected }) {
  if (!Array.isArray(objected) || !objected.length) return null
  return (
    <>
      <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: '12px' }}>
        <h2 id="ws-h-objections" style={{ margin: '0', fontSize: '18px', fontWeight: '500', lineHeight: '1.25', color: '#EDEDEF' }}>What you said was wrong</h2>
        <span style={{ font: "12.5px 'Geist Mono', monospace", color: '#82828C', whiteSpace: 'nowrap' }}>kept beside the statement</span>
      </div>
      <ol aria-labelledby="ws-h-objections" style={{ margin: '0', padding: '14px 16px', listStyle: 'none', display: 'flex', flexDirection: 'column', gap: '14px', border: '1px solid #1F1F23', borderRadius: '8px', background: '#0A0A0B' }}>
        {objected.map((s) => (
          <li key={s.id} style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
            <span style={{ fontSize: '13.5px', lineHeight: '20px', color: '#EDEDEF', overflowWrap: 'anywhere' }}>{'“' + s.plain + '”'}</span>
            {s.verdictWord ? <span style={{ fontSize: '12.5px', color: '#A1A1AA' }}>What ThruLine says about it, unchanged: {s.verdictWord}</span> : null}
            {s.objections.map((o) => (
              <div key={o.id} style={{ display: 'flex', flexDirection: 'column', gap: '2px', paddingLeft: '10px', borderLeft: '2px solid #2A2A30' }}>
                <span style={{ fontSize: '13.5px', lineHeight: '20px', color: '#EDEDEF', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>
                  <span style={{ color: '#A1A1AA' }}>{o.lead}</span> {o.reason}
                </span>
                <span style={{ fontSize: '12.5px', color: '#A1A1AA' }}>{o.meta}</span>
                {o.matches ? null : <span style={{ fontSize: '12.5px', color: '#F2C94C' }}>{SENTENCE_CHANGED}</span>}
              </div>
            ))}
          </li>
        ))}
      </ol>
      <p style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#A1A1AA' }}>{KEPT_BESIDE}</p>
    </>
  )
}
