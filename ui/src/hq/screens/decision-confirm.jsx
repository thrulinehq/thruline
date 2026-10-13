/* "Confirm on this Mac", beside the terminal command it never replaces (C35).
 *
 * Shared by the two screens that offer an act on a decision: the Inbox's
 * "Decision broken" preview (set it aside for this version, or retire it) and
 * the Change Brief's proposed decisions (accept). Each shows the terminal
 * command to copy, as before, and, only when the list says the native
 * transport is available (data/decisions.js `confirmPlan`), a control that
 * walks the flow in data/decision-confirm.js:
 *
 *   prepare an inert line → read its exact review back → show it in full with
 *   its SHA-256 → only then offer the button that asks for the native window →
 *   follow that job and say only what the job says.
 *
 * A click in this page is NOT the person's confirmation, and the copy says so
 * in those words. The confirmation is the Confirm control in the separate
 * window that opens on this Mac, then the Mac's own presence check. Nothing
 * here calls an act done until the job names a receipt. Every refusal the
 * server sends is drawn with its text.
 *
 * Drawn in the chip, row and field language of the screens it sits in (Inbox/
 * decisions.jsx, Change-Brief/notes.jsx). The design gallery has no decisions,
 * so no artboard draws any of it. */
import React, { useEffect, useId, useRef, useState } from 'react'
import { confirmClient, confirmFlow, reviewFacts } from '../data/decision-confirm.js'
import { DECISIONS_CHANGED, confirmPlan, shortVersion } from '../data/decisions.js'

const MONO = "'Geist Mono', monospace"

/** A terminal command with a Copy button. HQ copies it; it records nothing and runs nothing. */
export function Command({ label, command }) {
  const [copied, setCopied] = useState(false)
  if (!command) return null
  const copy = () => {
    try {
      if (navigator && navigator.clipboard) navigator.clipboard.writeText(command)
      setCopied(true)
    } catch (e) {
      setCopied(false)
    }
  }
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
      <div style={{ fontSize: '12px', color: '#A1A1AA' }}>{label}</div>
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: '8px' }}>
        <code style={{ flex: '1 1 auto', minWidth: '0', font: `12px/1.5 ${MONO}`, color: '#EDEDEF', background: '#000000', border: '1px solid #2A2A30', borderRadius: '4px', padding: '6px 8px', overflowWrap: 'anywhere' }}>{command}</code>
        <button type="button" onClick={copy} className="tl-btn" style={{ flex: '0 0 auto', height: '28px', padding: '0 10px', background: 'transparent', color: '#EDEDEF', border: '1px solid #66666F', borderRadius: '6px', fontSize: '12.5px', cursor: 'pointer' }}>
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
    </div>
  )
}

