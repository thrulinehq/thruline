import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { ALTITUDES, KEYMAP, installKeys, pressPrimary, setSingleKeys, singleKeysOn } from './altitude.js'

/* The shell chrome shared by the Change screen and the workspace (P6-1;
 * design record §3.1, §6).
 *
 * WHAT IT DRAWS. The top bar — where you are (project › work › version), how
 * much to show (Brief · Explore · Workbench), how it is worded (Plain · Technical),
 * Ask, and when this screen was read — and the labelled sidebar (Work ·
 * Outline · Saved views) that stands in for the fifteen-icon rail at Brief and
 * Explore. Plus the two sheets the keyboard opens: the key list (`?`) and Ask
 * itself (`/`, and — since P6-4 — ⌘K).
 *
 * ASK, SINCE P6-4. A caller that hands this bar an `askPanel` gets a LIVE Ask:
 * the top-bar control is a button, the key opens the panel inside this file's
 * one `Sheet`, and the focus trap, Escape, the scrim and the return of focus
 * are the ones already here rather than a second copy in the panel. A caller
 * that hands none — a screen with no Change open — keeps the off slot, whose
 * reason is now the true one: Ask answers questions about one Change.
 *
 * WHAT IT MAY SAY. Every word is `HQ_COPY.shell`. It names NO state, NO
 * version and NO count of its own: the breadcrumb's version and any number
 * arrive as props, already derived by `change-model.js` (the Change screen) or
 * by the workspace's own reading, and are only filled into a template here.
 * `shell-chrome.test.js` holds this file to exactly these imports, and to no
 * literal state word.
 *
 * WHAT IT OWNS. The one key handler (`altitude.js` `installKeys`), installed
 * once per window, and the short note the shell says when a key is declared
 * and not yet acting, or when `.` finds the main action off.
 */

const S = HQ_COPY.shell

/** A control that is off, with both halves of why, as its own description. */
export function Slot({ id, label, pair, short, onExplain }) {
  const why = pair.reason + ' ' + pair.enabler
  return <span className="ws-shell-slot">
    <button
      type="button"
      className="ws-shell-slot-btn"
      aria-disabled="true"
      aria-describedby={id}
      title={why}
      onClick={() => onExplain && onExplain(why)}
    >{label}</button>
    {short ? <span className="ws-shell-slot-tag" aria-hidden="true">{short}</span> : null}
    <span id={id} className={short ? 'ws-shell-sr' : 'ws-shell-why'}>{why}</span>
  </span>
}

/**
 * The top bar, and the one key handler.
 *
 * @param {object} props
 * @param {Array<{key: string, text: string, onClick?: Function, href?: string, work?: boolean}>} props.crumbs
 * @param {'brief'|'explore'|'workbench'} props.altitude
 * @param {(altitude: string) => void} props.onAltitude a CHOICE: the caller remembers it
 * @param {'manager'|'operator'} props.register
 * @param {(register: string) => void} props.onRegister
 * @param {string|null} [props.readWhen] a real read time, or null for the honest unavailable form
 * @param {string|null} [props.readTitle] T10 — the same read's date, time and UTC offset, shown on hover
 * @param {() => void} [props.onRefresh]
 * @param {boolean} [props.refreshing]
 * @param {((dir: 1|-1) => string|null)|null} [props.onWalk] J / K, on a screen
 *   that draws the Trace; returns the sentence to say. Absent: the screen has
 *   no picture, and the key says so.
 * @param {(() => string|null)|null} [props.onThread] F, the same way
 * @param {() => void} [props.onEscape] Esc with no sheet open (the Trace stops
 *   lighting its thread)
 * @param {((props: {onClose: () => void}) => React.ReactNode)|null} [props.askPanel]
 *   what Ask shows on this screen (P6-4). Absent: Ask is the off slot saying
 *   why, and `/` opens the explanation of what it is.
 */
