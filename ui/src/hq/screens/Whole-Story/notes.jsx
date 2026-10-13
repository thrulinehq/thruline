/* The notes on the whole-story page (C30 R-LEDGER): the words people kept on
 * this Change, with who and when, in the Decision history's own list language.
 *
 * Read only: this page writes nothing. A note is words; it is listed here, and
 * never counted toward, or read by, anything the page says is proven, ready or
 * approved. A note kept with an approval or a withdrawal says so. The server
 * masks every note it serves. It fills one slot of the ported template
 * (`noteRows`), empty in the gallery and when the Change has no notes, so the
 * artboard is unchanged. */
import React from 'react'

/**
 * @param {{notes: Array<{id: string, who: string, when: string, body: string, decisionId: string|null}>}} props  data/notes.js `notesOf`
 */
export function WholeStoryNotes({ notes }) {
  if (!Array.isArray(notes) || !notes.length) return null
  return (
    <>
      <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: '12px' }}>
        <h2 id="ws-h-notes" style={{ margin: '0', fontSize: '18px', fontWeight: '500', lineHeight: '1.25', color: '#EDEDEF' }}>Notes</h2>
        <span style={{ font: "12.5px 'Geist Mono', monospace", color: '#82828C', whiteSpace: 'nowrap' }}>words people kept</span>
      </div>
      <ol aria-labelledby="ws-h-notes" style={{ margin: '0', padding: '14px 16px', listStyle: 'none', display: 'flex', flexDirection: 'column', gap: '14px', border: '1px solid #1F1F23', borderRadius: '8px', background: '#0A0A0B' }}>
        {notes.map((n) => (
          <li key={n.id} style={{ display: 'grid', gridTemplateColumns: '40px minmax(0, 1fr)', gap: '10px' }}>
            <span style={{ font: "12.5px 'Geist Mono', monospace", lineHeight: '20px', color: '#82828C' }}>{n.when ? n.when.slice(11) : '—'}</span>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
              <span style={{ fontSize: '13.5px', lineHeight: '20px', color: '#EDEDEF', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{n.body}</span>
              <span style={{ fontSize: '12.5px', color: '#A1A1AA' }}>
                {n.who}
                {n.when ? ' · ' + n.when : ''}
                {n.decisionId ? ' · kept with an approval or a withdrawal' : ''}
              </span>
            </div>
          </li>
        ))}
      </ol>
    </>
  )
}
