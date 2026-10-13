import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { Blocked } from './blocked.jsx'
import { oneAtATime, refusalText, runSheetModel } from './run-route.js'

/* P6-5 (design record §3.7) — the run sheet: Check it again, without a
 * terminal.
 *
 * WHAT IT SAYS. What will run, where, what it can and cannot establish, and —
 * once per project — the permission, with the promise "It does not install
 * anything and does not touch the network." Then one control: Run the check,
 * which is Cancel while the run is going. Then the last outcome, one of six,
 * in words, and only Passed reads as a pass. Every string is `HQ_COPY.run`;
 * every sentence and the choice of the one primary is `run-route.js`'s.
 *
 * WHAT IT MAY DO. Read the run sheet and the per-launch token, and write to
 * exactly the three addresses declared below, each with that token: start a
 * run, allow checks in this project, and (P6-5B) take that permission back.
 * The permission and its history are shown from the record's rows, never
 * from anything this browser remembers. Stopping a
 * run is the T-3-4 address, which `change-workspace.jsx` declares beside the
 * control that first used it and hands in as `onCancel`, so it is declared in
 * one place. The raw command is only ever inside the Technical drawer.
 *
 * A DOUBLE-CLICK IS ONE JOB. The press handler is wrapped in `oneAtATime`,
 * which drops every further press synchronously while one is being handled,
 * and the button is off while it is; the server refuses a second start under
 * its own lock and names the run that is already going.
 */

const R = HQ_COPY.run

/** The one read of this launch's token. */
export const RUN_TOKEN = '/api/run/token'
/** The run sheet (GET) and the one address a run is started at (POST). */
export const RUN_SHEET = (id) => '/api/changes/' + encodeURIComponent(id) + '/run'
/** The one address a project's permission is given at (POST). */
export const RUN_PERMISSION = (id) => '/api/changes/' + encodeURIComponent(id) + '/run/permission'
/** P6-5B — the one address that permission is taken back at (POST). */
export const RUN_PERMISSION_REVOKE = (id) => '/api/changes/' + encodeURIComponent(id) + '/run/permission/revoke'

/** Where "I pasted it — track as pending" is kept: this browser, this Change. */
const pendingKey = (id) => 'thruline.run.pending.' + id

async function readJson(url) {
  try {
    const res = await fetch(url, { cache: 'no-store', headers: { Accept: 'application/json' } })
    const doc = await res.json().catch(() => null)
    return { ok: res.ok, status: res.status, doc }
  } catch (_e) {
    return { ok: false, status: 0, doc: null }
  }
}

