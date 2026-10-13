/* The `?` map (C30 R-KEYS): every key on the screen in front, then every key
 * that works everywhere, from the key table (rows.js `mapRows`).
 *
 * The canvas has no artboard for it, so it is built only from markup the
 * canvas draws: Find, ask, act's overlay, scrim, panel, header label, group
 * heading, `esc` button and footer (Command-Ask/template.jsx), with Settings ›
 * Keyboard's own map rows inside (Settings/template.jsx, "Keyboard map").
 *
 * Esc closes it (the dispatcher), as do a click on the scrim, the esc button
 * and following "Keyboard settings". While it is open, Tab stays inside it;
 * on close, focus goes back where it was. */
import React, { useEffect, useRef } from 'react'
import { href } from '../routes.js'
import { MODE_WORDS, mapRows } from './rows.js'

const MONO = "'Geist Mono', monospace"

/** Each screen in words, for the map's first heading. */
export const SCREEN_WORDS = Object.freeze({
  Inbox: 'Inbox',
  Changes: 'Changes',
  Projects: 'Projects',
  'Record-Over-Time': 'Record over time',
  'Team-View': 'Team',
  Settings: 'Settings',
  Onboarding: 'First run',
  'Change-Brief': 'Brief',
  'Every-Statement': 'Every statement',
  'What-Changed': 'What changed',
  'Done-Criteria': 'What counts as done',
  'Repair-Loop': 'Check it again · Tell the agent',
  'Approve-Sheet': 'Approve',
  'Whole-Story': 'The whole story',
  'PR-Face': 'On the pull request',
})

const FOCUSABLE = 'button:not([disabled]), a[href]'

function Rows({ rows, label }) {
  return (
    <div role="table" aria-label={label} style={{ display: 'grid', gridTemplateColumns: 'repeat(2, minmax(0, 1fr))', gap: '0 24px', padding: '0 10px 8px' }}>
      {rows.map((k) => (
        <div key={k.id} role="row" style={{ display: 'flex', alignItems: 'center', gap: '12px', minHeight: '36px', borderBottom: '1px solid #1F1F23' }}>
          <span role="cell" style={{ width: '72px', flex: '0 0 72px' }}><kbd style={{ display: 'inline-flex', alignItems: 'center', height: '22px', boxSizing: 'border-box', padding: '0 6px', border: `1px solid ${k.kbdBd}`, borderRadius: '4px', font: `12px ${MONO}`, color: k.keyColor, whiteSpace: 'nowrap' }}>{k.keyText}</kbd></span>
          <span role="cell" style={{ flex: '1 1 auto', minWidth: '0', fontSize: '13px', color: k.labelColor, overflowWrap: 'anywhere', padding: '5px 0' }}>{k.label}</span>
          <span role="cell" style={{ fontSize: '12px', color: '#82828C' }}>{k.state}</span>
        </div>
      ))}
    </div>
  )
}

