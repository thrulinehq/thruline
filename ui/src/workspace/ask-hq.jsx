import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { Blocked } from './blocked.jsx'
import { answer, ask, openDoor, scopeChip, suggestions } from './ask-hq.js'

/* The Ask bar — a scoped command bar, never a chat (P6-4; design record §3.5).
 *
 * WHAT IT DRAWS, top to bottom, in this order every time: the one permanent
 * line, the removable scope chip, the five questions this record can answer,
 * one box to type in, and then either an answer card with "Evidence it used"
 * or the "The record doesn't say" card. One action after an answer.
 *
 * WHAT IT IS NOT. There is no avatar, no typing indicator, no personality, no
 * accusation, no history of a conversation and nothing that resembles a reply.
 * Nothing is ever pending: an answer is a pure function over the model the
 * screen already has, so it is on screen in the same render as the question.
 *
 * WHAT IT MAY SAY. `HQ_COPY.ask`, and the typed fields of the one derived
 * view-model, and nothing else — the rule `ask-hq.js` states and
 * `ask-hq.test.js` drives over the rendered answers.
 *
 * WHAT IT MAY DO. It reads nothing and writes nothing. Its only effect on the
 * world is `openDoor`, which moves the Change screen to a stage and scrolls to
 * a line that stage already renders.
 *
 * WHERE IT LIVES. Inside the shell's one sheet (`shell-chrome.jsx`), so the
 * focus trap, Escape, the scrim and the return of focus to whatever opened it
 * are the shell's, already built and already tested, rather than a second copy
 * of the same rules living here.
 */

const A = HQ_COPY.ask
const list = (v) => (Array.isArray(v) ? v : [])

/** What Ask found, with the evidence it used and one thing to do next. */
function AnswerCard({ result, onDoor }) {
  return <section className="ws-ask-answer" aria-label={result.question || A.title}>
    <h3 className="ws-ask-answer-q">{result.question}</h3>
    {result.sentences.map((s, i) => <p key={i} className={i ? 'ws-ask-answer-line' : 'ws-ask-answer-lead'}>{s}</p>)}
    <h4 className="ws-ask-evidence-h">{A.answer.evidence_heading}</h4>
    {result.evidence.length
      ? <ul className="ws-ask-evidence">
        {result.evidence.map((e, i) => <li key={e.kind + ':' + e.id + ':' + i}>
          <span className="ws-ask-evidence-kind">{e.kindName}</span>
          <span className="ws-ask-evidence-label">{e.label}</span>
          <button type="button" className="ws-change-secondary ws-ask-door" onClick={() => onDoor(e.door)}>{A.door}</button>
        </li>)}
      </ul>
      : <p className="ws-change-muted">{A.answer.evidence_none}</p>}
    {result.action
      ? <>
        <h4 className="ws-ask-action-h">{A.answer.action_heading}</h4>
        {/* ONE action, and it is a door to the evidence on the screen behind
          * this sheet. Ask never offers a second thing to do. */}
        <div data-primary-action="">
          <button type="button" className="ws-change-primary ws-ask-action" onClick={() => onDoor(result.action.door)}>{result.action.label}</button>
        </div>
      </>
      : null}
  </section>
}

/**
 * The record could not answer this one — written with the same care as an
 * answer, in the three parts §3.5 names: looked for · missing · what would
 * answer it. It is never a blank, and it never reads as a pass.
 */
function CannotCard({ result }) {
  return <section className="ws-ask-cannot" aria-label={A.unanswered.heading}>
    <h3 className="ws-ask-cannot-h">{A.unanswered.heading}</h3>
    {result.question ? <p className="ws-ask-answer-q">{result.question}</p> : null}
    <dl className="ws-ask-cannot-parts">
      <div><dt>{A.unanswered.looked_for}</dt><dd>{result.lookedFor}</dd></div>
      <div><dt>{A.unanswered.missing}</dt><dd>{result.missing}</dd></div>
      <div><dt>{A.unanswered.would_answer}</dt><dd>{result.wouldAnswer}</dd></div>
    </dl>
  </section>
}

/**
 * Ask, for one Change.
 *
 * @param {object} props
 * @param {object|null} props.hq what `deriveChangeModel` returned
 * @param {(stage: string) => void} props.onGo the Change screen's stage move
 * @param {() => void} [props.onClose] the sheet's own close, called when a
 *   door takes the person to the screen behind this one
 */
export function AskPanel({ hq, onGo, onClose }) {
  const [scoped, setScoped] = React.useState(true)
  const [text, setText] = React.useState('')
  const [result, setResult] = React.useState(null)
  const chip = scopeChip(hq)
  const offered = suggestions(hq)

  const go = (door) => {
    openDoor(door, onGo)
    if (onClose) onClose()
  }
  const pick = (id) => { setText(''); setResult(answer(id, hq)) }
  const submit = (e) => {
    e.preventDefault()
    if (!text.trim()) return
    setResult(ask(text, hq))
  }

  return <div className="ws-ask">
    {/* The one line that is always here, before anything is asked or
      * answered (§3.5). It is not a footnote and it is not dismissible. */}
    <p className="ws-ask-permanent">{A.permanent}</p>

    <div className="ws-ask-scope">
      <span className="ws-ask-scope-h">{A.scope.heading}</span>
      {scoped
        ? <span className="ws-ask-chip">
          {chip.text}
          <button type="button" className="ws-ask-chip-x" onClick={() => { setScoped(false); setResult(null) }}>{A.scope.remove}</button>
        </span>
        : null}
    </div>

    {scoped
      ? <>
        {/* The questions come BEFORE the box: a person who has never used this
          * sees what it can answer rather than an empty field (§3.5). */}
        <h3 className="ws-ask-suggest-h">{A.suggestions.heading}</h3>
        <p className="ws-change-muted">{A.suggestions.hint}</p>
        <ul className="ws-ask-suggest">
          {offered.map((q) => <li key={q.id}>
            <button type="button" className="ws-change-secondary ws-ask-suggest-btn" onClick={() => pick(q.id)}>{q.question}</button>
          </li>)}
        </ul>

        <form className="ws-ask-form" onSubmit={submit}>
          <label className="ws-ask-label" htmlFor="ws-ask-box">{A.box.label}</label>
          <input
            id="ws-ask-box"
            className="ws-ask-box"
            type="text"
            value={text}
            placeholder={A.box.placeholder}
            onChange={(e) => setText(e.target.value)}
          />
          {/* Off only while there is nothing to ask, and it says so (rule 6). */}
          {text.trim()
            ? <button type="submit" className="ws-change-secondary ws-ask-submit">{A.box.submit}</button>
            : <Blocked id="ws-ask-why" label={A.box.submit} pair={A.box.submit_why} />}
        </form>

        {result
          ? <>
            {result.answered ? <AnswerCard result={result} onDoor={go} /> : <CannotCard result={result} />}
            <button type="button" className="ws-change-secondary ws-ask-again" onClick={() => { setResult(null); setText('') }}>{A.answer.another}</button>
          </>
          : null}
      </>
      : <>
        {/* The chip is removable, and with it gone Ask says what it has left
          * to look in — which is nothing — and offers the one act that puts it
          * back. It never answers out of scope. */}
        <p className="ws-ask-none">{A.scope.none}</p>
        <div data-primary-action="">
          <button type="button" className="ws-change-primary ws-ask-restore" onClick={() => setScoped(true)}>{A.scope.restore}</button>
        </div>
      </>}
  </div>
}

export default AskPanel
