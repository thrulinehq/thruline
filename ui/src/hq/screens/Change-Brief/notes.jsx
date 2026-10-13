/* The Brief's notes (C30 R-LEDGER): the words people kept on this Change, with
 * who and when, and the small form the ⋯ menu's "Add a note" opens.
 *
 * Drawn in the canvas's own row, label and field language, style for style
 * from the Decisions rows above it (decisions.jsx) and the Approve sheet's note
 * box. It fills one slot of the ported template (`noteRows`); the gallery's
 * DEMO has no notes and its menu item only closes the menu, so the artboard is
 * unchanged.
 *
 * A note is words. Keeping one writes one row and nothing a verdict, a count,
 * readiness or an approval reads; the line under the box says so. Every word
 * a person typed stays in the box until the record has kept it. A note kept
 * with an approval (written on the Approve sheet) is listed here too, with a
 * label saying so. The server masks every note it serves.
 */
import React, { useState } from 'react'
import { fmtCount } from '../../data/count.js'
import { NOTE_MAX_CHARS } from '../../data/notes.js'
import { noteAnswerOf } from './notes-actions.js'

const MONO = "'Geist Mono', monospace"

/** Characters as the record counts them (code points, not UTF-16 units). */
const lengthOf = (text) => Array.from(text).length

/**
 * @param {object} props
 * @param {Array<{id: string, who: string, when: string, body: string, decisionId: string|null}>} props.notes  data/notes.js `notesOf`
 * @param {string|null} props.changeId  the tracked Change a note is kept on; null for untracked work
 * @param {boolean} props.open  the form is open (the ⋯ menu's "Add a note")
 * @param {() => void} props.onClose
 * @param {(body: string) => Promise<object>} props.add  resolves to `addNote`'s answer
 * @param {() => void} [props.reload]  re-reads the record after a note was kept
 */
export function BriefNotes({ notes, changeId, open, onClose, add, reload }) {
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const [refusal, setRefusal] = useState('')
  if (!open && !notes.length) return null

  const chars = lengthOf(text)
  const over = chars > NOTE_MAX_CHARS
  const canKeep = !sending && !!text.trim() && !over
  const keep = async () => {
    if (!canKeep) return
    setSending(true)
    setRefusal('')
    let answer
    try {
      answer = noteAnswerOf(await add(text))
    } catch (_e) {
      answer = noteAnswerOf({ ok: false, status: 0, doc: { error: 'unreachable' } })
    }
    setSending(false)
    if (answer.kind === 'kept') {
      setText('')
      onClose()
      if (reload) reload()
    } else {
      setRefusal(answer.detail)
    }
  }
  const close = () => {
    if (sending) return
    setRefusal('')
    onClose()
  }

  return (
    <section aria-label="Notes on this Change" style={{ display: 'flex', flexDirection: 'column', gap: '6px', paddingTop: '6px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '10px', height: '20px' }}>
        <h2 style={{ margin: '0', fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>Notes</h2>
        <span style={{ font: `500 12px ${MONO}`, color: '#EDEDEF', fontVariantNumeric: 'tabular-nums' }}>{fmtCount(notes.length)}</span>
        <span style={{ fontSize: '12.5px', color: '#82828C', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>words people kept here, with their name and the time</span>
      </div>
      {notes.length ? (
        <div role="list" style={{ display: 'flex', flexDirection: 'column', gap: '1px', background: '#1F1F23', border: '1px solid #1F1F23', borderRadius: '8px', overflow: 'hidden' }}>
          {notes.map((n) => (
            <div key={n.id} role="listitem" style={{ background: '#0A0A0B', padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: '4px' }}>
              <div style={{ fontSize: '12px', lineHeight: '17px', color: '#82828C' }}>
                <span style={{ color: '#EDEDEF' }}>{n.who}</span>
                {n.when ? ' · ' + n.when : ''}
                {n.decisionId ? ' · kept with an approval or a withdrawal' : ''}
              </div>
              <div style={{ fontSize: '13.5px', lineHeight: '20px', color: '#EDEDEF', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{n.body}</div>
            </div>
          ))}
        </div>
      ) : null}
      {open && !changeId ? (
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <p style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#A1A1AA' }}>A note is kept on a Change, and this work is not tracked as one yet.</p>
          <button type="button" className="tl-ghost" onClick={close} style={{ height: '24px', padding: '0 6px', background: 'transparent', border: '0', borderRadius: '4px', color: '#A1A1AA', font: 'inherit', fontSize: '12.5px', cursor: 'pointer' }}>Close</button>
        </div>
      ) : null}
      {open && changeId ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
          <label htmlFor="bn-note" style={{ display: 'flex', flexDirection: 'column', gap: '2px', fontSize: '13px', lineHeight: '1.35', color: '#A1A1AA' }}>
            A note on this Change
            <span style={{ fontSize: '12.5px', color: '#82828C' }}>kept with your name and the time · it changes no result and no approval</span>
          </label>
          <textarea
            id="bn-note"
            className="tl-field"
            rows="3"
            value={text}
            onChange={(e) => setText(e && e.target ? String(e.target.value) : '')}
            disabled={sending}
            placeholder="What you want to remember about this Change"
            style={{ width: '100%', boxSizing: 'border-box', resize: 'vertical', minHeight: '38px', padding: '9px 10px', background: '#000000', border: '1px solid #66666F', borderRadius: '6px', color: '#EDEDEF', font: 'inherit', fontSize: '13.5px', lineHeight: '1.45' }}
          ></textarea>
          {over ? (
            <p role="status" style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#F2C94C' }}>{fmtCount(chars - NOTE_MAX_CHARS)} too many: a note can be at most {fmtCount(NOTE_MAX_CHARS)} characters.</p>
          ) : null}
          {refusal ? (
            <p role="alert" style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#EDEDEF' }}>
              <strong style={{ fontWeight: '600' }}>Not kept.</strong> {refusal}
            </p>
          ) : null}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <button type="button" className="tl-btn" onClick={keep} disabled={!canKeep} aria-disabled={!canKeep} style={{ height: '30px', padding: '0 12px', background: 'transparent', border: '1px solid #66666F', borderRadius: '6px', color: canKeep ? '#EDEDEF' : '#82828C', font: 'inherit', fontSize: '13px', cursor: canKeep ? 'pointer' : 'default' }}>{sending ? 'Keeping…' : 'Keep note'}</button>
            <button type="button" className="tl-ghost" onClick={close} disabled={sending} style={{ height: '30px', padding: '0 10px', background: 'transparent', border: '0', borderRadius: '4px', color: '#A1A1AA', font: 'inherit', fontSize: '13px', cursor: 'pointer' }}>Cancel</button>
          </div>
        </div>
      ) : null}
    </section>
  )
}