export function ShellTopBar({ crumbs, altitude, onAltitude, register, onRegister, readWhen, readTitle, onRefresh, refreshing, onWalk, onThread, onEscape, askPanel }) {
  const [sheet, setSheet] = React.useState(null)
  const [note, setNote] = React.useState(null)
  /* The handler is installed ONCE and reads the latest props through a ref,
   * so a re-render never installs a second handler on the same keys. */
  const live = React.useRef({})
  live.current = { altitude, onAltitude, register, onRegister, onWalk, onThread, onEscape, sheet }

  React.useEffect(() => {
    if (typeof window === 'undefined') return undefined
    return installKeys(window, (action, e) => {
      const p = live.current
      /* T-6-4: an open sheet is modal. Only what closes it or swaps it for
       * the other sheet acts; nothing behind it moves or takes focus. */
      if (p.sheet && action !== 'close' && action !== 'help' && action !== 'ask') return
      if (action === 'brief' || action === 'explore' || action === 'workbench') { setNote(null); p.onAltitude && p.onAltitude(action); return }
      if (action === 'register') { p.onRegister && p.onRegister(p.register === 'operator' ? 'manager' : 'operator'); return }
      if (action === 'ask') { setSheet('ask'); return }
      if (action === 'help') { setSheet('keys'); return }
      if (action === 'close') {
        const open = !!p.sheet
        setSheet(null); setNote(null)
        if (!open && p.onEscape) p.onEscape()
        return
      }
      if (action === 'walk') { setNote(p.onWalk ? p.onWalk(e && String(e.key).toLowerCase() === 'k' ? -1 : 1) || null : S.notes.walk_here_none); return }
      if (action === 'thread') { setNote(p.onThread ? p.onThread() || null : S.notes.thread_here_none); return }
      if (action === 'primary') {
        const r = pressPrimary(typeof document === 'undefined' ? null : document)
        if (r.pressed) setNote(null)
        /* A3-F8a: the decision's own sentence is said as it is — Approve is
         * not "off", it is pressed by hand. */
        else if (r.reason && r.reason === S.notes.approve_by_hand) setNote(r.reason)
        else setNote(r.reason ? fill(S.notes.primary_off, { reason: r.reason }) : S.notes.no_primary)
      }
    })
  }, [])

  /* T-6-4: a move between altitudes can take away the element that held
   * focus (Brief draws no canvas). When it does, focus lands on the altitude
   * now pressed, never on the page itself. A screen that draws the selected
   * card at the new altitude has already put focus there by the time this
   * runs, and is left alone. */
  const altitudes = React.useRef(null)
  const lastAltitude = React.useRef(altitude)
  React.useEffect(() => {
    if (lastAltitude.current === altitude) return
    lastAltitude.current = altitude
    if (typeof document === 'undefined') return
    const at = document.activeElement
    if (at && at !== document.body && at.isConnected) return
    const pressed = altitudes.current && altitudes.current.querySelector('button[aria-pressed="true"]')
    if (pressed && typeof pressed.focus === 'function') pressed.focus()
  }, [altitude])

  const R = S.registers
  return <header className="ws-shell-top" aria-label={S.topbar.label}>
    <nav className="ws-shell-crumbs" aria-label={S.topbar.crumbs_label}>
      <ol>
        {(crumbs || []).map((c, i) => <li key={c.key} className={'ws-shell-crumb ws-shell-crumb-' + c.key + (c.work ? ' ws-shell-crumb-work' : '')}>
          {i ? <span className="ws-shell-crumb-sep" aria-hidden="true">›</span> : null}
          {c.onClick
            ? <button type="button" className="ws-shell-crumb-btn" onClick={c.onClick}>{c.text}</button>
            : c.href
              ? <a className="ws-shell-crumb-btn" href={c.href}>{c.text}</a>
              : <span aria-current={c.work ? 'page' : undefined}>{c.text}</span>}
        </li>)}
      </ol>
    </nav>

    <div className="ws-shell-seg" role="group" aria-label={S.topbar.altitude_label} ref={altitudes}>
      {ALTITUDES.map((a) => <button
        key={a}
        type="button"
        className="ws-shell-seg-btn"
        aria-pressed={altitude === a}
        title={S.altitudes[a].hint}
        onClick={() => onAltitude && onAltitude(a)}
      >{S.altitudes[a].label}</button>)}
    </div>

    <div className="ws-shell-seg" role="group" aria-label={S.topbar.register_label} title={R.hint}>
      {['manager', 'operator'].map((r) => <button
        key={r}
        type="button"
        className="ws-shell-seg-btn"
        aria-pressed={register === r}
        onClick={() => onRegister && onRegister(r)}
      >{R[r]}</button>)}
    </div>

    {/* P6-4: a door, when this screen has a Change for Ask to look in; the
      * off slot with both halves of why when it has not. */}
    {askPanel
      ? <button type="button" className="ws-shell-ask-btn" title={HQ_COPY.ask.hint} onClick={() => setSheet('ask')}>{HQ_COPY.ask.label}</button>
      : <Slot id="ws-shell-ask-why" label={S.ask.label} pair={S.ask.why} short={S.ask.short} onExplain={() => setSheet('ask')} />}

    <p className="ws-shell-read">
      {readWhen
        ? <span title={readTitle || undefined}>{fill(S.topbar.read_at, { when: readWhen })}</span>
        : <span title={S.topbar.read_unknown_why}>{S.topbar.read_unknown}</span>}
      {onRefresh
        ? <button type="button" className="ws-shell-refresh" disabled={!!refreshing} aria-describedby={refreshing ? 'ws-shell-refresh-why' : undefined} onClick={onRefresh}>{refreshing ? S.topbar.refreshing : S.topbar.refresh}</button>
        : null}
      {/* T-6-6 (rule 6): off only while a read is in flight, and it says so. */}
      {onRefresh && refreshing ? <span id="ws-shell-refresh-why" className="ws-shell-sr">{S.topbar.refreshing_why.reason} {S.topbar.refreshing_why.enabler}</span> : null}
      <button type="button" className="ws-shell-refresh" onClick={() => setSheet('keys')}>{S.topbar.keys}</button>
    </p>

    <p className="ws-shell-note" role="status" aria-live="polite">{note || ''}</p>

    {sheet === 'keys' ? <KeySheet onClose={() => setSheet(null)} /> : null}
    {sheet === 'ask'
      ? askPanel
        ? <Sheet id="ws-shell-ask-title" title={HQ_COPY.ask.title} onClose={() => setSheet(null)}>{askPanel({ onClose: () => setSheet(null) })}</Sheet>
        : <AskSheet onClose={() => setSheet(null)} />
      : null}
  </header>
}

