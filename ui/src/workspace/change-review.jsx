/* T-4-1 — what review found, on the Change screen. T-4-2 — and where each
 * thing it found stands.
 *
 * Lays out what `buildChangeReviews` (review-model.js) decided. It derives
 * nothing: every sentence is already filled from `HQ_COPY.change.review`, and
 * every number is a typed field of the record. It reaches the two GET
 * drill-downs through `readProjection`, and — since T-4-2 — exactly ONE write
 * address, `CHANGE_FINDING_STANDING`, declared beside the one control that
 * uses it (`StandingForm`). That control is secondary, never the screen's
 * primary, and it writes only when a person presses it with their name in the
 * box. `change-workspace.test.js` counts it in the census of write addresses.
 *
 * Plain first, technical one level down (03-USER-LENS rule 1): who looked, at
 * which version, whether it is still this version, what it raised and its own
 * opinion are on the surface; the record's ids, digests, route and the raw and
 * parsed report are inside the `<details>` below each review.
 */
import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { readProjection, refusalCode, writeAction } from './data.js'
import { fill } from './hq-narrate.js'
import { reviewRefusalText, SETS_ASIDE, standingFormState, standingRefusalText } from './review-model.js'
import { Blocked } from './blocked.jsx'

const RC = HQ_COPY.change.review
const SC = RC.standing

/* Layout only — no colour, no motion, no outline (visual-system.test.js). A
 * long report wraps and scrolls inside its own box instead of widening the page. */