function Group({ id, title, sub, rows }) {
  if (!rows.length) return null
  return (
    <div role="group" aria-labelledby={id} style={{ display: 'flex', flexDirection: 'column' }}>
      <div style={{ height: '28px', flex: '0 0 28px', boxSizing: 'border-box', display: 'flex', alignItems: 'flex-end', gap: '10px', padding: '0 10px 7px' }}>
        <span id={id} style={{ fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>{title}</span>
        <span style={{ fontSize: '12px', color: '#82828C' }}>{sub}</span>
      </div>
      <Rows rows={rows} label={title} />
    </div>
  )
}

export function KeysMap({ screen, at, mode, onClose }) {
  const where = at || {}
  const rows = mapRows({ screen, change: where.change, mode })
  const dialog = useRef(null)
  useEffect(() => {
    const before = typeof document !== 'undefined' ? document.activeElement : null
    const first = dialog.current && dialog.current.querySelector(FOCUSABLE)
    if (first) first.focus()
    return () => {
      if (before && typeof before.focus === 'function' && before.isConnected) before.focus()
    }
  }, [])
  // Tab and Shift+Tab go round inside the map
  const trap = (e) => {
    if (e.key !== 'Tab' || !dialog.current) return
    const all = [...dialog.current.querySelectorAll(FOCUSABLE)]
    if (!all.length) return
    const i = all.indexOf(document.activeElement)
    const next = e.shiftKey ? (i <= 0 ? all.length - 1 : i - 1) : (i < 0 || i === all.length - 1 ? 0 : i + 1)
    e.preventDefault()
    all[next].focus()
  }
  const words = SCREEN_WORDS[screen] || ''
  return (
    <div className="tl-keys-overlay" style={{ position: 'fixed', left: '0', top: '0', right: '0', bottom: '0', zIndex: '1000', color: '#EDEDEF', fontFamily: "'Geist', -apple-system, 'Helvetica Neue', sans-serif", fontSize: '14px' }}>
      <div aria-hidden="true" onClick={onClose} style={{ position: 'absolute', left: '0', top: '0', right: '0', bottom: '0', background: 'rgba(0,0,0,.64)' }}></div>
      <div style={{ position: 'absolute', left: '50%', top: '44px', bottom: '26px', width: 'min(1512px, 100vw)', transform: 'translateX(-50%)', pointerEvents: 'none' }}>
        <section ref={dialog} role="dialog" aria-modal="true" aria-labelledby="tl-keys-label" onKeyDown={trap} style={{ position: 'absolute', left: 'calc(50% - 360px)', top: '32px', width: '720px', maxHeight: '794px', zIndex: '30', boxSizing: 'border-box', display: 'flex', flexDirection: 'column', background: '#111113', border: '1px solid #2A2A30', borderRadius: '10px', boxShadow: '0 24px 80px rgba(0,0,0,.72)', overflow: 'hidden', pointerEvents: 'auto' }}>
          <div style={{ flex: '0 0 auto', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '10px', padding: '12px 16px', borderBottom: '1px solid #1F1F23' }}>
            <h2 id="tl-keys-label" style={{ margin: '0', fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }}>Keys</h2>
            <span style={{ fontSize: '12px', color: '#82828C' }}>{words ? 'on ' + words + ', and everywhere' : 'everywhere'}</span>
            <span style={{ flex: '1' }}></span>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: '6px', height: '20px', boxSizing: 'border-box', padding: '0 8px', border: '1px solid #2A2A30', borderRadius: '5px', fontSize: '12px', color: '#A1A1AA', whiteSpace: 'nowrap' }}>Single keys <span style={{ color: '#EDEDEF' }}>{MODE_WORDS[mode] || MODE_WORDS.on}</span></span>
            <button type="button" className="tl-ghost" onClick={onClose} aria-label="Close the map of keys" aria-keyshortcuts="Escape" style={{ height: '26px', padding: '0 8px', display: 'inline-flex', alignItems: 'center', gap: '6px', background: 'transparent', border: '1px solid #66666F', borderRadius: '5px', color: '#A1A1AA', font: `11px ${MONO}`, cursor: 'pointer', whiteSpace: 'nowrap' }}>esc</button>
          </div>
          <div style={{ flex: '1 1 auto', minHeight: '0', overflow: 'auto', display: 'flex', flexDirection: 'column', padding: '6px 8px' }}>
            <Group id="tl-keys-here" title="On this screen" sub={words} rows={rows.here} />
            <Group id="tl-keys-all" title="Everywhere" sub="on every screen" rows={rows.everywhere} />
          </div>
          <div style={{ height: '34px', flex: '0 0 34px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '16px', padding: '0 14px', borderTop: '1px solid #1F1F23', background: '#0A0A0B', fontSize: '12px', color: '#82828C', whiteSpace: 'nowrap' }}>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: '6px' }}><kbd style={{ display: 'inline-flex', alignItems: 'center', justifyContent: 'center', height: '18px', boxSizing: 'border-box', padding: '0 4px', font: `11px ${MONO}`, color: '#A1A1AA', border: '1px solid #66666F', borderRadius: '3px' }}>esc</kbd>closes the map</span>
            <span style={{ flex: '1' }}></span>
            <a href={href('settings', { project: where.project, pane: 'keyboard' })} onClick={onClose} style={{ color: '#A1A1AA', textDecoration: 'underline', textDecorationColor: '#66666F', textUnderlineOffset: '3px' }}>Keyboard settings</a>
          </div>
        </section>
      </div>
    </div>
  )
}
