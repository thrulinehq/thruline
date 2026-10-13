/* The Brief's objections (C30 R-LEDGER): what people said was wrong with the
 * statement in the inspector, drawn beside the verdict it already has, and the
 * small form the ⋯ menu's "This is wrong…" opens.
 *
 * Two slots of the ported template, both empty in the gallery and when there is
 * nothing to show, so the artboard is unchanged: `BriefObjectionLines` in the
 * inspector's body, under "What would settle it", and `BriefObjectionForm` in
 * its sticky footer above the buttons (the menu that opens it lives there).
 * Drawn in the notes form's own row, label and field language (notes.jsx).
 *
 * An objection is words beside a statement. It changes no verdict: the line
 * says what the verdict still is, and the form says so beside the box. Every
 * word a person typed stays in the box until the record has kept it. When the
 * statement's sentence is no longer the one the person objected to, the line
 * says so and does not hang the objection on the new wording. The server masks
 * every reason it serves.
 */
import React, { useState } from 'react'
import { fmtCount } from '../../data/count.js'
import { KEPT_BESIDE, OBJECTION_MAX_CHARS, SENTENCE_CHANGED } from '../../data/objections.js'
import { objectionAnswerOf } from './objection-actions.js'

/** Characters as the record counts them (code points, not UTF-16 units). */
const lengthOf = (text) => Array.from(text).length

/**
 * @param {object} props
 * @param {Array<{id: string, lead: string, reason: string, meta: string, matches: boolean}>} props.objections  data/objections.js `objectionsOf`
 * @param {string} props.verdictWord  the statement's own verdict in words ('' when unknown), drawn and never changed
 */
export function BriefObjectionLines({ objections, verdictWord }) {
  if (!Array.isArray(objections) || !objections.length) return null
  return (
    <div role="group" aria-label="What you said is wrong with this statement" style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
      <h2 style={{ margin: '0', fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>What you said is wrong</h2>
      <div role="list" style={{ display: 'flex', flexDirection: 'column', gap: '1px', background: '#1F1F23', border: '1px solid #1F1F23', borderRadius: '8px', overflow: 'hidden' }}>
        {objections.map((o) => (
          <div key={o.id} role="listitem" style={{ background: '#0A0A0B', padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: '4px' }}>
            <div style={{ fontSize: '13.5px', lineHeight: '20px', color: '#EDEDEF', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>
              <span style={{ color: '#A1A1AA' }}>{o.lead}</span> {o.reason}
            </div>
            <div style={{ fontSize: '12px', lineHeight: '17px', color: '#82828C' }}>{o.meta}</div>
            {o.matches ? null : <div style={{ fontSize: '12.5px', lineHeight: '18px', color: '#F2C94C' }}>{SENTENCE_CHANGED}</div>}
          </div>
        ))}
      </div>
      <p style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#A1A1AA' }}>
        {verdictWord ? 'What ThruLine says about this statement is unchanged: ' + verdictWord + '. ' : ''}
        {KEPT_BESIDE}
      </p>
    </div>
  )
}

/**
 * @param {object} props
 * @param {boolean} props.open  the form is open (the ⋯ menu's "This is wrong…")
 * @param {string} props.sentence  the statement being objected to, in plain words
 * @param {() => void} props.onClose
 * @param {(reason: string) => Promise<object>} props.keep  resolves to `recordObjection`'s answer
 * @param {() => void} [props.reload]  re-reads the record after an objection was kept
 */
export function BriefObjectionForm({ open, sentence, onClose, keep, reload }) {
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const [refusal, setRefusal] = useState('')
  if (!open) return null

  const chars = lengthOf(text)
  const over = chars > OBJECTION_MAX_CHARS
  const canKeep = !sending && !!text.trim() && !over
  const send = async () => {
    if (!canKeep) return
    setSending(true)
    setRefusal('')
    let answer
    try {
      answer = objectionAnswerOf(await keep(text))
    } catch (_e) {
      answer = objectionAnswerOf({ ok: false, status: 0, doc: { error: 'unreachable' } })
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
    <div role="group" aria-label="Say this statement is wrong" style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
      <label htmlFor="bo-reason" style={{ display: 'flex', flexDirection: 'column', gap: '2px', fontSize: '13px', lineHeight: '1.35', color: '#A1A1AA' }}>
        What is wrong with this statement?
        <span style={{ fontSize: '12.5px', color: '#82828C' }}>{KEPT_BESIDE}</span>
      </label>
      {sentence ? (
        <blockquote style={{ margin: '0', padding: '6px 10px', borderLeft: '2px solid #2A2A30', fontSize: '13px', lineHeight: '19px', color: '#EDEDEF', overflowWrap: 'anywhere', maxHeight: '76px', overflow: 'auto' }}>{'“' + sentence + '”'}</blockquote>
      ) : null}
      <textarea
        id="bo-reason"
        className="tl-field"
        rows="3"
        value={text}
        onChange={(e) => setText(e && e.target ? String(e.target.value) : '')}
        disabled={sending}
        placeholder="What the agent got wrong"
        style={{ width: '100%', boxSizing: 'border-box', resize: 'vertical', minHeight: '38px', padding: '9px 10px', background: '#000000', border: '1px solid #66666F', borderRadius: '6px', color: '#EDEDEF', font: 'inherit', fontSize: '13.5px', lineHeight: '1.45' }}
      ></textarea>
      {over ? (
        <p role="status" style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#F2C94C' }}>{fmtCount(chars - OBJECTION_MAX_CHARS)} too many: a reason can be at most {fmtCount(OBJECTION_MAX_CHARS)} characters.</p>
      ) : null}
      {refusal ? (
        <p role="alert" style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#EDEDEF' }}>
          <strong style={{ fontWeight: '600' }}>Not kept.</strong> {refusal}
        </p>
      ) : null}
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <button type="button" className="tl-btn" onClick={send} disabled={!canKeep} aria-disabled={!canKeep} style={{ height: '30px', padding: '0 12px', background: 'transparent', border: '1px solid #66666F', borderRadius: '6px', color: canKeep ? '#EDEDEF' : '#82828C', font: 'inherit', fontSize: '13px', cursor: canKeep ? 'pointer' : 'default' }}>{sending ? 'Keeping…' : 'Keep objection'}</button>
        <button type="button" className="tl-ghost" onClick={close} disabled={sending} style={{ height: '30px', padding: '0 10px', background: 'transparent', border: '0', borderRadius: '4px', color: '#A1A1AA', font: 'inherit', fontSize: '13px', cursor: 'pointer' }}>Cancel</button>
      </div>
    </div>
  )
}
