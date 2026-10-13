/* The frame a routed screen keeps when it has nothing of its own to draw: it
 * is still reading, or a read failed (C30 R-NAV, the Team sheet).
 *
 * THE RULE: a read error, or a read still out, is said INSIDE the app frame —
 * the top bar and the sidebar stay, and only the main area carries the line.
 * It is never a blank page and never a full page of its own: a person who
 * lands on a failed screen can still go to the Inbox, Changes, Projects,
 * Record over time, Team or Settings, or open Find, ask, act. Every screen's
 * "Reading…" and "could not read…" goes through `FrameState`; none draws its
 * own `100vh` ground (frame-state.test.js reads the screens' sources and
 * mounts them on failing reads to keep it that way).
 *
 * The frame is the canvas's own, minus what needs the data that is missing:
 * the logo (Inbox, home), where you are, Find, ask, act; the sections
 * with this screen current; and the status bar's privacy sentence. It reads
 * the person's place from `FrameContext`, which app.jsx provides.
 *
 * `ScreenBoundary` is the same rule for a screen that throws while drawing
 * (data it did not expect, a chunk that did not load): React would otherwise
 * unmount the whole app to a blank page.
 */
import React, { createContext, useContext } from 'react'
import { href, resolveLink } from './routes.js'
import { PRIVACY } from './data/project.js'
import { KEY_TABLE } from './keys/table.js'

/** The person's place and the palette's opener, given by app.jsx to every screen. */
export const FrameContext = createContext({ at: {}, openCommand: null })

/** Where the person is, by screen — the line after the logo. */
export const HERE = Object.freeze({
  inbox: 'Inbox',
  changes: 'Changes',
  projects: 'Projects',
  record: 'Record over time',
  recordStatements: 'Statements across Changes',
  team: 'Team',
  settings: 'Settings',
  start: 'First run',
  brief: 'Brief',
  statements: 'Every statement',
  changed: 'What changed',
  done: 'What counts as done',
  check: 'Check it again',
  approve: 'Approve',
  story: 'The whole story',
  pr: 'On the pull request',
  step: 'One step',
})

/** The sections of the sidebar, in the canvas's order. The key sequence each shows is the key table's (`hintOf`). */
export const SECTIONS = Object.freeze([
  { screen: 'inbox', label: 'Inbox', icon: ['M2 9.5 L4 3 H12 L14 9.5 V13 H2 Z', 'M2 9.5 H5.5 L6.5 11 H9.5 L10.5 9.5 H14'] },
  { screen: 'changes', label: 'Changes', icon: ['M8 2 L14 5 L8 8 L2 5 Z', 'M2 8 L8 11 L14 8', 'M2 11 L8 14 L14 11'] },
  { screen: 'projects', label: 'Projects', icon: ['M2 4 V12.5 H14 V5.8 H8 L6.5 4 Z'] },
  { screen: 'record', label: 'Record over time', icon: ['M2 13.5 H14', 'M3.5 10.5 L6.5 7.2 L9 9.2 L13 4.5'] },
  { screen: 'team', label: 'Team', icon: ['M1.8 13.5 C2.3 10.8 4 9.6 6 9.6 C8 9.6 9.7 10.8 10.2 13.5', 'M11.2 9.6 C12.9 9.6 14 10.8 14.3 13'], circles: [[6, 5.5, 2.3], [11.3, 6.2, 1.8]] },
  { screen: 'settings', label: 'Settings', icon: ['M2.5 4 H13.5', 'M2.5 8 H13.5', 'M2.5 12 H13.5'], dots: [[6, 4], [10.5, 8], [5, 12]] },
])

/** The key sequence a section shows beside its name: the key table's (keys/table.js), never written here. The Inbox shows none, as the canvas draws it. */
export function hintOf(screen) {
  if (screen === 'inbox') return null
  const entry = KEY_TABLE.find((e) => e.global && e.go === screen)
  return entry ? entry.keys[0] : null
}