/** What Tab can reach inside a box, in order. */
const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])'

/**
 * A small dialog: focus goes to its close control, Escape closes it, Tab and
 * Shift+Tab stay inside it while it is open (T-6-4), and on close focus goes
 * back to the element that had it before it opened.
 */
function Sheet({ id, title, onClose, children }) {
  const close = React.useRef(null)
  const box = React.useRef(null)
  React.useEffect(() => {
    const before = typeof document === 'undefined' ? null : document.activeElement
    if (close.current && typeof close.current.focus === 'function') close.current.focus()
    return () => {
      if (before && typeof document !== 'undefined' && before !== document.body && before.isConnected && typeof before.focus === 'function') before.focus()
    }
  }, [])
  const trap = (e) => {
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
    <section className="ws-shell-dialog" role="dialog" aria-modal="true" aria-labelledby={id} ref={box} onKeyDown={trap}>
      <h2 id={id}>{title}</h2>
      {children}
      <button ref={close} type="button" className="ws-shell-refresh" onClick={onClose}>{S.keys.close_button}</button>
    </section>
  </div>
}

/** `?` — the §6 map, every row, the unwired ones saying when they arrive. */
export function KeySheet({ onClose }) {
  const K = S.keys
  const shown = (k) => (k === 'Escape' ? 'Esc' : k.length === 1 ? k.toUpperCase() : k)
  const win = typeof window === 'undefined' ? null : window
  /* N13-KEYS-214 (WCAG 2.2 SC 2.1.4): one switch for every one-character
   * key. Its state is a word, never a colour alone. */
  const [single, setSingle] = React.useState(() => singleKeysOn(win))
  const flip = () => { const next = !single; setSingleKeys(win, next); setSingle(next) }
  return <Sheet id="ws-shell-keys-title" title={K.title} onClose={onClose}>
    <p className="ws-shell-muted">{K.intro}</p>
    <p className="ws-shell-single">
      <span id="ws-shell-single-label">{K.single_keys.label}</span>
      {' '}
      <button type="button" role="switch" className="ws-shell-refresh" aria-checked={single} aria-labelledby="ws-shell-single-label" aria-describedby="ws-shell-single-hint" onClick={flip}>{single ? K.single_keys.on : K.single_keys.off}</button>
      <span id="ws-shell-single-hint" className="ws-shell-muted ws-shell-single-hint">{K.single_keys.hint}</span>
    </p>
    <dl className="ws-shell-keys">
      {/* Keyed by its LINE, not by its action: two rows can open the same
        * thing (`/` and ⌘K both open Ask) and a key that repeats makes React
        * drop one of them. */}
      {KEYMAP.map((row) => <div key={row.copy} data-wired={row.wired ? 'yes' : 'no'}>
        <dt>{row.keys.map((k) => <kbd key={k}>{shown(k)}</kbd>)}</dt>
        <dd>{K[row.copy]}{row.wired ? null : <span className="ws-shell-muted"> {K.later}</span>}</dd>
      </div>)}
    </dl>
    <h3>{K.workspace_heading}</h3>
    <p className="ws-shell-muted">{K.workspace_keys}</p>
    <p className="ws-shell-muted">{K.moved}</p>
  </Sheet>
}

/** `/` on a screen with no Change open: what Ask is, and where it is on. */
export function AskSheet({ onClose }) {
  const A = S.ask
  return <Sheet id="ws-shell-ask-title" title={A.title} onClose={onClose}>
    <p>{A.body}</p>
    <p className="ws-shell-muted">{A.promise}</p>
    <p className="ws-shell-muted">{A.enabler}</p>
  </Sheet>
}

/**
 * The labelled sidebar (Work · Outline · Saved views).
 *
 * @param {object} props
 * @param {string} [props.workName] the name of what is open, already a record value
 * @param {() => void} props.onWork goes to the list of work
 * @param {React.ReactNode} [props.saved] what Saved views holds, or null for the off slot
 * @param {{reason: string, enabler: string}|null} [props.savedOff] why Saved views is off here
 * @param {() => void} [props.onSaved] when Saved views is a door rather than a list
 * @param {(() => void)|null} [props.onOutline] opens the Outline twin; absent,
 *   Outline is an off slot saying why (a screen with no Trace)
 * @param {boolean} [props.outlineOn] whether the Outline is what is showing
 * @param {{text: string, href: string}|null} [props.licence] the quiet licence
 *   line (T-DIST-4), already worded by `licence-view.js`; a door to the
 *   licence page, never a banner
 */
export function ShellSidebar({ workName, onWork, saved, savedOff, onSaved, onOutline, outlineOn, licence }) {
  const B = S.sidebar
  return <nav className="ws-shell-side" aria-label={B.label}>
    <ul>
      <li>
        <button type="button" className="ws-shell-side-item" onClick={onWork}>{B.work}</button>
        <span className="ws-shell-muted ws-shell-side-hint">{B.work_hint}</span>
        {workName ? <span className="ws-shell-side-current" aria-current="page">{workName}</span> : null}
      </li>
      <li>
        {onOutline
          ? <>
            <button type="button" className="ws-shell-side-item" aria-pressed={!!outlineOn} onClick={onOutline}>{B.outline}</button>
            <span className="ws-shell-muted ws-shell-side-hint">{B.outline_hint}</span>
          </>
          : <Slot id="ws-shell-outline-why" label={B.outline} pair={B.outline_why} />}
      </li>
      <li>
        {savedOff
          ? <Slot id="ws-shell-saved-why" label={B.saved} pair={savedOff} />
          : saved
            ? <details className="ws-shell-saved" open>
              <summary className="ws-shell-side-item">{B.saved}</summary>
              <span className="ws-shell-muted ws-shell-side-hint">{B.saved_hint}</span>
              {saved}
            </details>
            : <>
              <button type="button" className="ws-shell-side-item" onClick={onSaved}>{B.saved}</button>
              <span className="ws-shell-muted ws-shell-side-hint">{B.saved_open_explore}</span>
            </>}
      </li>
    </ul>
    {licence ? <p className="ws-shell-muted ws-shell-licence"><a href={licence.href}>{licence.text}</a></p> : null}
  </nav>
}

export default ShellTopBar
