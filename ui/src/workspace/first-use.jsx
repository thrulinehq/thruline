import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { API, CHANGE_PROPOSALS, FIRST_USE_PROPOSAL, readProjection, writeAction } from './data.js'
import { storageReading } from './inbox-view.js'
import { candidateRows, rowFacts, pickAction, proposalView, checkAvailability, criteriaLines, scopeLines } from './first-use-view.js'
import { runCheckCommand } from './coverage-view.js'
import { INBOX_HASH, goToInbox } from '../routes.js'
import './workspace.css'

/* The first-use path — install, pick, read one true thing (T-2-7;
 * `B11.5`–`B11.7`, `B8.4`, `03-USER-LENS.md` rule 8 and §4 step 1, `L1.1`).
 *
 * WHAT THIS SCREEN IS. The first thing a person meets on a machine that has
 * picked nothing yet. Three steps and one primary action each:
 *
 *   1. PICK — repositories with recorded work and other folders the person can
 *      start from, each with what is visible about it from typed fields, and the sentence about
 *      what ThruLine will keep BEFORE anything is written down (rule 13).
 *   2. READ — the Change the project's most recent recorded work would make,
 *      named in the person's own recorded words, with what ThruLine suggests
 *      checking and what this machine cannot run. Or, when nothing is recorded
 *      for it, the sentence that says exactly that and the one action that
 *      would change it — never "setup required" (`B11.7`).
 *   3. DONE — the Change is written down; the primary action goes to the Inbox,
 *      where its reason is the first true statement about this person's own
 *      project. Accepting what counts as done is offered here as a SECOND,
 *      SEPARATE click and is never required to get to the Inbox (`B5.7`).
 *
 * WHAT IT MAY WRITE. Exactly three things, one click each, each on a control
 * whose label says what it writes: the picked project's container and the
 * recordings already on disk for it (`POST /api/first-use/register`), the
 * Change with the work confirmed (`POST /api/first-use/change`), and — only if
 * a person clicks the second control — one accepted scope version
 * (`POST /api/changes/<id>/scope/accept`). It never writes a decision, an
 * approval or a result.
 *
 * WHAT IT MAY SAY. Every user-visible string comes from `HQ_COPY.first_use`
 * (and the storage line from `HQ_COPY.change.inbox.storage`, the Inbox's own
 * `L1.1` words, so both screens say one number one way), with two disclosed
 * exceptions, both of which are the RECORD speaking rather
 * than this product: the repository path a recording names, and the words the
 * person themself used when they asked for the work (the Change's title, its
 * request, and the suggested criteria — all masked by the engine before they
 * are served). No state word, no percentage, nothing green, and no number that
 * is not a typed field of a served document.
 */

/**
 * What counts as done, accepted (`POST /api/changes/<id>/scope/accept`).
 *
 * A SEPARATE ACT from writing the Change, on a separate route, and the only
 * Change write this build makes (`B5.7`, T-2-3). It is written down here
 * rather than in `data.js` because T-2-4 pins that the read layer holds no
 * Change write address — the Change screen must not be able to reach one.
 */
const SCOPE_ACCEPT = (id) => '/api/changes/' + encodeURIComponent(id) + '/scope/accept'

const C = HQ_COPY.first_use

/* ── reading ──────────────────────────────────────────────────────────── */

/** What this machine already has. One read, no write. */
export async function readFirstUse() {
  const read = await readProjection(API.firstUse)
  if (read.status === 'refused') return { status: 'refused', refusal: read.refusal }
  const overview = candidateRows(read.doc || null)
  return { status: 'ready', step: 'pick', ...overview, selected: null, storage: storageReading({ bytes: overview.storeBytes, path: overview.storeFolder }) }
}

/** The Change this project's most recent recorded work would make. */
export async function readProposal(project) {
  const read = await readProjection(FIRST_USE_PROPOSAL(project))
  if (read.status === 'refused') return { status: 'refused', refusal: read.refusal }
  return { status: 'ready', proposal: proposalView(read.doc || null) }
}

/* ── the pieces ───────────────────────────────────────────────────────── */