/** Every write goes through here, and only the three declared above. */
async function postWithToken(url, body) {
  const token = await readJson(RUN_TOKEN)
  if (!token.ok || !token.doc || typeof token.doc.token !== 'string') {
    return { ok: false, status: token.status, doc: token.doc || { error: token.status ? 'run_token_required' : 'unreachable' } }
  }
  try {
    const res = await fetch(url, {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json', 'X-ThruLine-Run-Token': token.doc.token },
      body: JSON.stringify(body || {}),
    })
    const doc = await res.json().catch(() => null)
    return { ok: res.ok, status: res.status, doc }
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

function readPending(id) {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage.getItem(pendingKey(id))
  } catch (_e) {
    return null
  }
}

function writePending(id, value) {
  try {
    if (typeof localStorage === 'undefined') return
    if (value) localStorage.setItem(pendingKey(id), value)
    else localStorage.removeItem(pendingKey(id))
  } catch (_e) { /* a browser that keeps nothing keeps no reminder */ }
}

/**
 * The sheet's body: everything a person reads, from the model alone. Pure,
 * so each outcome renders the same in a test as on the screen.
 */
export function RunSheetBody({ model, busy, refusal, showPrompt, showElsewhere, copied, on }) {
  const act = on || {}
  if (!model || model.status !== 'ready') return <p className="ws-change-muted" role="status">{model ? model.loading : R.loading}</p>
  const o = model.outcome
  const primary = model.primary
  return <>
    {o
      ? <section className="ws-run-outcome" data-outcome={o.kind} data-passes={o.passes ? 'yes' : 'no'} aria-labelledby="ws-run-outcome-title">
        <h3 id="ws-run-outcome-title">{o.title}</h3>
        {o.lines.map((line, i) => <p key={i}>{line}</p>)}
        {o.failureLine
          ? <>
            <p className="ws-change-muted">{R.outcomes.failed.said}</p>
            <blockquote className="ws-run-said">{o.failureLine}</blockquote>
          </>
          : null}
        {o.when ? <p className="ws-change-muted">{fill(R.ran_at, { when: o.when })}</p> : null}
        {/* RUN-LINK-TRUTH (PW-F9): a pass no line counts yet says so, in the
          * Verify stage's own words, with one door to that line's existing
          * link control. The door navigates; it writes nothing. */}
        {model.unlinked
          ? <p className="ws-run-unlinked">
            {model.unlinked.sentence}{' '}
            <button type="button" className="ws-change-secondary ws-run-unlinked-door" onClick={() => act.door && act.door(model.unlinked.n)}>{model.unlinked.door}</button>
          </p>
          : null}
      </section>
      : null}
    {model.notStarted
      ? <section className="ws-run-outcome" data-outcome="did_not_start" data-passes="no">
        <h3>{model.notStarted.title}</h3>
        <p>{model.notStarted.line}</p>
      </section>
      : null}
    {model.runningLine ? <p className="ws-run-running" role="status">{model.runningLine}</p> : null}

    <dl className="ws-run-facts">
      <dt>{R.what_heading}</dt><dd>{model.what}</dd>
      {model.whereLine ? <><dt>{R.where_heading}</dt><dd>{model.whereLine}</dd></> : null}
      <dt>{R.can_heading}</dt><dd>{model.canLine}</dd>
      <dt>{R.cannot_heading}</dt><dd>{model.cannotLine}</dd>
    </dl>

    {model.permission.needed
      ? <section className="ws-run-permission" aria-labelledby="ws-run-permission-title">
        <h3 id="ws-run-permission-title">{model.permission.heading}</h3>
        <p>{model.permission.text}</p>
        <p className="ws-run-promise">{model.permission.promise}</p>
      </section>
      : <p className="ws-change-muted">{model.permission.given ? <>{model.permission.given} </> : null}{model.promise}</p>}

    {/* P6-5B — who allowed checks here, and every time it was taken back,
      * from the record. Taking it back is secondary: it never competes with
      * the one primary below. */}
    {model.consent && (model.consent.lines.length || model.consent.stateLine)
      ? <section className="ws-run-consent" aria-labelledby="ws-run-consent-title">
        <h3 id="ws-run-consent-title">{model.consent.heading}</h3>
        {model.consent.lines.length ? <ul className="ws-run-consent-lines">{model.consent.lines.map((line, i) => <li key={i}>{line}</li>)}</ul> : null}
        {model.consent.stateLine ? <p className="ws-run-consent-state">{model.consent.stateLine}</p> : null}
        {model.consent.canTakeBack
          ? <p>
            <button type="button" className="ws-change-secondary ws-run-take-back" disabled={busy} onClick={() => act.takeBack && act.takeBack()}>{busy ? R.working : model.consent.takeBack}</button>
            {' '}<span className="ws-change-muted">{model.consent.takeBackNote}</span>
          </p>
          : null}
      </section>
      : null}

    {model.pendingLine
      ? <p className="ws-run-pending">{model.pendingLine} <span className="ws-change-muted">{R.tool.pending_scope}</span></p>
      : null}

    <div className="ws-run-primary">
      {primary.blocked
        ? <Blocked id="ws-run-why" label={primary.label} pair={primary.blocked} primary />
        : <button type="button" className="ws-change-primary" disabled={busy} onClick={() => act.primary && act.primary(primary.act)}>{busy ? R.working : primary.label}</button>}
      {primary.note ? <p className="ws-change-muted">{primary.note}</p> : null}
      {model.places
        ? <button type="button" className="ws-change-secondary" onClick={() => act.elsewhere && act.elsewhere()}>{R.tool.look_elsewhere}</button>
        : null}
    </div>

    {model.toolMissing && showPrompt
      ? <section className="ws-run-prompt" aria-labelledby="ws-run-prompt-title">
        <h3 id="ws-run-prompt-title">{R.tool.prompt_heading}</h3>
        <pre className="ws-run-prompt-text">{model.toolMissing.prompt}</pre>
        <button type="button" className="ws-change-secondary" onClick={() => act.copy && act.copy(model.toolMissing.prompt)}>{R.tool.copy}</button>
        {copied ? <span className="ws-change-muted" role="status"> {R.tool.copied}</span> : null}
        <button type="button" className="ws-change-secondary" onClick={() => act.pasted && act.pasted()}>{R.tool.pasted}</button>
      </section>
      : null}

    {model.places && showElsewhere
      ? <section className="ws-run-elsewhere" aria-labelledby="ws-run-elsewhere-title">
        <h3 id="ws-run-elsewhere-title">{R.tool.elsewhere_heading}</h3>
        <p className="ws-change-muted">{R.tool.elsewhere_note}</p>
        {model.places.some((e) => e.hasTool)
          ? null
          : <p className="ws-change-muted">{R.tool.none_found}</p>}
        <ul className="ws-run-places">
          {model.places.map((e) => <li key={e.id}>
            <span>{e.label}</span> <span className="ws-change-muted">{e.hasLine}</span>
            {e.hasTool
              ? <button type="button" className="ws-change-secondary" disabled={busy} onClick={() => act.pick && act.pick(e.id)}>{R.tool.pick}</button>
              : <Blocked id={'ws-run-place-why-' + e.id} label={R.tool.pick} pair={R.tool.pick_off} />}
          </li>)}
        </ul>
      </section>
      : null}

    {refusal ? <p className="ws-change-notes" role="alert">{refusal}</p> : null}

    {/* The raw command lives here and nowhere else (design record §3.7). */}
    <details className="ws-run-technical">
      <summary>{R.technical.heading}</summary>
      <dl>
        {model.technical.command ? <><dt>{R.technical.command}</dt><dd><code>{model.technical.command}</code></dd></> : null}
        {model.technical.folder ? <><dt>{R.technical.folder}</dt><dd><code>{model.technical.folder}</code></dd></> : null}
        {model.technical.environment ? <><dt>{R.technical.environment}</dt><dd><code>{model.technical.environment}</code></dd></> : null}
        {model.technical.run ? <><dt>{R.technical.run}</dt><dd><code>{model.technical.run}</code></dd></> : null}
        {model.technical.exit ? <><dt>{R.technical.exit}</dt><dd><code>{model.technical.exit}</code></dd></> : null}
        {model.technical.reason ? <><dt>{R.technical.reason}</dt><dd><code>{model.technical.reason}</code></dd></> : null}
        {model.consent && model.consent.technical.length
          ? <><dt>{R.technical.consent_heading}</dt><dd><ul className="ws-run-consent-receipts">{model.consent.technical.map((t) => <li key={t.id || t.label}>{t.label} <code>{t.id}</code> <code>{t.link}</code></li>)}</ul></dd></>
          : null}
        {model.consent && model.consent.file ? <><dt>{R.technical.consent_file}</dt><dd><code>{model.consent.file}</code></dd></> : null}
      </dl>
    </details>
  </>
}

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), summary, [tabindex]:not([tabindex="-1"])'