const PRE = Object.freeze({ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', maxHeight: '24rem', overflow: 'auto' })

/** One stored report, read on request as the server's masked copy. */
function Inspect({ url, label }) {
  const [state, setState] = React.useState({ status: 'idle' })
  if (!url) return null
  const open = () => {
    setState({ status: 'loading' })
    readProjection(url).then((read) => {
      if (read.status === 'ready' && read.doc) setState({ status: 'ready', doc: read.doc })
      else setState({ status: 'refused', text: reviewRefusalText(refusalCode(read)) })
    })
  }
  return <div className="ws-review-inspect">
    <button type="button" className="ws-change-secondary" onClick={open}>{label}</button>
    {state.status === 'loading' ? <p className="ws-change-muted">{RC.loading}</p> : null}
    {state.status === 'refused' ? <p className="ws-change-muted" role="status">{state.text}</p> : null}
    {state.status === 'ready'
      ? <>
        <p className="ws-change-muted">{RC.copy_note}</p>
        <pre className="ws-review-pre" style={PRE} tabIndex={0}>{JSON.stringify(state.doc.content, null, 2)}</pre>
      </>
      : null}
  </div>
}

/** One review: the plain answer, then the record one click deeper. */
export function ReviewBlock({ view }) {
  if (!view) return null
  return <article className="ws-review" data-review-state={view.state} data-review-integrity={view.integrity} data-applies={view.applies ? 'yes' : 'no'}>
    <p className="ws-change-request">{view.who}</p>
    {view.where ? <p className="ws-change-request">{view.where}</p> : null}
    {view.status ? <p className="ws-change-request ws-review-status">{view.status}</p> : null}
    {view.found ? <p className="ws-change-request">{view.found}</p> : null}
    {view.findings.length
      ? <ol className="ws-change-criteria ws-review-findings">{view.findings.map((f, i) => <li key={i}>
        {f.where ? <span className="ws-change-muted">{f.where}: </span> : null}
        <strong>{f.kind}</strong>{f.message ? <> — {f.message}</> : null}
      </li>)}</ol>
      : null}
    {view.opinion ? <p className="ws-change-request ws-review-opinion">{view.opinion}</p> : null}
    {view.limits ? <p className="ws-change-muted">{view.limits}</p> : null}
    <details className="ws-review-technical">
      <summary>{RC.technical}</summary>
      <dl>
        {view.technical.map(([k, v]) => <React.Fragment key={k}><dt>{k}</dt><dd>{v}</dd></React.Fragment>)}
      </dl>
      <Inspect url={view.raw} label={RC.show_raw} />
      <Inspect url={view.parsed} label={RC.show_parsed} />
    </details>
  </article>
}

/**
 * Brief — the newest review, in the story, when there is one. No action of
 * its own: the Brief's one primary is the Inspector's, and this adds none.
 */
export function ReviewSummary({ reviews }) {
  if (!reviews || !reviews.any) return null
  return <section className="ws-review-summary" aria-label={RC.heading}>
    <h2 className="ws-review-heading">{RC.heading}</h2>
    <ReviewBlock view={reviews.latest} />
    {reviews.earlierLine ? <p className="ws-change-muted">{reviews.earlierLine}</p> : null}
  </section>
}

/**
 * The Review stage — the newest review, then every earlier one, each saying
 * which version it read. The stage's one primary stays where it was and says
 * why it is off (`blocked.review_from_terminal`), rendered by the caller.
 */
export function ReviewSection({ reviews, empty }) {
  if (!reviews || !reviews.any) return empty
  return <>
    <ReviewBlock view={reviews.latest} />
    {reviews.earlier.length
      ? <details className="ws-review-earlier">
        <summary>{reviews.earlierLine}</summary>
        {reviews.earlier.map((v) => <section key={v.id} aria-label={fill(RC.round, { round: v.round })}>
          <h3>{fill(RC.round, { round: v.round })}</h3>
          <ReviewBlock view={v} />
        </section>)}
      </details>
      : null}
  </>
}

/* ── T-4-2: where each thing review found stands ──────────────────────── */

/** The one address this file writes to: one finding's standing. There is only one. */
export const CHANGE_FINDING_STANDING = (id, finding) =>
  '/api/changes/' + encodeURIComponent(id) + '/findings/' + encodeURIComponent(finding) + '/standing'

/**
 * Change where one finding stands. A SECONDARY control inside a closed
 * `<details>`: the screen's one primary stays where it is. Save is off until
 * the choice can succeed, and says why and what turns it on (rule 6). A
 * refused save says so in the copy file's words and never reads as saved.
 */
export function StandingForm({ item, changeId, onSaved }) {
  const [form, setForm] = React.useState({ choice: '', actor: '', authority: '', reason: '', evidence: '' })
  const [busy, setBusy] = React.useState(false)
  const [refusal, setRefusal] = React.useState(null)
  const set = (k) => (e) => { setRefusal(null); setForm({ ...form, [k]: e.target.value }) }
  const state = standingFormState(item, form)
  const id = 'ws-standing-' + item.key.replace(/[^A-Za-z0-9_-]/g, '-')
  const save = async () => {
    setBusy(true)
    setRefusal(null)
    const answer = await writeAction(CHANGE_FINDING_STANDING(changeId, item.key), state.body)
    setBusy(false)
    if (answer.status !== 'ready') { setRefusal(standingRefusalText(refusalCode(answer))); return }
    if (onSaved) onSaved()
  }
  const setsAside = SETS_ASIDE.includes(form.choice)
  return <div className="ws-standing-form">
    <label className="ws-cov-actor">
      <span>{SC.choose}</span>
      <select value={form.choice} onChange={set('choice')} data-standing-choice="">
        <option value="">{SC.choose_none}</option>
        {item.options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
    </label>
    {form.choice === 'fixed_with_evidence' && item.evidence.length
      ? <label className="ws-cov-actor">
        <span>{SC.evidence_label}</span>
        <select value={form.evidence} onChange={set('evidence')} data-standing-evidence="">
          <option value="">{SC.choose_none}</option>
          {item.evidence.map((e) => <option key={e.value} value={e.value}>{e.label}</option>)}
        </select>
      </label>
      : null}
    <label className="ws-cov-actor">
      <span>{SC.reason_label}</span>
      <input type="text" value={form.reason} onChange={set('reason')} data-standing-reason="" />
    </label>
    {setsAside
      ? <label className="ws-cov-actor">
        <span>{SC.authority_label}</span>
        <input type="text" value={form.authority} onChange={set('authority')} data-standing-authority="" />
      </label>
      : null}
    <label className="ws-cov-actor">
      <span>{SC.actor_label}</span>
      <input type="text" value={form.actor} onChange={set('actor')} data-standing-actor="" />
    </label>
    {state.ready
      ? <button type="button" className="ws-change-secondary" disabled={busy} onClick={save}>{busy ? SC.saving : SC.save}</button>
      : <Blocked id={id + '-why'} label={SC.save} pair={state.blocked} />}
    <p className="ws-change-muted">{SC.writes}</p>
    {refusal ? <p className="ws-change-notes" role="alert">{refusal}</p> : null}
  </div>
}

/** One finding: where it is, where it stands in plain words, then the record one click down. */
export function StandingItem({ item, changeId, onSaved }) {
  return <li className="ws-standing" data-standing={item.standing} data-group={item.group}>
    <p className="ws-change-request">
      {item.where ? <span className="ws-change-muted">{item.where}: </span> : null}
      <strong>{item.kind}</strong>{item.message ? <> — {item.message}</> : null}
    </p>
    {item.fixedLine
      ? <>
        <p className="ws-change-request ws-standing-now"><strong>{item.fixedLine}</strong></p>
        {item.fixed ? null : <p className="ws-change-request">{item.meaning}</p>}
        <p className="ws-change-request ws-standing-recheck">{item.recheckSaid}</p>
        {item.recheckLines.length ? <ul className="ws-standing-recheck-list">{item.recheckLines.map((l, n) => <li key={n}>{l}</li>)}</ul> : null}
      </>
      : <p className="ws-change-request ws-standing-now"><strong>{item.label}.</strong> {item.meaning}</p>}
    {item.why ? <p className="ws-change-request">{item.why}</p> : null}
    {item.who ? <p className="ws-change-muted">{item.who}</p> : null}
    {item.shownBy ? <p className="ws-change-request ws-standing-shown">{item.shownBy}</p> : null}
    {item.recheck ? <Inspect url={item.recheck} label={SC.show_recheck} /> : null}
    <p className="ws-change-muted">{item.raised}</p>
    <details className="ws-standing-change">
      <summary>{SC.change}</summary>
      <StandingForm item={item} changeId={changeId} onSaved={onSaved} />
    </details>
    {item.history.length
      ? <details className="ws-standing-history">
        <summary>{item.historyHeading}</summary>
        <ul>{item.history.map((h, n) => <li key={n}>{h}</li>)}</ul>
      </details>
      : null}
    <details className="ws-review-technical">
      <summary>{SC.technical}</summary>
      <dl>{item.technical.map(([k, v]) => <React.Fragment key={k}><dt>{k}</dt><dd>{v}</dd></React.Fragment>)}</dl>
    </details>
  </li>
}

/**
 * Where each thing review found stands, grouped: still wrong, marked resolved
 * and waiting for a re-check, fixed with a re-check that shows it, set aside
 * by a person, and for information only. It adds no primary action.
 */
export function FindingStandings({ standings, changeId, onSaved }) {
  if (!standings) return null
  return <section className="ws-standings" aria-label={standings.heading}>
    <h2 className="ws-review-heading">{standings.heading}</h2>
    {!standings.any
      ? <p className="ws-change-muted">{standings.none}</p>
      : <>
        <p className="ws-change-muted">{standings.intro}</p>
        {standings.summary.map((line, n) => <p key={n} className="ws-change-request ws-standings-summary">{line}</p>)}
        {standings.groups.map((g) => <section key={g.key} className="ws-standings-group" data-group={g.key} aria-label={g.heading}>
          <h3>{g.heading}</h3>
          <ol className="ws-change-criteria">{g.items.map((i) => <StandingItem key={i.key} item={i} changeId={changeId} onSaved={onSaved} />)}</ol>
        </section>)}
      </>}
  </section>
}