/**
 * What this product will keep, and where — and what it is keeping already,
 * as a real number (rule 13, `L1.1`). The folder is one click down, like
 * every other path.
 *
 * T-7-1 rehearsal: the second line used to say "Nothing is written there until
 * you pick a project below." while the folder already held the record store
 * and a first reading of the machine, written when `thruline ui` started. It
 * now says what is there, measured by the server, in the Inbox's own words
 * (`change.inbox.storage`), so the two screens cannot give two numbers.
 */
export function Storage({ storage }) {
  const s = storage || { known: false }
  const kept = HQ_COPY.change.inbox.storage
  return <section className="ws-inbox-storage ws-first-storage">
    <p className="ws-inbox-storage-line">{C.storage.sentence}</p>
    <p className="ws-inbox-muted">{s.known ? fill(kept.sentence, { size: s.size }) : kept.unknown} {kept.note}</p>
    {s.path ? <details className="ws-inbox-technical">
      <summary>{C.storage.where_open}</summary>
      <dl><div><dt>{C.storage.where_folder}</dt><dd>{s.path}</dd></div></dl>
    </details> : null}
  </section>
}

/** One folder offered by the server, whether or not it has recordings. */
export function Candidate({ row, picked, onSelect }) {
  const facts = rowFacts(row)
  return <li className="ws-first-candidate">
    <label className="ws-first-pick">
      <input
        type="radio"
        name="ws-first-project"
        value={row.key}
        checked={picked}
        onChange={() => onSelect && onSelect(row.key)}
      />
      {/* The repository as the record names it: the record speaking, not this
          product. It is the one thing on this screen a person recognises. */}
      <span className="ws-first-label">{row.label}</span>
      <span className="ws-first-facts">{facts.map((f, i) => <span key={i} className="ws-first-fact">{fill(C.pick[f.key], f.values)}</span>)}</span>
    </label>
  </li>
}

/** The one primary action, or the pair that says why it is off (rule 6). */
export function Primary({ action, onClick, id }) {
  if (action.enabled) {
    return <>
      {/* An action that is not the ordinary one says so BEFORE it is clicked.
          `pickAction`'s `inbox` answer is the case: the project is already
          picked and already tracked, and this control goes to where its work
          already is rather than writing it down again. */}
      {action.note ? <p className="ws-inbox-muted" id={id}>{action.note}</p> : null}
      <button type="button" className="ws-inbox-primary" onClick={onClick} aria-describedby={action.note ? id : undefined}>{action.label}</button>
    </>
  }
  return <>
    <button type="button" className="ws-inbox-primary" disabled aria-describedby={id}>{action.label}</button>
    <p className="ws-inbox-muted" id={id}>{action.reason} {action.enabler}</p>
  </>
}

/** The command that would put something here, prepared — never run. */
export function Command({ command, label, onCopy }) {
  return <>
    <p className="ws-inbox-command"><code>{command}</code></p>
    <button type="button" className="ws-inbox-primary" onClick={() => onCopy && onCopy(command)}>{label}</button>
  </>
}

/* ── step 1: pick what you are working on ─────────────────────────────── */