const FONT = "'Geist', -apple-system, 'Helvetica Neue', sans-serif"
const MONO = "'Geist Mono', monospace"

function Icon({ section }) {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      {(section.circles || []).map(([cx, cy, r]) => <circle key={cx + ':' + cy} cx={cx} cy={cy} r={r} />)}
      {section.icon.map((d) => <path key={d} d={d} />)}
      {(section.dots || []).map(([cx, cy]) => <circle key={cx + ':' + cy} cx={cx} cy={cy} r="1.6" fill="#0A0A0B" />)}
    </svg>
  )
}

/** The sidebar's section list: every section a link, this screen's current. */
function Sections({ at }) {
  const project = (at && at.project) || undefined
  return (
    <nav aria-label="Sections" style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
      {SECTIONS.map((s) => {
        // the list a Record count opens is part of Record over time
        const here = at && (at.screen === s.screen || (s.screen === 'record' && at.screen === 'recordStatements'))
        const base = { position: 'relative', height: '32px', display: 'flex', alignItems: 'center', gap: '10px', padding: '0 8px 0 10px', borderRadius: '6px', fontSize: '13.5px' }
        return (
          <a
            key={s.screen}
            href={href(s.screen, { project })}
            aria-current={here ? 'page' : undefined}
            className={here ? undefined : 'tl-ghost'}
            style={here ? { ...base, background: '#16161A', color: '#EDEDEF', fontWeight: '500' } : { ...base, color: '#A1A1AA' }}
          >
            {here ? <span aria-hidden="true" style={{ position: 'absolute', left: '0', top: '7px', bottom: '7px', width: '2px', borderRadius: '1px', background: '#FF7A29', boxShadow: '0 0 6px rgba(255,154,82,.55)' }}></span> : null}
            <Icon section={s} />
            <span style={{ flex: '1 1 auto' }}>{s.label}</span>
            {hintOf(s.screen) ? <span style={{ font: '12px ' + MONO, color: '#82828C' }}>{hintOf(s.screen)}</span> : null}
          </a>
        )
      })}
    </nav>
  )
}

/**
 * The app frame with one message in its main area. `role` is "status" while
 * reading and "alert" when a read failed; `children` is the line (text, or a
 * line with a link). With `page`, the main area is a scrolling page and
 * `children` is the page itself (a list too long for a centred line).
 */