/**
 * The sheet itself: a dialog over the Change screen. Focus goes to its close
 * control, Escape closes it, Tab stays inside it.
 *
 * @param {object} props
 * @param {string} props.changeId
 * @param {{short?: string}|null} [props.version] the model's version (change-model.js)
 * @param {(run: string, actor: string) => Promise<{status: string, refusal?: string}>} [props.onCancel]
 * @param {() => void} props.onClose
 * @param {() => void} [props.onRan] called when a run has ended while the sheet was open
 * @param {Array<{n: number, text: string}>|null} [props.unlinked] the lines the served readiness names as having a passing run nobody linked
 * @param {(n: number) => void} [props.onDoor] go to one line's link control on the Change screen
 * @param {boolean} [props.browsePlaces] Settings opens this same sheet with its
 *   served place picker visible before a run; opening still writes nothing.
 */
export function RunSheet({ changeId, version, unlinked, onDoor, onCancel, onClose, onRan, browsePlaces = false }) {
  const [view, setView] = React.useState(null)
  const [busy, setBusy] = React.useState(false)
  const [refusal, setRefusal] = React.useState(null)
  /* D-049: which refusal the last press met, so a program refused for living
   * inside the project is followed by the places outside it to pick. */
  const [refused, setRefused] = React.useState(null)
  const [showPrompt, setShowPrompt] = React.useState(false)
  const [showElsewhere, setShowElsewhere] = React.useState(browsePlaces)
  const [copied, setCopied] = React.useState(false)
  const [pending, setPending] = React.useState(() => readPending(changeId))
  const [env, setEnv] = React.useState(null)
  const close = React.useRef(null)
  const box = React.useRef(null)
  const wasRunning = React.useRef(false)

  const load = React.useCallback(async () => {
    const read = await readJson(RUN_SHEET(changeId))
    if (read.ok && read.doc) setView(read.doc)
    else setRefusal(refusalText(read.doc && read.doc.error ? read.doc.error : 'unreachable'))
  }, [changeId])

  React.useEffect(() => { load() }, [load])
  React.useEffect(() => {
    const before = typeof document === 'undefined' ? null : document.activeElement
    if (close.current && typeof close.current.focus === 'function') close.current.focus()
    return () => {
      if (before && typeof document !== 'undefined' && before !== document.body && before.isConnected && typeof before.focus === 'function') before.focus()
    }
  }, [])

  const model = runSheetModel(view, { version, pending, env, refused, unlinked, browsePlaces })

  /* Honest motion: while a run is going, read the record again; when it has
   * ended, say so once to the screen underneath. */
  React.useEffect(() => {
    if (!model || model.status !== 'ready') return undefined
    if (model.running) {
      wasRunning.current = true
      const t = setTimeout(load, 700)
      return () => clearTimeout(t)
    }
    if (wasRunning.current) {
      wasRunning.current = false
      if (onRan) onRan()
    }
    return undefined
  })

  const start = async (environment) => {
    const m = latest.current
    if (!m.check) return
    if (m.permission.needed) {
      const granted = await postWithToken(RUN_PERMISSION(changeId), {})
      if (!granted.ok) { setRefusal(refusalText(granted.doc && granted.doc.error)); return }
    }
    const body = { check: m.check.id }
    const where = environment || m.environment
    if (where && where !== 'hq') body.environment = where
    const started = await postWithToken(RUN_SHEET(changeId), body)
    if (!started.ok && !(started.doc && started.doc.error === 'already_running')) {
      setRefusal(refusalText(started.doc && started.doc.error, started.doc))
      setRefused(started.doc && started.doc.error ? started.doc.error : null)
    }
    if (environment) setEnv(environment)
    // Only a started or already-running job can later announce that it ended.
    // A refused start has no run for onRan to report.
    if (started.ok || (started.doc && started.doc.error === 'already_running')) wasRunning.current = true
  }

  /* The handler reads the CURRENT model, not the one from its first render. */
  const latest = React.useRef(model)
  latest.current = model
  const handle = React.useRef(null)
  handle.current = async (what, environment) => {
    const m = latest.current
    if (what === 'cancel') {
      if (onCancel && m.runKey) {
        const answer = await onCancel(m.runKey, m.actor)
        if (answer && answer.status !== 'ready') setRefusal(answer.refusal || refusalText('other'))
      }
    } else if (what === 'take-back') {
      const taken = await postWithToken(RUN_PERMISSION_REVOKE(changeId), {})
      if (!taken.ok) setRefusal(refusalText(taken.doc && taken.doc.error))
    } else if (what === 'ask') {
      setShowPrompt(true)
    } else if (what === 'run' || what === 'allow-run' || what === 'pick') {
      if (what !== 'pick') { writePending(changeId, null); setPending(null) }
      await start(environment)
    }
  }
  const press = React.useRef(null)
  if (!press.current) {
    press.current = oneAtATime(async (what, environment) => {
      setBusy(true)
      setRefusal(null)
      setRefused(null)
      try {
        await handle.current(what, environment)
      } finally {
        await load()
        setBusy(false)
      }
    })
  }
  const on = {
    primary: (what) => press.current(what),
    elsewhere: () => setShowElsewhere(true),
    door: (n) => { onClose(); if (onDoor) onDoor(n) },
    takeBack: () => press.current('take-back'),
    pick: (id) => press.current('pick', id),
    copy: async (text) => {
      try {
        if (typeof navigator !== 'undefined' && navigator.clipboard) await navigator.clipboard.writeText(text)
        setCopied(true)
      } catch (_e) { setCopied(false) }
    },
    pasted: () => {
      const at = new Date().toISOString()
      writePending(changeId, at)
      setPending(at)
      setShowPrompt(false)
    },
  }

  const trap = (e) => {
    if (e.key === 'Escape') { e.preventDefault(); onClose(); return }
    if (e.key !== 'Tab' || !box.current) return
    const all = [...box.current.querySelectorAll(FOCUSABLE)]
    if (!all.length) return
    const first = all[0]
    const last = all[all.length - 1]
    const at = typeof document === 'undefined' ? null : document.activeElement
    const inside = at && box.current.contains(at)
    if (e.shiftKey && (at === first || !inside)) { e.preventDefault(); last.focus() }
    else if (!e.shiftKey && (at === last || !inside)) { e.preventDefault(); first.focus() }
  }

  return <div className="ws-shell-sheet-scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose() }}>
    <section className="ws-shell-dialog ws-run-sheet" role="dialog" aria-modal="true" aria-labelledby="ws-run-title" ref={box} onKeyDown={trap}>
      <h2 id="ws-run-title">{R.heading}</h2>
      <RunSheetBody model={model} busy={busy} refusal={refusal} showPrompt={showPrompt} showElsewhere={showElsewhere} copied={copied} on={on} />
      <button ref={close} type="button" className="ws-change-secondary ws-run-close" onClick={onClose}>{R.close}</button>
    </section>
  </div>
}