export function Pick({ state, onSelect, onUse, onCopy }) {
  const rows = state.rows || []
  const recorded = rows.filter((row) => row.recorded > 0)
  const other = rows.filter((row) => !(row.recorded > 0))
  const action = pickAction(state.selected, rows)
  return <>
    <h2 className="ws-inbox-empty-heading">{C.pick.heading}</h2>
    {rows.length ? <>
      {recorded.length ? <section aria-labelledby="ws-first-recorded">
        <h3 className="ws-inbox-empty-heading" id="ws-first-recorded">{C.pick.body}</h3>
        <ul className="ws-first-list">
          {recorded.map((row) => <Candidate key={row.key} row={row} picked={state.selected === row.key} onSelect={onSelect} />)}
        </ul>
      </section> : null}
      {other.length ? <section aria-labelledby="ws-first-other">
        <h3 className="ws-inbox-empty-heading" id="ws-first-other">{C.pick.other_heading}</h3>
        {!recorded.length ? <p className="ws-inbox-muted">{rows.every((row) => row.recorded === 0) ? C.pick.none_recorded : C.pick.unconfirmed}</p> : null}
        <ul className="ws-first-list">
          {other.map((row) => <Candidate key={row.key} row={row} picked={state.selected === row.key} onSelect={onSelect} />)}
        </ul>
      </section> : null}
      <Storage storage={state.storage} />
      {state.busy ? <p className="ws-inbox-muted" role="status">{C.pick.working}</p>
        : <Primary action={action} onClick={onUse} id="ws-first-why-pick" />}
    </> : <><section className="ws-inbox-empty" data-empty="no_project">
      {/* T-6-6 (`HQ_COPY.empty.no_project`): what this is, why it is empty,
          and the one thing that fills it. */}
      <h3 className="ws-inbox-empty-heading">{C.pick.none.heading}</h3>
      <p className="ws-inbox-muted">{HQ_COPY.empty.no_project.what}</p>
      <p className="ws-inbox-empty-body">{C.pick.none.body}</p>
      <p className="ws-inbox-empty-body">{HQ_COPY.empty.no_project.fill}</p>
      <p className="ws-inbox-muted">{C.pick.none.how}</p>
      <Command command={C.propose.none.command} label={C.propose.none.copy_command} onCopy={onCopy} />
      <p className="ws-inbox-muted">{C.propose.none.installs}</p>
    </section>
    {/* T-7-1 rehearsal: an empty machine is still keeping something the
        moment this screen is served, and `L1.1` says so on the first screen a
        person meets, whether or not there is anything to pick. */}
    <Storage storage={state.storage} /></>}
    {state.failed ? <p className="ws-inbox-muted" role="alert">{fill(C.pick.failed, { reason: state.failed })}</p> : null}
  </>
}

/* ── step 2: the Change the record already holds ──────────────────────── */

export function Propose({ state, onStart, onCopy }) {
  const p = state.proposal || { have: false }
  if (!p.have) {
    /* `B11.7`: the honest absence, with the one action that would change it.
       Never "setup required", and never an empty screen. */
    return <section className="ws-inbox-empty" data-empty="no_recordings">
      <h2 className="ws-inbox-empty-heading">{C.propose.none.heading}</h2>
      {/* T-6-6: what this is. The body is `empty.no_recordings.why` + `.fill`,
          composed in the copy file, so the card's verbatim sentence stands. */}
      <p className="ws-inbox-muted">{HQ_COPY.empty.no_recordings.what}</p>
      <p className="ws-inbox-empty-body">{C.propose.none.body}</p>
      <p className="ws-inbox-muted">{C.propose.none.how}</p>
      <Command command={C.propose.none.command} label={C.propose.none.copy_command} onCopy={onCopy} />
      <p className="ws-inbox-muted">{C.propose.none.installs}</p>
    </section>
  }
  const touched = p.touched == null ? null
    : p.touched === 0 ? C.propose.touched_none
      : p.touched === 1 ? C.propose.touched_one
        : fill(C.propose.touched_many, { n: p.touched })
  return <>
    <h2 className="ws-inbox-empty-heading">{C.propose.heading}</h2>
    <p className="ws-inbox-muted">{C.propose.body}</p>
    <div className="ws-inbox-item">
      {/* The person's own recorded words, masked by the engine before they
          were served. Never rewritten here, never summarised. */}
      <h3 className="ws-inbox-title">{p.title}</h3>
      <p className="ws-inbox-also-label">{C.propose.asked_heading}</p>
      <p className="ws-inbox-reason">{p.request}</p>
      {touched ? <p className="ws-inbox-muted">{touched}</p> : null}
    </div>
    {p.checks.length ? <section className="ws-first-checks">
      <h3 className="ws-inbox-empty-heading">{C.propose.checks_heading}</h3>
      <p className="ws-inbox-muted">{C.propose.checks_note}</p>
      <ul className="ws-first-check-list">
        {p.checks.map((check) => {
          const absent = checkAvailability(check)
          return <li key={check.id}>
            <span className="ws-first-check-tool">{check.tool}</span>
            <span className="ws-inbox-muted"> · {check.mandatory ? C.propose.check_mandatory : C.propose.check_advisory}</span>
            {absent ? <span className="ws-first-absent"> · {fill(C.propose[absent.key], absent.values)}</span> : null}
          </li>
        })}
      </ul>
      {p.absent ? <p className="ws-inbox-muted">{C.propose.unavailable_note}</p> : null}
      {p.needsSettings ? <p className="ws-inbox-muted">{C.propose.needs_settings_note}</p> : null}
    </section> : null}
    {state.busy ? <p className="ws-inbox-muted" role="status">{C.propose.working}</p>
      : <Primary action={{ enabled: true, label: C.propose.action }} onClick={onStart} id="ws-first-why-start" />}
    {state.failed ? <p className="ws-inbox-muted" role="alert">{fill(C.propose.failed, { reason: state.failed })}</p> : null}
  </>
}