export function FrameState({ role = 'status', page = false, children }) {
  const { at, openCommand } = useContext(FrameContext)
  const where = at || {}
  const label = HERE[where.screen] || 'ThruLine'
  const inChange = Boolean(where.change)
  const find = resolveLink('Command-Ask.dc.html', where) || href('inbox', { project: where.project, find: true })
  const message = { maxWidth: '520px', lineHeight: role === 'alert' ? '22px' : 1.5, textAlign: 'center', color: role === 'alert' ? '#EDEDEF' : '#A1A1AA' }
  return (
    <div data-frame="kept" style={{ width: '100%', height: '100vh', boxSizing: 'border-box', background: '#000000', color: '#EDEDEF', fontFamily: FONT, fontSize: '14px', display: 'flex', flexDirection: 'column', overflow: 'hidden', position: 'relative' }}>
      <header style={{ height: '44px', flex: '0 0 44px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '12px', padding: '0 12px', background: '#0A0A0B', borderBottom: '1px solid #1F1F23' }}>
        <a href={href('inbox', { project: where.project })} aria-label="Inbox, home" style={{ display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: '28px', height: '28px', borderRadius: '5px' }}>
          <span style={{ width: '20px', height: '20px', borderRadius: '4px', background: '#FF7A29', color: '#000000', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', font: '600 12px ' + MONO }}>t</span>
        </a>
        <nav aria-label="Where you are" style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '13px', whiteSpace: 'nowrap' }}>
          {inChange ? (
            <>
              <a href={href('changes', { project: where.project })} style={{ color: '#A1A1AA' }}>Changes</a>
              <span aria-hidden="true" style={{ color: '#82828C' }}>›</span>
            </>
          ) : null}
          <span style={{ color: '#EDEDEF', fontWeight: '500' }}>{label}</span>
        </nav>
        <span style={{ flex: '1 1 0' }}></span>
        <a
          href={find}
          onClick={openCommand ? (e) => { e.preventDefault(); openCommand() } : undefined}
          className="tl-ghost"
          aria-label="Find, ask, act. Opens the palette, shortcut Command K"
          style={{ flex: '0 1 400px', minWidth: '0', height: '30px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '10px', padding: '0 8px 0 10px', background: '#000000', border: '1px solid #66666F', borderRadius: '6px', color: '#82828C', fontSize: '13px' }}
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true" style={{ flex: '0 0 auto' }}><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5 L14 14" /></svg>
          <span style={{ flex: '1 1 auto', minWidth: '0', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>Find a Change, a person, a rule or a commit</span>
          <kbd style={{ font: '12px ' + MONO, color: '#A1A1AA', border: '1px solid #66666F', borderRadius: '3px', padding: '0 4px', lineHeight: '16px' }}>⌘K</kbd>
        </a>
        <span style={{ flex: '1 1 0' }}></span>
      </header>
      <div style={{ flex: '1 1 auto', minHeight: '0', display: 'flex' }}>
        <aside aria-label="HQ" style={{ width: '220px', flex: '0 0 220px', boxSizing: 'border-box', display: 'flex', flexDirection: 'column', gap: '24px', padding: '12px 10px 14px', background: '#0A0A0B', borderRight: '1px solid #1F1F23', overflow: 'auto' }}>
          <Sections at={where} />
        </aside>
        <main aria-label={label} style={page
          ? { flex: '1 1 auto', minWidth: '0', minHeight: '0', overflow: 'auto', padding: '28px 24px 64px', boxSizing: 'border-box', background: '#000000' }
          : { flex: '1 1 auto', minWidth: '0', minHeight: '0', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '24px', boxSizing: 'border-box', background: '#000000' }}>
          {page ? children : <div role={role} style={message}>{children}</div>}
        </main>
      </div>
      <footer style={{ height: '26px', flex: '0 0 26px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '12px', padding: '0 14px', borderTop: '1px solid #1F1F23', background: '#0A0A0B', fontSize: '12px', color: '#82828C', whiteSpace: 'nowrap' }}>
        <span style={{ flex: '1' }}></span>
        <span>{PRIVACY}</span>
      </footer>
    </div>
  )
}

/** A read still out: the frame, and one quiet line. */
export function Reading() {
  return <FrameState role="status">Reading the record…</FrameState>
}

/** The words when the server did not answer at all. */
export const DID_NOT_ANSWER = 'ThruLine on this machine did not answer, so this page can’t show anything yet. Start it with thruline ui, then reload this page.'

/** A read that failed: the frame, and what happened. `what` is the sentence; a bare failure says the server did not answer. */
export function Failed({ what }) {
  return <FrameState role="alert">{what || DID_NOT_ANSWER}</FrameState>
}

/** The words for a screen that threw while drawing. */
export const COULD_NOT_DRAW = 'ThruLine could not draw this page. Reload it; if it keeps happening, run thruline verify.'

/**
 * A screen that throws while drawing is said in the frame, not left as a
 * blank page. Moving to another address clears it (`resetKey`). The error is
 * also written to the console: this hides nothing.
 */
export class ScreenBoundary extends React.Component {
  state = { failed: false }

  static getDerivedStateFromError() {
    return { failed: true }
  }

  componentDidCatch(error) {
    // eslint-disable-next-line no-console
    console.error('ThruLine: a screen could not be drawn.', error)
  }

  componentDidUpdate(prev) {
    if (this.state.failed && prev.resetKey !== this.props.resetKey) this.setState({ failed: false })
  }

  render() {
    return this.state.failed ? <FrameState role="alert">{COULD_NOT_DRAW}</FrameState> : this.props.children
  }
}
