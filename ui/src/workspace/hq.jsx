import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { VerdictPanel } from './verdict-panel.jsx'
import ChecksPanel from './checks-panel.jsx'
const CheckCatalog = React.lazy(() => import('./check-catalog.jsx').then(m => ({ default: m.CheckCatalog })))

/* Every string this file renders inside the Verdict tabpanel comes from
 * HQ_COPY.chrome (ADJUDICATIONS item D finding 5). */
const C = HQ_COPY.chrome

class CatalogBoundary extends React.Component {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() {
    return this.state.failed ? <section className="ws-hq-section"><h2>{C.catalog_failed.title}</h2><p>{C.catalog_failed.body}</p><button onClick={() => window.location.reload()}>{C.catalog_failed.reload}</button></section> : this.props.children
  }
}

/** The panel while the session is being read; the same words on either tab. */
function Reading() {
  return <div role="status" className="ws-hq-section"><h2>{C.reading.title}</h2><p>{C.reading.body}</p></div>
}

/* Modes. Two tabs — Verdict and Trace — plus two verification surfaces that
 * stay reachable without a tab: `checks` (ChecksPanel, reached by Re-check)
 * and `verify` / `tools` (the catalog, reached from ChecksPanel and the shell).
 * Anything else, including unset, is Verdict. */
const TABS = ['verdict', 'trace']
function normalizeMode(mode) {
  if (mode === 'checks') return 'checks'
  if (mode === 'verify' || mode === 'tools') return 'catalog'
  if (mode === 'trace' || mode === 'inspect' || mode === 'inspector') return 'trace'
  return 'verdict'
}

export function HqWorkspace({ session, mode, onMode, selected, reading, onInspect, onCopy, inspector, onShowProof, register, onRegister }) {
  const m = normalizeMode(mode)
  const active = m === 'trace' ? 'trace' : 'verdict'
  /* The register survives a Verdict → Trace → Verdict round trip; the panel
   * remounts per tab, so the choice is held here unless the shell owns it. */
  const [localRegister, setLocalRegister] = React.useState('manager')
  const reg = register || localRegister
  const setReg = onRegister || setLocalRegister
  const showProof = onShowProof || ((id) => onInspect && onInspect(id))
  return <>
    <div className="ws-hq-tabs" role="tablist" aria-label={C.tabs_label}>{TABS.map((id) => <button key={id} role="tab" id={`hq-tab-${id}`} aria-controls={`hq-panel-${id}`} aria-selected={active === id} tabIndex={active === id ? 0 : -1} onClick={() => onMode(id)} onKeyDown={e => {
      if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) {
        e.preventDefault(); const next = e.key === 'Home' ? TABS[0] : e.key === 'End' ? TABS[1] : TABS[(TABS.indexOf(active) + 1) % 2]
        onMode(next); document.getElementById(`hq-tab-${next}`)?.focus()
      }
    }}>{HQ_COPY.chrome.tabs[id]}</button>)}</div>
    <ChecksPanel session={session} active={m === 'checks'} onMode={onMode} selected={selected} reading={reading} onInspect={onInspect} onCopy={onCopy}/>
    {m === 'checks' ? null : m === 'catalog' ? <div className="ws-hq-content" role="tabpanel" id="hq-panel-verdict" aria-labelledby="hq-tab-verdict">
      <div className="ws-checks-return"><button onClick={() => onMode('verdict')}>← {HQ_COPY.chrome.tabs.verdict}</button></div>
      <CatalogBoundary><React.Suspense fallback={<p className="ws-hq-section" role="status">{C.catalog_loading}</p>}><CheckCatalog key={mode === 'tools' ? 'tools' : 'verify'} session={session} initialSurface={mode === 'tools' ? 'tools' : 'local'} onInspect={onInspect} onCopy={onCopy} /></React.Suspense></CatalogBoundary>
    </div> : m === 'trace' ? <div className="ws-hq-content" role="tabpanel" id="hq-panel-trace" aria-labelledby="hq-tab-trace">
      {/* The Inspector owns the action row (Show me the proof · surrounding
       * events · Copy reference · Open receipt · Copy Markdown · Command
       * drawer) and renders it whenever it has a selection; with nothing
       * selected it renders an Absence with no buttons. A second row here
       * duplicated every button under a selection and was inert without one. */}
      {reading ? <Reading /> : inspector}
    </div> : <div className="ws-hq-content" role="tabpanel" id="hq-panel-verdict" aria-labelledby="hq-tab-verdict">
      {reading ? <Reading />
        /* Keyed on the session so a session switch resets the panel's
         * landing state; a tab round trip keeps the key and the remembered
         * landing in sessionStorage renders the verdict at once. */
        : <VerdictPanel key={session && session.id ? String(session.id) : 'no-session'} session={session} selected={selected} register={reg} onRegister={setReg} onShowProof={showProof} onCopy={onCopy} onRecheck={() => onMode('checks')} onOpenTrace={() => onMode('trace')} />}
    </div>}
  </>
}