/* ONE SHEET, HELD ABOVE THE DOORS. A run changes the record, the screen
 * underneath re-reads it, and the control that opened the sheet can stop
 * being rendered — the Inspector's one primary becomes Approve the moment a
 * run makes the Change ready. A sheet owned by that control would vanish
 * mid-run, taking Cancel and the outcome with it. So the Change screen holds
 * the one open sheet in `RunSheetHost`, and every door asks the host to open
 * it. The host keeps the latest props any door rendered, so the version the
 * sheet names is the screen's current one. Without a host (a door rendered on
 * its own, as in a static test), a door holds its own sheet, as before. */
const RunSheetContext = React.createContext(null)

/** Holds the one open run sheet for the screen inside it. */
export function RunSheetHost({ children }) {
  const [openFor, setOpenFor] = React.useState(null)
  /* RUN-LINK-TRUTH (PW-F11, words): the props a door hands in were kept in a
   * ref, so a sheet left open across a run went on naming the version it was
   * opened on after the Change re-read the record and moved on. What the
   * sheet SAYS — the version, and which lines have a pass nobody linked — now
   * re-renders it when it changes; the callbacks stay in the ref. */
  const [, setSaid] = React.useState('')
  const props = React.useRef({})
  const api = React.useMemo(() => ({
    open: (p) => { props.current = { ...props.current, ...p }; setSaid(sayingOf(props.current)); setOpenFor(p.changeId) },
    update: (p) => { props.current = { ...props.current, ...p }; setSaid(sayingOf(props.current)) },
    isOpen: () => openFor,
  }), [openFor])
  const p = props.current
  return <RunSheetContext.Provider value={api}>
    {children}
    {openFor
      ? <RunSheet changeId={openFor} version={p.version} unlinked={p.unlinked} onDoor={p.onDoor} onCancel={p.onCancel} onRan={p.onRan} onClose={() => setOpenFor(null)} />
      : null}
  </RunSheetContext.Provider>
}