/* ── step 3: it is written down ───────────────────────────────────────── */

export function Done({ state, onAccept, onGoToInbox, onCopy }) {
  const criteria = state.criteria || []
  const covers = state.covers || []
  return <>
    {/* Arrived at a second time, nothing was written — and the screen does not
        claim an act this product did not perform. */}
    <h2 className="ws-inbox-empty-heading">{state.already ? C.done.already_heading : C.done.heading}</h2>
    <p className="ws-inbox-empty-body">{state.already ? C.done.already_body : C.done.body}</p>
    {/* THE one primary action: the Inbox, where this Change's own reason is
        the first true statement about this person's project. */}
    {/* A link, so it can be opened the way any address is opened; the click
        itself also boots the screen it names, because this root cannot become
        another one (`routes.js` `goToInbox`, the same answer T-2-6 gave for
        the Inbox's own items). */}
    <a
      className="ws-inbox-primary"
      href={INBOX_HASH}
      onClick={(e) => { if (onGoToInbox) { e.preventDefault(); onGoToInbox() } }}
    >{C.done.action}</a>
    <section className="ws-first-criteria">
      <h3 className="ws-inbox-empty-heading">{C.done.criteria_heading}</h3>
      {state.accepted ? <p className="ws-inbox-muted">{fill(C.done.criteria_accepted, { list: state.accepted })}</p>
        : criteria.length ? <>
          <ol className="ws-change-criteria">{criteria.map((c, i) => <li key={i}>{c.text}</li>)}</ol>
          {/* What the same click covers (T-3-2b). Shown BEFORE it is clicked,
              because a person cannot agree to folders they were not shown. */}
          <h3 className="ws-inbox-empty-heading">{C.done.covers_heading}</h3>
          {covers.length
            ? <>
              <ul className="ws-first-covers">{covers.map((p, i) => <li key={i}><code>{p}</code></li>)}</ul>
              <p className="ws-inbox-muted">{C.done.covers_note}</p>
            </>
            : <p className="ws-inbox-muted">{C.done.covers_none}</p>}
          <p className="ws-inbox-muted">{C.done.criteria_note}</p>
          {state.busy ? <p className="ws-inbox-muted" role="status">{C.done.criteria_working}</p>
            : <button type="button" className="ws-first-secondary" onClick={onAccept}>{C.done.criteria_action}</button>}
        </> : <p className="ws-inbox-muted">{C.done.criteria_none}</p>}
      {state.failed ? <p className="ws-inbox-muted" role="alert">{fill(C.done.criteria_failed, { reason: state.failed })}</p> : null}
    </section>
    {/* The next thing worth doing, prepared and never run — the same command
        the Change screen offers, from the same builder. */}
    {state.change ? <section className="ws-first-check">
      <h3 className="ws-inbox-empty-heading">{C.done.check_heading}</h3>
      {/* Secondary by construction: the Inbox link is this screen's one
          primary action, and a second thing styled like it would be two. */}
      <p className="ws-inbox-command"><code>{runCheckCommand(state.change)}</code></p>
      <button type="button" className="ws-first-secondary" onClick={() => onCopy && onCopy(runCheckCommand(state.change))}>{C.done.check_action}</button>
      <p className="ws-inbox-muted">{C.done.check_note}</p>
    </section> : null}
  </>
}

/* ── the screen ───────────────────────────────────────────────────────── */

/**
 * @param {object} props
 * @param {object} props.state what the reads and the clicks left behind
 */