export function Connections({ onClose, onSessions, session }) {
  const [cell, setCell] = React.useState(['claude-code', 'sendMessage'])
  const capabilities = [['signIn', 'Account sign-in'], ['discover', 'Session discovery'], ['attach', 'Attach to a session'], ['history', 'Recorded history'], ['liveEvents', 'Live events'], ['permissions', 'Permission prompts'], ['sendMessage', 'Send a message']]
  const providers = [
    { key: 'claude-code', name: 'Claude Code', mono: 'CC', description: 'Live connection has not been implemented in this build. Existing captured Claude Code records can still be inspected.' },
    { key: 'codex', name: 'Codex', mono: 'CX', description: 'Live connection has not been implemented in this build. Provider-managed sign-in and live messaging are separate integration work.' },
    { key: 'record', name: 'Local evidence', mono: 'EX', description: 'The workspace reads captured sessions from the local ThruLine server. Refreshing a reading does not attach to or stop an agent.' },
  ]
  const supported = (p, c) => p === 'record' && ['discover', 'history'].includes(c)
  const selectedProvider = providers.find(p => p.key === cell[0])
  return <div className="ws-connections-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) onClose() }}><section className="ws-connections" role="dialog" aria-modal="true" aria-labelledby="connections-heading" onKeyDown={e => { if (e.key === 'Escape') onClose(); if (e.key === 'Tab') { const list = Array.from(e.currentTarget.querySelectorAll('button, a, input, select')).filter(n => !n.disabled); if (e.shiftKey && document.activeElement === list[0]) { e.preventDefault(); list.at(-1)?.focus() } else if (!e.shiftKey && document.activeElement === list.at(-1)) { e.preventDefault(); list[0]?.focus() } } }}>
    <header><div><h2 id="connections-heading">Connections</h2><p>Local records available · agent connections not configured</p></div><button autoFocus aria-label="Close connections" onClick={onClose}>×</button></header>
    <div className="ws-capabilities"><div className="ws-cap-row"><span>Capability</span>{providers.map(p => <strong key={p.key}>{p.name}</strong>)}</div>{capabilities.map(([c, label]) => <div className="ws-cap-row" key={c}><span>{label}</span>{providers.map(p => <button key={p.key} aria-label={`${p.name} · ${label} · ${supported(p.key, c) ? "Available" : p.key === "record" ? "Not applicable" : "Not connected"}`} aria-pressed={cell[0] === p.key && cell[1] === c} onClick={() => setCell([p.key, c])}>{supported(p.key, c) ? '● Available' : p.key === 'record' ? '— Not applicable' : '◌ Not connected'}</button>)}</div>)}</div>
    <div className="ws-cap-detail"><strong>{selectedProvider.name} · {capabilities.find(c => c[0] === cell[1])[1]}</strong><p>{supported(...cell) ? 'Read captured sessions for this project from the local evidence store. This is a reading of stored records, not an authenticated live agent connection.' : selectedProvider.description}</p></div>
    <div className="ws-provider-cards">{providers.map(p => <article key={p.key}><span className="ws-provider-mono">{p.mono}</span><h3>{p.name}</h3><p>{p.description}</p>{p.key === 'record' ? <button onClick={onSessions}>Browse captured sessions</button> : <button disabled>Integration pending</button>}</article>)}</div>
    <p className="ws-hq-muted">Account access, agent activity, captured evidence, and verification results are separate. Nothing on this screen collects provider credentials or interrupts a running agent.</p>
  </section></div>
}
