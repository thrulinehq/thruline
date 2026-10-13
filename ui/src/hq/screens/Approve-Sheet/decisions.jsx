/* Approve refuses on a decision the person set (C29, C35).
 *
 * Whether a decision refuses is the engine's own typed answer, not the state
 * word (data/decisions.js `approvalRefusals`): the list's `approval_gate` must
 * be active, and the decision's or its result's `blocks_approval` must be true
 * for exactly the version on the sheet. Then Approve is off through the
 * sheet's own refusal state (logic.js `why`), and this box says which
 * decisions, in the person's own words, with each one's sentence and "See
 * where" (the Brief's decision rows): broken on this version, no proof found
 * on it, or no check recorded on it yet. A dormant gate, a warning, a valid
 * waiver, a stale result and an unverifiable result the policy only warns
 * about refuse nothing, and when the gate is not active the box says so with
 * the server's own reason. A decision with a result on another version, or
 * none, is a note. The server stays the gate: this says why before the button
 * is pressed.
 *
 * The canvas has no artboard for it: the box is the sheet's own "Changed
 * while this was open" refusal box, style for style, in the spec's one coral
 * ✕ ("the record shows the opposite"). It fills one slot of the ported
 * template; the gallery's DEMO has no decisions, so the artboard is unchanged. */
import React from 'react'
import { approvalRefusals, gateNote, notCheckedOn, shortVersion } from '../../data/decisions.js'

const MONO = "'Geist Mono', monospace"

/**
 * What the sheet says about decisions on `candidate`. PURE.
 * @param {object|undefined} model  decisionsModel(...) (data/decisions.js)
 * @param {string|null} candidate   the full id of the version on the sheet
 * @returns {{refusals: object[], unchecked: object[], gateNote: string|null, short: string|null, why: {l1: string, l2: string}|null}}
 */
export function sheetDecisions(model, candidate) {
  const short = shortVersion(candidate)
  if (!model || model.status !== 'served' || !candidate) return { refusals: [], unchecked: [], gateNote: null, short, why: null }
  const refusals = approvalRefusals(model, candidate).map((r) => ({
    id: r.id,
    kind: r.kind,
    words: r.words,
    sentence: r.sentence,
    where: r.firstFinding ? r.firstFinding.where : null,
  }))
  /* not checked on this version: a result on another version (notCheckedOn), or none at all;
     a decision the engine already refuses on is listed as a refusal, not twice */
  const refused = new Set(refusals.map((r) => r.id))
  const unchecked = notCheckedOn(model, candidate)
    .concat(model.rows.filter((r) => r.lifecycle === 'accepted' && r.blocks && r.state === 'not_checked'))
    .filter((r) => !refused.has(r.id))
    .map((r) => ({ id: r.id, words: r.words }))
  const n = refusals.length
  const why = n
    ? {
      l1: n === 1 ? 'A decision you set blocks approving this version.' : n + ' decisions you set block approving this version.',
      l2: '“' + refusals[0].words + '”: ' + refusals[0].sentence + (n > 1 ? ' And ' + (n - 1) + ' more, listed above.' : ''),
    }
    : null
  return { refusals, unchecked, gateNote: gateNote(model), short, why }
}

/** The refusal box (when a decision refuses), the gate note and the not-checked note. */
export function DecisionRefusal({ view, seeWhere }) {
  if (!view || (!view.refusals.length && !view.unchecked.length && !view.gateNote)) return null
  const gate = view.gateNote ? <p style={{ flex: '0 0 auto', margin: '0', fontSize: '12.5px', lineHeight: '1.45', color: '#A1A1AA' }}>{view.gateNote}</p> : null
  const checked = view.unchecked.length ? (
    <p style={{ flex: '0 0 auto', margin: '0', fontSize: '12.5px', lineHeight: '1.45', color: '#82828C' }}>
      {'Not checked on ' + view.short + ': '}
      {view.unchecked.map((u, i) => (i ? ', ' : '') + '“' + u.words + '”').join('')}
      {'. The next check on ' + view.short + ' says whether ' + (view.unchecked.length === 1 ? 'it holds' : 'they hold') + '.'}
    </p>
  ) : null
  const note = gate || checked ? <>{gate}{checked}</> : null
  if (!view.refusals.length) return note
  return (
    <>
      <section aria-label="Decisions that block this approval" style={{ flex: '0 0 auto', display: 'grid', gridTemplateColumns: '4px minmax(0, 1fr)', columnGap: '14px', background: '#111113', border: '1px solid rgba(255,99,105,.45)', borderRadius: '8px', overflow: 'hidden' }}>
        <span aria-hidden="true" style={{ background: '#FF6369' }} />
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', padding: '12px 14px 12px 0' }}>
          <div style={{ fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#FF8A8F' }}>
            <span aria-hidden="true" style={{ color: '#FF6369' }}>{'✕ '}</span>
            {view.refusals.length === 1 ? 'A decision you set blocks this' : 'Decisions you set block this'}
          </div>
          <div style={{ fontSize: '14px', lineHeight: '1.5', color: '#EDEDEF' }}>
            {'Not approved while ' + (view.refusals.length === 1 ? 'this decision blocks' : 'these decisions block') + ' approving ' + view.short + '. ' + (view.refusals.length === 1 ? 'It says' : 'Each says') + ' why. Change the places named and check again, or set one aside for this version in a terminal.'}
          </div>
          <div role="list" style={{ display: 'flex', flexDirection: 'column', gap: '1px', background: '#1F1F23', border: '1px solid #1F1F23', borderRadius: '6px', overflow: 'hidden' }}>
            {view.refusals.map((r) => (
              <div key={r.id} role="listitem" style={{ background: '#0A0A0B', padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: '3px' }}>
                <span style={{ fontSize: '13.5px', lineHeight: '1.4', fontWeight: '500', color: '#EDEDEF' }}>{'“' + r.words + '”'}</span>
                <span style={{ fontSize: '12.5px', lineHeight: '1.4', color: '#A1A1AA' }}>{r.sentence}</span>
                {r.where ? <span style={{ font: `12.5px ${MONO}`, color: '#EDEDEF', overflowWrap: 'anywhere' }}>{r.where}</span> : null}
              </div>
            ))}
          </div>
          {seeWhere ? (
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px', flexWrap: 'wrap' }}>
              <span style={{ fontSize: '13px', color: '#A1A1AA' }}>The Brief lists every decision on this version, with the first place the record shows.</span>
              <span style={{ flex: '1' }} />
              <a href={seeWhere} className="tl-btn" style={{ height: '32px', padding: '0 14px', background: '#FF7A29', color: '#000000', borderRadius: '6px', fontWeight: '600', fontSize: '13.5px', display: 'inline-flex', alignItems: 'center', whiteSpace: 'nowrap' }}>See where</a>
            </div>
          ) : null}
        </div>
      </section>
      {note}
    </>
  )
}