export function FirstUseScreen({ state, onSelect, onUse, onStart, onAccept, onCopy, onGoToInbox }) {
  if (state.status === 'loading') {
    return <main className="ws-inbox ws-first"><div className="ws-inbox-inner"><p className="ws-inbox-muted" role="status">{C.looking}</p></div></main>
  }
  if (state.status === 'refused') {
    /* Rule 12: a read that failed is said, and nothing is claimed about what
       this machine does or does not have. */
    return <main className="ws-inbox ws-first"><div className="ws-inbox-inner">
      <h1 className="ws-inbox-heading">{C.title}</h1>
      <p className="ws-inbox-muted">{fill(C.failed, { reason: state.refusal || '' })}</p>
    </div></main>
  }
  return <main className="ws-inbox ws-first"><div className="ws-inbox-inner">
    <h1 className="ws-inbox-heading">{C.title}</h1>
    {state.step === 'pick' ? <Pick state={state} onSelect={onSelect} onUse={onUse} onCopy={onCopy} /> : null}
    {state.step === 'propose' ? <Propose state={state} onStart={onStart} onCopy={onCopy} /> : null}
    {state.step === 'done' ? <Done state={state} onAccept={onAccept} onGoToInbox={onGoToInbox} onCopy={onCopy} /> : null}
    {(state.notes || []).length ? <ul className="ws-inbox-notes">{(state.notes || []).map((n, i) => <li key={i}>{n}</li>)}</ul> : null}
  </div></main>
}

/** The same screen, reading and writing for itself. What `main.jsx` mounts. */
export default function FirstUse() {
  const [state, setState] = React.useState({ status: 'loading' })
  React.useEffect(() => {
    let live = true
    readFirstUse().then((r) => { if (live) setState(r) })
    return () => { live = false }
  }, [])

  const copy = (text) => { try { navigator.clipboard.writeText(text) } catch { /* nothing else to do */ } }
  const select = (key) => setState((s) => ({ ...s, selected: key, failed: null }))

  /* One click: the picked project's container and what is already on disk for
     it. Then the Change it would make, read — not written.
     A project that is already picked and already tracked is not registered a
     second time and proposes nothing a second time: the click goes to the Inbox,
     where that work already is. */
  const use = async () => {
    const key = state.selected
    if (!key) return
    if (pickAction(key, state.rows || []).kind === 'inbox') {
      goToInbox(typeof window === 'undefined' ? null : window)
      return
    }
    setState((s) => ({ ...s, busy: true, failed: null }))
    const written = await writeAction(API.firstUseRegister, { key })
    if (written.status !== 'ready') {
      setState((s) => ({ ...s, busy: false, failed: written.refusal }))
      return
    }
    const project = written.doc && written.doc.project
    const read = await readProposal(project)
    if (read.status !== 'ready') {
      setState((s) => ({ ...s, busy: false, failed: read.refusal }))
      return
    }
    setState((s) => ({ ...s, busy: false, step: 'propose', project, proposal: read.proposal }))
  }

  /* One click: the Change, with the work that was on screen confirmed. It
     writes no scope version — accepting what counts as done is the separate
     click below, and a test asserts this one leaves that table empty. */
  const start = async () => {
    setState((s) => ({ ...s, busy: true, failed: null }))
    const written = await writeAction(API.firstUseChange, { project: state.project })
    if (written.status !== 'ready') {
      setState((s) => ({ ...s, busy: false, failed: written.refusal }))
      return
    }
    const change = written.doc && written.doc.change
    /* The server refuses to write one piece of recorded work down twice, and
       says so in a typed field. The screen says the same thing: nothing was
       written, and here is the Change that already holds it. */
    const already = !!(written.doc && written.doc.already_tracked === true)
    const proposals = await readProjection(CHANGE_PROPOSALS(change))
    setState((s) => ({
      ...s,
      busy: false,
      step: 'done',
      change,
      already,
      criteria: proposals.status === 'ready' ? criteriaLines(proposals.doc || null) : [],
      covers: proposals.status === 'ready' ? scopeLines(proposals.doc || null) : [],
    }))
  }

  /* The second, separate act. Offered; never required to reach the Inbox. */
  const accept = async () => {
    setState((s) => ({ ...s, busy: true, failed: null }))
    const written = await writeAction(SCOPE_ACCEPT(state.change), { all: true, actor: 'you' })
    if (written.status !== 'ready') {
      setState((s) => ({ ...s, busy: false, failed: written.refusal }))
      return
    }
    const version = written.doc && written.doc.accepted ? written.doc.accepted.version : null
    setState((s) => ({ ...s, busy: false, accepted: version }))
  }

  const toInbox = () => goToInbox(typeof window === 'undefined' ? null : window)
  return <FirstUseScreen state={state} onSelect={select} onUse={use} onStart={start} onAccept={accept} onCopy={copy} onGoToInbox={toInbox} />
}