const FIELD = { width: '100%', boxSizing: 'border-box', padding: '7px 10px', background: '#000000', border: '1px solid #66666F', borderRadius: '6px', color: '#EDEDEF', font: 'inherit', fontSize: '13.5px', lineHeight: '1.45' }
const LABEL = { display: 'flex', flexDirection: 'column', gap: '4px', fontSize: '13px', lineHeight: '1.35', color: '#A1A1AA' }
const OUTLINE = { height: '30px', padding: '0 12px', background: 'transparent', border: '1px solid #66666F', borderRadius: '6px', color: '#EDEDEF', font: 'inherit', fontSize: '13px', cursor: 'pointer' }
const PRIMARY = { height: '30px', padding: '0 14px', background: '#FF7A29', border: '0', borderRadius: '6px', color: '#000000', font: 'inherit', fontSize: '13px', fontWeight: '600', cursor: 'pointer' }
const GHOST = { height: '30px', padding: '0 10px', background: 'transparent', border: '0', borderRadius: '4px', color: '#A1A1AA', font: 'inherit', fontSize: '13px', cursor: 'pointer' }
const SMALL = { margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#A1A1AA' }

/** What each action is called on its form. */
const TITLE = { accept: 'Accept this rule on this Mac', retire: 'Retire this rule on this Mac', waive: 'Set this rule aside for this version, on this Mac' }

/** The sentence beside the button that opens the window, and why it confirms nothing. */
const NOT_A_CONFIRMATION =
  'A click in this page is not your confirmation. The confirmation is the Confirm control in the separate window that opens on this Mac, followed by the Mac’s own presence check.'

function Review({ review }) {
  const facts = reviewFacts(review.doc)
  return (
    <section aria-label="The exact review" style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
      <div style={{ fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>The exact review</div>
      {facts.summary ? <div style={{ fontSize: '13.5px', lineHeight: '1.45', color: '#EDEDEF' }}>{facts.summary}</div> : null}
      {facts.rows.length ? (
        <dl style={{ margin: '0', display: 'grid', gridTemplateColumns: 'auto minmax(0, 1fr)', columnGap: '12px', rowGap: '4px', fontSize: '12.5px', lineHeight: '18px' }}>
          {facts.rows.map(([k, v]) => (
            <React.Fragment key={k}>
              <dt style={{ color: '#82828C' }}>{k}</dt>
              <dd style={{ margin: '0', color: '#EDEDEF', overflowWrap: 'anywhere' }}>{v}</dd>
            </React.Fragment>
          ))}
        </dl>
      ) : null}
      <pre tabIndex={0} aria-label="Every field of the review, exactly as ThruLine returned it" style={{ margin: '0', maxHeight: '240px', overflow: 'auto', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', font: `12px/1.5 ${MONO}`, color: '#EDEDEF', background: '#000000', border: '1px solid #2A2A30', borderRadius: '4px', padding: '8px 10px' }}>{facts.json}</pre>
      <div style={{ fontSize: '12px', lineHeight: '17px', color: '#82828C', overflowWrap: 'anywhere' }}>
        Review digest (SHA-256): <span style={{ fontFamily: MONO, color: '#EDEDEF' }}>{review.sha}</span>
      </div>
    </section>
  )
}

/**
 * The control for one action on one decision. Renders nothing it cannot back:
 * `plan` (data/decisions.js `confirmPlan`) must offer it.
 * @param {object} props
 * @param {{offer: true, action: string, fixed: object}} props.plan
 * @param {string} props.project   the HQ project id the list was served for
 * @param {string} props.decision  the decision's id
 * @param {object} [props.client]  `confirmClient()`; a test hands in a stub-backed one
 * @param {(job: object) => void} [props.onRecorded]  after a receipt is named
 */
export function NativeConfirm({ plan, project, decision, client, onRecorded }) {
  const uid = useId()
  const [open, setOpen] = useState(false)
  const [snap, setSnap] = useState(null)
  const [name, setName] = useState('')
  const [because, setBecause] = useState('')
  const [severity, setSeverity] = useState('')
  const action = plan.action
  /* The walk is made in the click that opens the panel, so it exists the moment
     the panel does, and it is stopped when the panel closes or the screen goes.
     What the caller hands in is read at that moment. */
  const flowRef = useRef(null)
  const offRef = useRef(null)
  const given = useRef({ client, onRecorded })
  given.current = { client, onRecorded }

  const release = () => {
    if (offRef.current) offRef.current()
    if (flowRef.current) flowRef.current.stop()
    offRef.current = null
    flowRef.current = null
  }
  useEffect(() => release, [])
  const begin = () => {
    release()
    const recorded = (job) => {
      const { onRecorded: mine } = given.current
      if (typeof mine === 'function') mine(job)
      else if (typeof window !== 'undefined' && window.dispatchEvent && typeof window.CustomEvent === 'function') window.dispatchEvent(new window.CustomEvent(DECISIONS_CHANGED))
    }
    const f = confirmFlow({ client: given.current.client || confirmClient(), project, decision, action, onRecorded: recorded })
    flowRef.current = f
    offRef.current = f.subscribe(setSnap)
    setSnap(f.state)
    setOpen(true)
  }
  const close = () => {
    release()
    setSnap(null)
    setOpen(false)
  }

  const fixed = plan.fixed || {}
  const s = snap || { step: 'form', busy: false, act: null, review: null, outcome: null, refusal: null }
  const lead = (
    <p style={SMALL}>
      {NOT_A_CONFIRMATION} Preparing adds a pending line to ThruLine’s record; it does nothing until you confirm it in that window.
    </p>
  )

  if (!open) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
        <div style={{ fontSize: '12px', color: '#A1A1AA' }}>Or confirm it in a window on this Mac</div>
        <div>
          <button type="button" className="tl-btn" onClick={begin} style={OUTLINE}>Confirm on this Mac</button>
        </div>
      </div>
    )
  }

  const canPrepare = !s.busy && !!name.trim() && !!because.trim() && (action !== 'accept' || severity === 'blocks' || severity === 'warns')
  const prepare = () => {
    if (!canPrepare || !flowRef.current) return
    flowRef.current.prepare({ actor: name, because, ...(action === 'accept' ? { severity } : {}), ...fixed })
  }
  const outcome = s.outcome
  const bad = outcome && ['failed', 'no_receipt', 'unreadable', 'lost', 'unreachable', 'recorded_file_stale'].includes(outcome.kind)
  const inForm = s.step === 'form'
  const inReview = s.step === 'prepared' || s.step === 'reviewing' || s.step === 'review' || s.step === 'starting'

  return (
    <section aria-label={TITLE[action]} style={{ display: 'flex', flexDirection: 'column', gap: '10px', border: '1px solid #2A2A30', borderRadius: '6px', padding: '12px', background: '#0A0A0B' }}>
      <div style={{ fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>{TITLE[action]}</div>

      {inForm ? (
        <>
          {lead}
          <label htmlFor={uid + '-name'} style={LABEL}>
            Your name
            <input id={uid + '-name'} type="text" className="tl-field" value={name} disabled={s.busy} onChange={(e) => setName(e && e.target ? String(e.target.value) : '')} placeholder="The name to record" style={FIELD} />
          </label>
          <label htmlFor={uid + '-because'} style={LABEL}>
            Why
            <textarea id={uid + '-because'} className="tl-field" rows="2" value={because} disabled={s.busy} onChange={(e) => setBecause(e && e.target ? String(e.target.value) : '')} placeholder="Your reason, in your words" style={{ ...FIELD, resize: 'vertical', minHeight: '38px' }}></textarea>
          </label>
          {action === 'accept' ? (
            <fieldset style={{ margin: '0', padding: '0', border: '0', display: 'flex', flexDirection: 'column', gap: '4px' }}>
              <legend style={{ padding: '0', fontSize: '13px', color: '#A1A1AA' }}>When the record shows it broken</legend>
              <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13.5px', color: '#EDEDEF' }}>
                <input type="radio" name={uid + '-severity'} value="blocks" checked={severity === 'blocks'} disabled={s.busy} onChange={() => setSeverity('blocks')} />
                It blocks approval
              </label>
              <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13.5px', color: '#EDEDEF' }}>
                <input type="radio" name={uid + '-severity'} value="warns" checked={severity === 'warns'} disabled={s.busy} onChange={() => setSeverity('warns')} />
                It only warns
              </label>
            </fieldset>
          ) : null}
          {action === 'accept' && fixed.applies_from_version ? <p style={SMALL}>It applies from {shortVersion(fixed.applies_from_version)}, the version ThruLine is reading now.</p> : null}
          {action === 'waive' && fixed.version_id ? <p style={SMALL}>It is set aside for {shortVersion(fixed.version_id)} only.</p> : null}
          {s.refusal ? (
            <p role="alert" style={{ ...SMALL, color: '#EDEDEF' }}>
              <strong style={{ fontWeight: '600' }}>Not done.</strong> {s.refusal.text}
            </p>
          ) : null}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <button type="button" className="tl-btn" onClick={prepare} disabled={!canPrepare} aria-disabled={!canPrepare} style={{ ...OUTLINE, color: canPrepare ? '#EDEDEF' : '#82828C', cursor: canPrepare ? 'pointer' : 'default' }}>{s.busy ? 'Preparing…' : 'Prepare the review'}</button>
            <button type="button" className="tl-ghost" onClick={close} disabled={s.busy} style={GHOST}>Cancel</button>
          </div>
        </>
      ) : null}

      {inReview ? (
        <>
          <p role="status" style={{ ...SMALL, color: '#EDEDEF' }}>
            A pending line is in the record{s.act ? <> (<span style={{ fontFamily: MONO }}>{s.act.actRowId}</span>)</> : null}. It does nothing until you confirm it on this Mac.
            {s.step === 'reviewing' ? ' Reading its exact review…' : ''}
          </p>
          {s.review ? <Review review={s.review} /> : null}
          {s.refusal ? (
            <p role="alert" style={{ ...SMALL, color: '#EDEDEF' }}>
              <strong style={{ fontWeight: '600' }}>Not done.</strong> {s.refusal.text}
            </p>
          ) : null}
          {s.review ? <p style={SMALL}>{NOT_A_CONFIRMATION}</p> : null}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
            {s.review ? (
              <button type="button" className="tl-btn" onClick={() => flowRef.current && flowRef.current.confirm()} disabled={s.busy} style={PRIMARY}>{s.step === 'starting' ? 'Asking…' : 'Open the window on this Mac'}</button>
            ) : (
              <button type="button" className="tl-btn" onClick={() => flowRef.current && flowRef.current.review()} disabled={s.busy || s.step === 'reviewing'} style={OUTLINE}>{s.step === 'reviewing' ? 'Reading…' : 'Show the review again'}</button>
            )}
            <button type="button" className="tl-ghost" onClick={close} disabled={s.busy} style={GHOST}>Close</button>
          </div>
          <p style={{ ...SMALL, color: '#82828C' }}>Closing this leaves the pending line unconfirmed. Nothing is accepted, retired or set aside by it.</p>
        </>
      ) : null}

      {s.step === 'following' || s.step === 'ended' ? (
        <>
          <p role={bad ? 'alert' : 'status'} style={{ ...SMALL, color: '#EDEDEF' }}>{outcome ? outcome.sentence : ''}</p>
          {s.step === 'following' ? <p style={{ ...SMALL, color: '#82828C' }}>This page reads the result as it changes. It does not call this done until ThruLine names a receipt.</p> : null}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            {outcome && outcome.kind === 'unreachable' ? <button type="button" className="tl-btn" onClick={() => flowRef.current && flowRef.current.follow()} style={OUTLINE}>Check again</button> : null}
            {outcome && ['failed', 'no_receipt'].includes(outcome.kind) ? <button type="button" className="tl-btn" onClick={() => flowRef.current && flowRef.current.reset()} style={OUTLINE}>Start over</button> : null}
            {s.step === 'ended' ? <button type="button" className="tl-ghost" onClick={close} style={GHOST}>Close</button> : null}
          </div>
        </>
      ) : null}
    </section>
  )
}

/**
 * One act on one decision: the terminal command to copy, and, only where the
 * list backs it, "Confirm on this Mac" — or the sentence saying why not, with
 * the server's own reason as given.
 * @param {object} props
 * @param {object} props.row     a decisions.js model row
 * @param {'accept'|'retire'|'waive'} props.action
 * @param {string} props.label   what the terminal command is for
 * @param {object} [props.client]
 */
export function DecisionAct({ row, action, label, client }) {
  if (!row || !row.commands || !row.commands[action]) return null
  const plan = confirmPlan(row, action)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
      <Command label={label} command={row.commands[action]} />
      {plan.offer ? <NativeConfirm plan={plan} project={row.project} decision={row.id} client={client} /> : plan.note ? <p style={SMALL}>{plan.note}</p> : null}
    </div>
  )
}