/**
 * The door: one button that opens the sheet. Secondary by default, because
 * the screen underneath already has its one primary.
 */
export function RunDoor({ changeId, version, unlinked, onDoor, label, primary, onCancel, onRan }) {
  const host = React.useContext(RunSheetContext)
  const [open, setOpen] = React.useState(false)
  React.useEffect(() => {
    if (host && changeId && host.isOpen() === changeId) host.update({ version, unlinked, onDoor, onCancel, onRan })
  })
  if (!changeId) return null
  const press = () => {
    if (host) host.open({ changeId, version, unlinked, onDoor, onCancel, onRan })
    else setOpen(true)
  }
  return <>
    <button type="button" className={primary ? 'ws-change-primary' : 'ws-change-secondary'} onClick={press} aria-haspopup="dialog">{label || R.door}</button>
    {!host && open ? <RunSheet changeId={changeId} version={version} unlinked={unlinked} onDoor={onDoor} onCancel={onCancel} onRan={onRan} onClose={() => setOpen(false)} /> : null}
  </>
}

/** What the open sheet says that a door can change: the version it names and
 * the lines with a pass nobody linked. A string, so an unchanged saying is an
 * unchanged state and re-renders nothing. */
function sayingOf(p) {
  const v = p && p.version && p.version.short ? String(p.version.short) : ''
  const lines = (Array.isArray(p && p.unlinked) ? p.unlinked : []).map((u) => (u && typeof u.n === 'number' ? u.n : '')).join(',')
  return v + '|' + lines
}

export default RunSheet