/**
 * The landing: this screen on a machine with nothing picked yet, the Inbox on
 * one that has picked something.
 *
 * `thruline ui` opens the browser on the address this mounts, and the CLI adds
 * `#/start` when the store holds no project — so the first screen is right
 * whether a person arrives from the command or from a bookmark. The decision
 * is made HERE rather than in `main.jsx` because the app reads the summary
 * index exactly once there and nowhere else (pinned, `lib/arcade.test.js`).
 *
 * BOTH ADDRESSES COME THROUGH HERE. `main.jsx` mounts this for the bare address
 * AND for `#/start`; the pick screen is never mounted by an address alone. It
 * used to be, and a bookmarked `#/start` on a machine that had already picked a
 * project re-opened "pick a project" and let the same recorded work be written
 * down as a second Change — two identical items in the Inbox with nothing on
 * screen to say why there were two.
 */

/**
 * Which of the two screens an address should open, from the one read.
 *
 * TWO ADDRESSES, TWO SENTENCES, AND THE STORE DECIDES BOTH.
 *
 *  - the bare address is the Inbox, unless nothing has been picked yet — then
 *    it is the first screen, because an Inbox that can only say it is empty is
 *    the dead end this path exists to remove (T-2-6, `B8.1`);
 *  - `#/start` is the pick screen, unless there is nothing left to pick —
 *    every repository this machine can see is already picked AND already has a
 *    Change — and then it is the Inbox too.
 *
 * The second sentence is the fix for a real defect. `#/start` is what
 * `thruline ui` prints on a first run, so it is the address people bookmark;
 * it used to re-open "pick a project" unconditionally, and each visit walked a
 * person into writing the same recorded work down again. Three visits, three
 * identical Inbox items, nothing on screen to say why there were three.
 *
 * It is NOT "any registered project sends you to the Inbox": that would leave a
 * person with a second repository and no door to it at all. What sends you to
 * the Inbox is having nothing left to do on this screen.
 *
 * @param {object|null} read the result of reading `GET /api/first-use`
 * @param {string} address `'inbox'` for the bare address, `'first-use'` for `#/start`
 */
export function landingScreen(read, address) {
  /* A server that cannot answer leaves the Inbox as the landing: the Inbox says
     its own honest thing about a store it cannot read, and guessing "you have
     nothing" would be this product inventing a fact about a person's machine.
     A count that is not a number is the same case. */
  const ready = !!(read && read.status === 'ready' && read.doc && typeof read.doc.registered === 'number')
  if (!ready) return 'inbox'
  if (read.doc.registered === 0) return 'first-use'
  if (address !== 'first-use') return 'inbox'
  const { rows } = candidateRows(read.doc)
  return rows.some((row) => !(row.registered && row.changes > 0)) ? 'first-use' : 'inbox'
}

export function Landing({ address }) {
  const [at, setAt] = React.useState({ screen: 'looking' })
  React.useEffect(() => {
    let live = true
    readProjection(API.firstUse).then((read) => {
      if (!live) return
      setAt({ screen: landingScreen(read, address) })
    })
    return () => { live = false }
  }, [address])
  const [Inbox, setInbox] = React.useState(null)
  React.useEffect(() => {
    let live = true
    if (at.screen !== 'inbox') return () => { live = false }
    import('./inbox.jsx').then((mod) => { if (live) setInbox(() => mod.default) })
    return () => { live = false }
  }, [at.screen])
  if (at.screen === 'looking') {
    return <main className="ws-inbox ws-first"><div className="ws-inbox-inner"><p className="ws-inbox-muted" role="status">{C.looking}</p></div></main>
  }
  if (at.screen === 'first-use') return <FirstUse />
  return Inbox ? <Inbox /> : <main className="ws-inbox"><div className="ws-inbox-inner"><p className="ws-inbox-muted" role="status">{HQ_COPY.change.inbox.reading}</p></div></main>
}
