/* Team-View's second-approval control (C30 R-LEDGER): "Second this version"
 * puts a second person's name beside the approval that stands, and "Take a
 * second back" takes that person's own name away again.
 *
 * Drawn in the canvas's own button, label and field language, style for style
 * from the notes form on the Brief (Change-Brief/notes.jsx) and the Approve
 * sheet's name box. It fills one slot of the live decision row in the ported
 * template (`d.control`); the gallery's DEMO has no such slot, so the artboard
 * is unchanged.
 *
 * A second is a name. It makes no version approved, changes no result and is
 * free; the form says so, and says the thing the ruling requires beside the
 * name box: ThruLine records the name that is typed and does not check who is
 * at the keyboard. Every word a person typed stays in the box until the record
 * has kept it, and a refusal is shown as the server served it.
 */
import React, { useState } from 'react'
import { NAME_NOT_CHECKED, SECOND_NAME_MAX_CHARS } from '../../data/seconds.js'
import { answerOf } from './actions.js'

const MONO = "'Geist Mono', monospace"

/** Characters as the record counts them (code points, not UTF-16 units). */
const lengthOf = (text) => Array.from(text).length

const ghost = { height: '28px', padding: '0 10px', display: 'inline-flex', alignItems: 'center', background: 'transparent', color: '#EDEDEF', border: '1px solid #66666F', borderRadius: '6px', font: 'inherit', fontSize: '12.5px', cursor: 'pointer', whiteSpace: 'nowrap' }

/**
 * @param {object} props
 * @param {{changeId: string, full: string, version: string, approver: string, seconds: Array<{id: string, who: string, when: string}>}} props.row
 * @param {(a: {changeId: string, seen: string, actor: string}) => Promise<object>} props.second  resolves to `secondVersion`'s answer
 * @param {(a: {changeId: string, actor: string}) => Promise<object>} props.withdraw  resolves to `withdrawSecond`'s answer
 * @param {() => void} [props.reload]  re-reads the record after something was recorded
 */
export function SecondControl({ row, second, withdraw, reload }) {
  const [mode, setMode] = useState(null) // null | 'second' | 'withdraw'
  const [name, setName] = useState('')
  const [sending, setSending] = useState(false)
  const [refusal, setRefusal] = useState('')
  const taking = mode === 'withdraw'
  const chars = lengthOf(name)
  const over = chars > SECOND_NAME_MAX_CHARS
  const canSend = !sending && !!name.trim() && !over

  const open = (next) => {
    setRefusal('')
    setMode(mode === next ? null : next)
  }
  const send = async () => {
    if (!canSend) return
    setSending(true)
    setRefusal('')
    let answer
    try {
      const res = taking
        ? await withdraw({ changeId: row.changeId, actor: name })
        : await second({ changeId: row.changeId, seen: row.full, actor: name })
      answer = answerOf(res)
    } catch (_e) {
      answer = answerOf({ ok: false, status: 0, doc: { error: 'unreachable' } })
    }
    setSending(false)
    if (answer.kind === 'recorded') {
      setName('')
      setMode(null)
      if (reload) reload()
    } else if (answer.kind === 'moved') {
      const files = answer.files.slice(0, 5).join(', ')
      setRefusal(answer.detail + (files ? ' Changed since: ' + files + (answer.files.length > 5 ? ' and more.' : '.') : ''))
      if (reload) reload()
    } else {
      setRefusal(answer.detail)
    }
  }
  const id = 'second-name-' + row.changeId

  return (
    <div data-second-control="" style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', minWidth: '0', flexWrap: 'wrap' }}>
        <button type="button" className="tl-ghost" aria-expanded={mode === 'second' ? 'true' : 'false'} onClick={() => open('second')} style={ghost}>Second this version</button>
        {row.seconds.length ? (
          <button type="button" className="tl-ghost" aria-expanded={taking ? 'true' : 'false'} onClick={() => open('withdraw')} style={ghost}>Take a second back</button>
        ) : null}
        <span style={{ fontSize: '12.5px', color: '#82828C' }}>another person’s name beside this approval · it changes no result</span>
      </div>
      {mode ? (
        <form
          aria-label={taking ? 'Take a second approval back' : 'Second this version'}
          onSubmit={(e) => {
            if (e && e.preventDefault) e.preventDefault()
            send()
          }}
          style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}
        >
          <label htmlFor={id} style={{ display: 'flex', flexDirection: 'column', gap: '2px', fontSize: '13px', lineHeight: '1.35', color: '#A1A1AA' }}>
            {taking ? 'The name your second was put under' : 'Your name'}
            <span style={{ fontSize: '12.5px', color: '#82828C' }}>{NAME_NOT_CHECKED}</span>
          </label>
          <input
            id={id}
            className="tl-field"
            type="text"
            autoComplete="off"
            value={name}
            onChange={(e) => setName(e && e.target ? String(e.target.value) : '')}
            disabled={sending}
            placeholder={taking ? 'The same name, as you typed it' : 'A name that is not ' + (row.approver || 'the approver')}
            style={{ width: '100%', maxWidth: '360px', boxSizing: 'border-box', height: '36px', padding: '0 10px', background: '#000000', border: '1px solid #66666F', borderRadius: '6px', color: '#EDEDEF', font: 'inherit', fontSize: '13.5px' }}
          />
          {over ? (
            <p role="status" style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#F2C94C' }}>A name can be at most {SECOND_NAME_MAX_CHARS} characters.</p>
          ) : null}
          {refusal ? (
            <p role="alert" style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#EDEDEF' }}>
              <strong style={{ fontWeight: '600' }}>Not recorded.</strong> {refusal}
            </p>
          ) : null}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <button type="submit" className="tl-btn" disabled={!canSend} aria-disabled={!canSend} style={{ height: '30px', padding: '0 12px', background: 'transparent', border: '1px solid #66666F', borderRadius: '6px', color: canSend ? '#EDEDEF' : '#82828C', font: 'inherit', fontSize: '13px', cursor: canSend ? 'pointer' : 'default' }}>
              {sending ? 'Recording…' : taking ? 'Take my second back' : 'Put my name on this version'}
            </button>
            <button type="button" className="tl-ghost" onClick={() => open(mode)} disabled={sending} style={{ height: '30px', padding: '0 10px', background: 'transparent', border: '0', borderRadius: '4px', color: '#A1A1AA', font: 'inherit', fontSize: '13px', cursor: 'pointer' }}>Cancel</button>
            <span style={{ font: '12px ' + MONO, color: '#82828C' }}>{row.version || ''}</span>
          </div>
        </form>
      ) : null}
    </div>
  )
}
