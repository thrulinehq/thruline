import React from 'react'
import { SLUG_FOR_WORD } from '../lib/board.js'
import { MAC_GLYPH, bord, chipsFromCounts, chipsFromRow, svText, tint } from '../lib/macmap.js'
import {
  MIND_ENDPOINTS,
  deriveMindResponse,
  deriveMindStatus,
  fixtureSourceDigest,
  interpretationBody,
  interpretationHttpFailure,
  mindCheckIsCurrent,
  mindCheckMayStart,
} from '../lib/mind.js'
import {
  attachExportedCountDiscrepancy,
  boundedCountDiscrepancy,
} from '../lib/fixture.js'
import { projectSearchResult } from '../lib/project-search.js'
import { countPhrase } from '../lib/voice.js'
import DecisionFlow from './decision-flow.jsx'
import Distilled from './distilled.jsx'
import Inspector from './inspector.jsx'
import WorkMap from './work-map.jsx'

/* ThruLine — the macOS shell.
 *
 * Screens: Projects (from the §9 index alone) and Graph (one project file,
 * loaded on open — a prop is not a load). Candidate-bearing exports open on a
 * deterministic work map; the bounded event stream and distilled reference
 * are subordinate doors. The inspector keeps L2's receipt beside each view.
 *
 * The load discipline survives the redesign: the sidebar and the Projects
 * grid render from the index; opening a project fetches exactly one fixture;
 * no other project fixture is fetched. */

const PROJECT = (fixture) => `/fixtures/projects/${fixture}.json`

function Failure({ what, detail }) {
  return (
    <pre style={{ margin: 0, padding: '24px', font: '12px/1.6 var(--mono)', color: 'var(--state-contradicted)', whiteSpace: 'pre-wrap' }}>
      {`ThruLine — cannot render.\n\n${what}\n\n${detail || ''}`}
    </pre>
  )
}

function CountMismatchBanner({ discrepancy }) {
  const bounded = boundedCountDiscrepancy(discrepancy)
  const rows = [
    ...bounded.missing.map((item) => ({ ...item, kind: 'missing' })),
    ...bounded.extra.map((item) => ({ ...item, kind: 'extra' })),
    ...bounded.unequal.map((item) => ({ ...item, kind: 'unequal' })),
  ]
  return (
    <section className="tl-count-mismatch" role="alert" aria-label="Exported count mismatch">
      <div>
        <strong>Exported totals do not match this record</strong>
        <p>The graph remains available, but its totals are not reliable.</p>
      </div>
      <ul aria-label="Count differences">
        {rows.map((item) => (
          <li key={`${item.kind}:${item.key}`}>
            <code>{item.key}</code>
            {item.kind === 'missing' ? <span>export missing · records {item.nodes}</span> : null}
            {item.kind === 'extra' ? <span>export {item.exported ?? 'invalid'} · records missing</span> : null}
            {item.kind === 'unequal' ? <span>export {item.exported ?? 'invalid'} · records {item.nodes}</span> : null}
          </li>
        ))}
        {bounded.omitted > 0 ? <li><span>{bounded.omitted} more count differences</span></li> : null}
      </ul>
    </section>
  )
}

const SECTION_H = {
  margin: 0,
  padding: '4px 8px 5px',
  font: '590 10px/1 var(--sans)',
  letterSpacing: '.085em',
  textTransform: 'uppercase',
  color: 'var(--text-2)',
  display: 'flex',
  alignItems: 'center',
  gap: '6px',
}

function SidebarRow({ row, active, onOpen }) {
  const chips = chipsFromRow(row, SLUG_FOR_WORD).slice(0, 3)
  return (
    <button
      type="button"
      onClick={() => onOpen(row)}
      title={row.name}
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: '8px',
        padding: '6px 8px',
        borderRadius: 'var(--r-control)',
        textAlign: 'left',
        background: active ? 'color-mix(in oklch, var(--accent) 16%, transparent)' : 'transparent',
        width: '100%',
      }}
    >
      <span style={{ minWidth: 0, flex: 1, display: 'flex', flexDirection: 'column', gap: '3px' }}>
        <span style={{ font: '500 13px/1.2 var(--mono)', color: 'var(--text-1)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {row.name}
        </span>
        {chips.length > 0 ? (
        <span style={{ display: 'flex', gap: '7px', flexWrap: 'wrap' }}>
          {chips.map((c) => (
            <span key={c.state} title={`${c.label}: ${c.n}`} style={{ display: 'inline-flex', alignItems: 'center', gap: '3px', font: '500 10.5px/1 var(--mono)', color: svText(c.state) }}>
              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d={c.glyph} />
              </svg>
              {c.n}
            </span>
          ))}
        </span>
        ) : null}
      </span>
      {row.recency ? (
        <span style={{ font: '500 10.5px/1.2 var(--mono)', color: 'var(--text-2)', whiteSpace: 'nowrap', paddingTop: '1px' }}>{row.recency}</span>
      ) : null}
    </button>
  )
}

function ProjectCard({ row, onOpen }) {
  const chips = chipsFromRow(row, SLUG_FOR_WORD)
  const source = row.source === 'codex' ? 'Codex' : 'Claude Code'
  return (
    <button
      type="button"
      onClick={() => onOpen(row)}
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '11px',
        padding: '13px 14px 12px',
        borderRadius: 'var(--r-card)',
        textAlign: 'left',
        background: 'color-mix(in oklch, var(--surface-1) 78%, transparent)',
        border: '1px solid var(--hairline)',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: '8px' }}>
        <span style={{ minWidth: 0, font: '590 15px/1.25 var(--mono)', letterSpacing: '-.01em', color: 'var(--text-1)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {row.name}
        </span>
        <span style={{ flex: 'none', font: '500 9.5px/1 var(--sans)', letterSpacing: '.03em', padding: '4px 6px', borderRadius: '5px', color: 'var(--text-2)', background: 'var(--hairline)', whiteSpace: 'nowrap' }}>
          {source}
        </span>
      </div>
      {row.recency ? <span style={{ font: '400 11px/1 var(--mono)', color: 'var(--text-2)' }}>{row.recency}</span> : null}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '5px' }}>
        {chips.map((c) => (
          <span key={c.state} title={`${c.label}: ${c.n}`} style={{ display: 'inline-flex', alignItems: 'center', gap: '4px', height: '21px', padding: '0 7px', borderRadius: 'var(--r-chip)', font: '500 11px/1 var(--mono)', color: svText(c.state), background: tint(c.state), boxShadow: `inset 0 0 0 1px ${bord(c.state)}` }}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
              <path d={c.glyph} />
            </svg>
            {c.n}
          </span>
        ))}
      </div>
    </button>
  )
}

/* `embedded` (RA6): the graph lives on as the Trace tab of the
 * Recovery OS shell, which brings its own sidebar and window chrome — so the
 * embedded form drops this component's own sidebar and traffic lights and
 * keeps everything else exactly as RA5 shipped it. */
export default function MacShell({ index, embedded = false, initialFixture = null, initialNode = null }) {
  const projects = index.projects || []
  const [screen, setScreen] = React.useState('projects')
  const [altitude, setAltitude] = React.useState('work')
  const [row, setRow] = React.useState(null)
  const [fixture, setFixture] = React.useState(null)
  const [sourceDigest, setSourceDigest] = React.useState('')
  const [mindStatus, setMindStatus] = React.useState({ state: 'idle' })
  const [mindRuns, setMindRuns] = React.useState({})
  const [error, setError] = React.useState(null)
  const [selected, setSelected] = React.useState(null)
  /* A188: set when a deep-linked claim is not in the loaded document. */
  const [deepLinkMiss, setDeepLinkMiss] = React.useState(null)
  const [inspector, setInspector] = React.useState(false)
  const [filters, setFilters] = React.useState([])
  const [search, setSearch] = React.useState('')
  const [projectSearch, setProjectSearch] = React.useState('')
  const [sort, setSort] = React.useState('attention')
  const [sidebar, setSidebar] = React.useState(() => !window.matchMedia('(max-width: 900px)').matches)
  const [palette, setPalette] = React.useState(false)
  const [settings, setSettings] = React.useState(false)
  const [theme, setTheme] = React.useState('dark')
  const [reducedT, setReducedT] = React.useState(false)
  const projectLoadEpoch = React.useRef(0)
  const mindCheckEpoch = React.useRef(null)

  const closeSidebar = React.useCallback(() => {
    setSidebar(false)
  }, [])

  const openProject = React.useCallback(async (r) => {
    const epoch = ++projectLoadEpoch.current
    setError(null)
    setFixture(null)
    setSourceDigest('')
    setMindStatus({ state: 'idle' })
    setMindRuns({})
    setRow(r)
    setScreen('graph')
    setAltitude('work')
    setSelected(null)
    setInspector(false)
    setFilters([])
    if (window.matchMedia('(max-width: 900px)').matches) setSidebar(false)
    try {
      const res = await fetch(PROJECT(r.fixture), { cache: 'no-store' })
      if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`)
      const sourceText = await res.text()
      const doc = JSON.parse(sourceText)
      if (doc.diagnostics) throw new Error('diagnostics export — the product surface renders graduated classes only')
      if (projectLoadEpoch.current !== epoch) return
      const loadedFixture = attachExportedCountDiscrepancy(doc)
      if (loadedFixture._count_discrepancy) {
        console.error(boundedCountDiscrepancy(loadedFixture._count_discrepancy))
      }
      setFixture(loadedFixture)
      try {
        const digest = await fixtureSourceDigest(sourceText)
        if (projectLoadEpoch.current !== epoch) return
        setSourceDigest(digest)
      } catch (_digestError) {
        if (projectLoadEpoch.current !== epoch) return
        setMindStatus({ state: 'failure', reason: 'source_digest' })
      }
    } catch (e) {
      if (projectLoadEpoch.current !== epoch) return
      setError(String(e.message || e))
    }
  }, [])

  /* Availability is explanatory only. This one guarded callback serves both
     the initial check and the explicit recheck; neither path runs inference. */
  const checkMindStatus = React.useCallback(async (recovery = false) => {
    const epoch = projectLoadEpoch.current
    const ready = Boolean(fixture && sourceDigest)
    if (!mindCheckMayStart(ready, mindCheckEpoch.current, epoch)) return
    mindCheckEpoch.current = epoch
    setMindStatus({ state: 'checking', recovery: recovery === true })
    try {
      const response = await fetch(MIND_ENDPOINTS.status, {
        method: 'GET',
        cache: 'no-store',
        headers: { Accept: 'application/json' },
      })
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      const status = deriveMindStatus(await response.json())
      if (!mindCheckIsCurrent(mindCheckEpoch.current, projectLoadEpoch.current, epoch)) return
      setMindStatus(status.ok ? status : { state: 'failure', reason: 'status' })
    } catch (_error) {
      if (!mindCheckIsCurrent(mindCheckEpoch.current, projectLoadEpoch.current, epoch)) return
      setMindStatus({ state: 'failure', reason: 'status' })
    } finally {
      if (mindCheckIsCurrent(mindCheckEpoch.current, projectLoadEpoch.current, epoch)) {
        mindCheckEpoch.current = null
      }
    }
  }, [fixture, sourceDigest])

  React.useEffect(() => {
    if (!fixture || !sourceDigest) return
    void checkMindStatus(false)
  }, [checkMindStatus, fixture, sourceDigest])

  const recheckMindStatus = React.useCallback(() => {
    void checkMindStatus(true)
  }, [checkMindStatus])

  const analyzeRun = React.useCallback(async (session, coverage) => {
    if (mindStatus.state !== 'available') {
      setMindRuns((runs) => ({
        ...runs,
        [session]: { state: 'unavailable', message: 'Local analysis was not started because the fixed local model is not ready.' },
      }))
      return
    }
    const request = interpretationBody(row?.fixture, session)
    if (!request || !fixture || !sourceDigest || !coverage) {
      setMindRuns((runs) => ({
        ...runs,
        [session]: { state: 'invalid_request', message: 'This exported run could not form the fixed local request.' },
      }))
      return
    }

    const epoch = projectLoadEpoch.current
    setMindRuns((runs) => ({ ...runs, [session]: { state: 'analyzing' } }))
    try {
      const response = await fetch(MIND_ENDPOINTS.interpret, {
        method: 'POST',
        headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
        body: JSON.stringify(request),
      })
      if (projectLoadEpoch.current !== epoch) return
      if (!response.ok) {
        const failure = interpretationHttpFailure(response.status)
        if (failure.state === 'unavailable') setMindStatus({ state: 'unavailable' })
        setMindRuns((runs) => ({ ...runs, [session]: failure }))
        return
      }
      const result = await deriveMindResponse(await response.json(), {
        project: request.project,
        session,
        generatedAt: fixture.generated_at,
        sourceDigest,
        coverage,
        fixture,
      })
      if (projectLoadEpoch.current !== epoch) return
      setMindRuns((runs) => ({
        ...runs,
        [session]: result.ok
          ? { state: 'success', analysis: result }
          : { state: 'failure', message: 'The local interpretation response did not match the guarded contract.' },
      }))
    } catch (_error) {
      if (projectLoadEpoch.current !== epoch) return
      setMindRuns((runs) => ({
        ...runs,
        [session]: { state: 'failure', message: 'The local interpretation request did not return a validated result.' },
      }))
    }
  }, [fixture, mindStatus.state, row?.fixture, sourceDigest])

  const select = React.useCallback((id) => {
    setSelected(id)
    setInspector(true)
  }, [])

  /* Deep link (RA6): land directly on one project's graph. Real navigation,
     not a load rule change — the same single fixture fetch as a click. */
  React.useEffect(() => {
    if (!initialFixture) return
    const target = projects.find((p) => p.fixture === initialFixture)
    if (target) openProject(target)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialFixture])

  /* A188 — the EXACT-CLAIM deep link. An Inbox row whose action reads "Look at
     the receipt." lands on that claim, with the receipt one tap behind
     (Doctrine §1), rather than merely on its project.

     The miss branch is the load-bearing half. The index carries a reference
     into a document it does not load and cannot validate (Q-A188.2), so a
     `node_id` that does not resolve is possible — unlikely, because the item id
     is content-addressed and expires when its subject changes, but not
     impossible. When it happens the failure is VISIBLE: a link that silently
     lands on nothing teaches a person the link is decorative, which is worse
     than not offering one. */
  React.useEffect(() => {
    if (!initialNode || !fixture) return
    const found = (fixture.nodes || []).some((n) => n.id === initialNode)
    if (found) {
      setAltitude('events')
      setSelected(initialNode)
      setInspector(true)
      setDeepLinkMiss(null)
    } else {
      setSelected(null)
      setDeepLinkMiss(initialNode)
    }
  }, [initialNode, fixture])

  React.useEffect(() => {
    const onKey = (e) => {
      const meta = e.metaKey || e.ctrlKey
      if (meta && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPalette((p) => !p)
        setSettings(false)
        return
      }
      if (e.key === 'Escape') {
        setPalette(false)
        setSettings(false)
        setInspector(false)
        if (window.matchMedia('(max-width: 900px)').matches) closeSidebar()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [closeSidebar])

  /* Tokens are declared on :root, and `light-dark()` inside an unregistered
     custom property resolves against the color-scheme of the element that
     DECLARES it — so the scheme has to move on the root, not on this div, or
     the glass panels stay in the old theme while the canvas flips. */
  React.useEffect(() => {
    document.documentElement.style.colorScheme = theme === 'auto' ? 'light dark' : theme
  }, [theme])

  const projectResults = React.useMemo(
    () => projectSearchResult(projects, projectSearch, row?.fixture),
    [projects, projectSearch, row?.fixture],
  )
  const needing = projectResults.matching.filter((p) => (p.attention?.needing_attention || 0) > 0)
  const sortedMatches = [...projectResults.matching].sort((a, b) => {
    if (sort === 'name') return a.name.localeCompare(b.name)
    return (b.attention?.needing_attention || 0) - (a.attention?.needing_attention || 0)
  })
  const sorted = projectResults.currentPinned ? [projectResults.visible[0], ...sortedMatches] : sortedMatches

  const graphCounts = React.useMemo(() => {
    if (!fixture) return {}
    const counts = {}
    for (const c of chipsFromRow(row, SLUG_FOR_WORD)) counts[c.state] = c.n
    return counts
  }, [fixture, row])
  const filterChips = chipsFromCounts(graphCounts)
  const totalNodes = Object.values(graphCounts).reduce((a, b) => a + b, 0)

  return (
    <div
      className={`tl-shell${reducedT ? ' tl-rt' : ''}`}
      style={{ height: embedded ? '100%' : '100vh' }}
    >
      {/* ── Sidebar ── */}
      {embedded ? null : (
      <aside
        aria-label="Projects"
        className={`tl-project-sidebar${sidebar ? ' open' : ' compact'}`}
      >
        <div style={{ flex: 'none', height: '52px', display: 'flex', alignItems: 'center', padding: '0 14px', gap: '8px' }}>
          <span aria-hidden="true" style={{ width: '12px', height: '12px', borderRadius: '50%', background: '#ff5f57' }} />
          <span aria-hidden="true" style={{ width: '12px', height: '12px', borderRadius: '50%', background: '#febc2e' }} />
          <span aria-hidden="true" style={{ width: '12px', height: '12px', borderRadius: '50%', background: '#28c840' }} />
          <button type="button" className="tl-sidebar-toggle" onClick={() => setSidebar((s) => !s)} aria-label="Toggle sidebar" style={{ width: '26px', height: '26px', display: 'grid', placeItems: 'center', borderRadius: 'var(--r-chip)', color: 'var(--text-3)' }}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <rect x="3" y="3" width="18" height="18" rx="2" />
              <path d="M9 3v18" />
            </svg>
          </button>
          <button type="button" className="tl-mobile-project-close" aria-label="Close navigation" onClick={closeSidebar}>
            <span aria-hidden="true">×</span>
            <span>Close</span>
          </button>
        </div>

        {sidebar ? (
          <div className="tl-scroll" style={{ flex: 1, overflowY: 'auto', overflowX: 'hidden', padding: '2px 8px 10px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
            <label className="tl-project-search">
              <span aria-hidden="true">⌕</span>
              <input type="search" value={projectSearch} onChange={(event) => setProjectSearch(event.target.value)} placeholder="Search projects" aria-label="Search projects" />
            </label>
            <span className="tl-project-search-count">{projectResults.matchCount} of {projectResults.totalCount} projects match</span>
            {/* Needs attention: real attention data, and the group itself obeys
                §4 — at zero it does not exist. */}
            {needing.length > 0 ? (
              <section>
                <h3 style={SECTION_H}>
                  Needs attention <span style={{ fontFamily: 'var(--mono)', fontWeight: 400, opacity: 0.65 }}>{needing.length}</span>
                </h3>
                {needing.map((p) => (
                  <SidebarRow key={p.fixture} row={p} active={row?.fixture === p.fixture} onOpen={openProject} />
                ))}
              </section>
            ) : null}
            <section>
              <h3 style={SECTION_H}>
                All projects <span style={{ fontFamily: 'var(--mono)', fontWeight: 400, opacity: 0.65 }}>{projects.length}</span>
              </h3>
              {projectResults.visible.map((p) => (
                <SidebarRow key={p.fixture} row={p} active={row?.fixture === p.fixture} onOpen={openProject} />
              ))}
              {projectResults.matchCount === 0 ? <p className="tl-project-search-empty">No project names match. {projectResults.currentPinned ? 'The selected project stays visible.' : ''}</p> : null}
            </section>
          </div>
        ) : (
          <div style={{ flex: 1 }} />
        )}

        {sidebar ? (
          <div style={{ flex: 'none', margin: '0 8px 10px', padding: '9px 10px', borderRadius: 'var(--r-card)', background: 'color-mix(in oklch, var(--surface-2) 70%, transparent)', border: '1px solid var(--hairline)', display: 'flex', flexDirection: 'column', gap: '5px' }}>
            <span style={{ display: 'flex', alignItems: 'center', gap: '6px', font: '500 11px/1.3 var(--sans)', color: 'var(--text-1)' }}>
              <span style={{ width: '7px', height: '7px', borderRadius: '50%', flex: 'none', background: 'var(--state-supported)' }} />
              Local — nothing leaves this device
            </span>
            {index.totals?.newest_record ? (
              <span style={{ font: '400 10.5px/1.45 var(--mono)', color: 'var(--text-2)' }}>
                Reading records through {index.totals.newest_record}
              </span>
            ) : null}
          </div>
        ) : null}
      </aside>
      )}

      {!embedded && sidebar ? <div className="tl-project-backdrop" aria-hidden="true" onClick={closeSidebar} /> : null}

      {/* ── Right column ── */}
      <div className="tl-column">
        <header className="tl-toolbar">
          {!embedded ? (
            <button type="button" className="tl-mobile-project-toggle" aria-label="Open projects" onClick={() => setSidebar(true)}>
              <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"><path d="M4 6h16M4 12h16M4 18h16" /></svg>
            </button>
          ) : null}
          <nav aria-label="Breadcrumb" className="tl-toolbar-breadcrumb">
            <button type="button" onClick={() => setScreen('projects')} style={{ font: '500 13px/1 var(--sans)', color: screen === 'projects' ? 'var(--text-1)' : 'var(--text-2)' }}>
              Projects
            </button>
            {screen === 'graph' && row ? (
              <span style={{ display: 'flex', alignItems: 'center', gap: '6px', minWidth: 0 }}>
                <span style={{ color: 'var(--text-3)', opacity: 0.6 }}>›</span>
                <span className="tl-toolbar-project" title={row.name}>{row.name}</span>
              </span>
            ) : null}
          </nav>

          {screen === 'graph' && fixture?.candidates ? (
            <div className="tl-view-switcher" role="group" aria-label="Trace view">
              <button type="button" aria-pressed={altitude === 'work'} onClick={() => setAltitude('work')}>Work map</button>
              <button type="button" aria-pressed={altitude === 'events'} onClick={() => setAltitude('events')}>All events ({totalNodes})</button>
              <button type="button" aria-pressed={altitude === 'distilled'} onClick={() => setAltitude('distilled')}>Distilled summary</button>
            </div>
          ) : null}

          {screen === 'graph' && altitude === 'events' && filterChips.length > 0 ? (
            <div role="group" aria-label="Filter by state" className="tl-filter-group">
              <button
                type="button"
                aria-pressed={filters.length === 0}
                onClick={() => setFilters([])}
                style={{ display: 'flex', alignItems: 'center', gap: '5px', height: '26px', padding: '0 9px', borderRadius: 'var(--r-chip)', font: '500 11.5px/1 var(--sans)', background: filters.length === 0 ? 'var(--hairline-strong)' : 'transparent', color: filters.length === 0 ? 'var(--text-1)' : 'var(--text-2)', boxShadow: 'inset 0 0 0 1px var(--hairline)' }}
              >
                All <span style={{ fontFamily: 'var(--mono)', fontWeight: 400, opacity: 0.8 }}>{totalNodes}</span>
              </button>
              {filterChips.map((c) => {
                const on = filters.includes(c.state)
                return (
                  <button
                    key={c.state}
                    type="button"
                    aria-pressed={on}
                    title={`${c.label} · ${countPhrase(c.n, 'claim', 'claims')}`}
                    onClick={() => setFilters((f) => (f.includes(c.state) ? f.filter((x) => x !== c.state) : [...f, c.state]))}
                    style={{ display: 'flex', alignItems: 'center', gap: '5px', height: '26px', padding: '0 9px', borderRadius: 'var(--r-chip)', font: '500 11.5px/1 var(--sans)', whiteSpace: 'nowrap', background: on ? tint(c.state) : 'transparent', color: on ? svText(c.state) : 'var(--text-2)', boxShadow: `inset 0 0 0 1px ${on ? bord(c.state) : 'var(--hairline)'}` }}
                  >
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" style={{ flex: 'none' }}>
                      <path d={c.glyph} />
                    </svg>
                    {c.label} <span style={{ fontFamily: 'var(--mono)', fontWeight: 400, opacity: 0.8 }}>{c.n}</span>
                  </button>
                )
              })}
            </div>
          ) : (
            <div style={{ flex: 1 }} />
          )}

          <div className="tl-toolbar-actions">
            {screen === 'graph' && altitude === 'events' ? (
              <label className="tl-search">
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" style={{ flex: 'none', color: 'var(--text-3)' }}>
                  <circle cx="11" cy="11" r="7" />
                  <path d="m20 20-3.5-3.5" />
                </svg>
                <input type="text" value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search events or evidence" aria-label="Search events or evidence" />
              </label>
            ) : null}
            <button type="button" onClick={() => setPalette(true)} aria-label="Command palette" title="Command palette  ⌘K" style={{ width: '28px', height: '26px', display: 'grid', placeItems: 'center', borderRadius: 'var(--r-control)', color: 'var(--text-2)' }}>
              ⌘
            </button>
            <button type="button" onClick={() => setSettings(true)} aria-label="Settings" style={{ width: '28px', height: '26px', display: 'grid', placeItems: 'center', borderRadius: 'var(--r-control)', color: 'var(--text-2)' }}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="3" />
                <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.2 2.2M16.9 16.9l2.2 2.2M4.9 19.1l2.2-2.2M16.9 7.1l2.2-2.2" />
              </svg>
            </button>
          </div>
        </header>

        <div className="tl-content-row">
          <main className="tl-main tl-scroll" style={{ overflow: screen === 'graph' ? 'hidden' : 'auto' }}>
            {screen === 'graph' && fixture?._count_discrepancy ? (
              <CountMismatchBanner discrepancy={fixture._count_discrepancy} />
            ) : null}
            <div className="tl-main-surface">
              {screen === 'projects' ? (
              projects.length === 0 ? (
                <Empty index={index} />
              ) : (
                <div style={{ padding: '26px 26px 34px', display: 'flex', flexDirection: 'column', gap: '20px' }}>
                  <div style={{ display: 'flex', alignItems: 'flex-end', justifyContent: 'space-between', gap: '20px', flexWrap: 'wrap' }}>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '5px', minWidth: 0 }}>
                      <h1 className="tl-bal" style={{ margin: 0, font: '700 28px/1.15 var(--sans)', letterSpacing: '-.018em', color: 'var(--text-1)' }}>Projects</h1>
                      {index.totals?.headline ? (
                        <p style={{ margin: 0, font: '400 12.5px/1.4 var(--sans)', color: 'var(--text-2)' }}>{index.totals.headline}</p>
                      ) : null}
                    </div>
                    <div className="tl-project-controls">
                      <label className="tl-project-search">
                        <span aria-hidden="true">⌕</span>
                        <input type="search" value={projectSearch} onChange={(event) => setProjectSearch(event.target.value)} placeholder="Search projects" aria-label="Search projects on this page" />
                      </label>
                      <span className="tl-project-search-count">{projectResults.matchCount} of {projectResults.totalCount} projects match</span>
                      <div role="group" aria-label="Sort" style={{ display: 'flex', gap: '2px', padding: '2px', borderRadius: 'var(--r-control)', background: 'color-mix(in oklch, var(--surface-2) 76%, transparent)', boxShadow: 'inset 0 0 0 1px var(--hairline)' }}>
                        {[
                          ['attention', 'Needs attention'],
                          ['name', 'Name'],
                        ].map(([k, label]) => (
                          <button key={k} type="button" aria-pressed={sort === k} onClick={() => setSort(k)} style={{ height: '24px', padding: '0 11px', borderRadius: '5px', font: '500 11.5px/1 var(--sans)', whiteSpace: 'nowrap', background: sort === k ? 'var(--surface-1)' : 'transparent', color: sort === k ? 'var(--text-1)' : 'var(--text-2)' }}>
                            {label}
                          </button>
                        ))}
                      </div>
                    </div>
                  </div>
                  {projectResults.matchCount === 0 ? <p className="tl-project-search-empty">No project names match. {projectResults.currentPinned ? 'The selected project stays visible below.' : 'Clear the search to reach every project.'}</p> : null}
                  <div className="tl-grid">
                    {sorted.map((p) => (
                      <ProjectCard key={p.fixture} row={p} onOpen={openProject} />
                    ))}
                  </div>
                </div>
              )
            ) : error ? (
              <Failure what={`The record for ${row?.name} could not be read.`} detail={`${PROJECT(row?.fixture)}\n${error}`} />
            ) : !fixture ? (
              <p style={{ padding: '26px', font: '400 12px/1.6 var(--mono)', color: 'var(--text-3)' }}>Reading the record…</p>
            ) : (
              fixture.candidates && altitude === 'work' ? (
                <WorkMap
                  fixture={fixture}
                  projectId={row?.fixture}
                  sourceDigest={sourceDigest}
                  mindStatus={mindStatus}
                  mindRuns={mindRuns}
                  onAnalyze={analyzeRun}
                  onCheckMind={recheckMindStatus}
                  onSelect={select}
                  onAllEvents={() => setAltitude('events')}
                  onDistilled={() => setAltitude('distilled')}
                />
              ) : fixture.candidates && altitude === 'distilled' ? (
                <Distilled fixture={fixture} onEveryEvent={() => setAltitude('events')} />
              ) : (
                <DecisionFlow fixture={fixture} selected={selected} filters={filters} search={search} onSelect={select} />
              )
              )}
              {/* A188: the deep link missed. Said out loud, in the place the
                  claim would have appeared, rather than swallowed. */}
              {deepLinkMiss ? (
                <p style={{ position: 'absolute', left: 0, right: 0, bottom: 0, margin: 0, padding: '10px 14px', font: '400 12px/1.5 var(--sans)', color: 'var(--o, #ff9257)', background: 'rgba(0,0,0,.35)' }}>
                  That claim is not in this project&apos;s record any more, so it could not be opened. The project is shown instead.
                </p>
              ) : null}
            </div>
          </main>

          {screen === 'graph' && inspector && fixture && selected ? (
            <>
              <button type="button" className="tl-inspector-backdrop" aria-label="Close receipt" onClick={() => setInspector(false)} />
              <Inspector fixture={fixture} nodeId={selected} onClose={() => setInspector(false)} />
            </>
          ) : null}
        </div>
      </div>

      {palette ? (
        <Palette
          onClose={() => setPalette(false)}
          commands={[
            { label: 'Go to projects', run: () => setScreen('projects') },
            ...(screen === 'graph'
              ? filterChips.map((c) => ({ label: `Focus ${c.label.toLowerCase()} claims`, run: () => setFilters([c.state]) }))
              : []),
            { label: 'Toggle light/dark', run: () => setTheme((t) => (t === 'dark' ? 'light' : 'dark')) },
          ]}
        />
      ) : null}
      {settings ? (
        <Settings
          onClose={() => setSettings(false)}
          theme={theme}
          setTheme={setTheme}
          reducedT={reducedT}
          setReducedT={setReducedT}
        />
      ) : null}
    </div>
  )
}

const EMPTY_H = { margin: 0, font: '590 22px/1.25 var(--sans)', letterSpacing: '-.014em', color: 'var(--text-1)' }

function Empty({ index }) {
  const [copied, setCopied] = React.useState(false)
  /* The measured-zero sentence arrives from the authority via the export;
     the fallback covers an index written before `subhead` existed. */
  const headline = index.totals?.headline || 'No records read yet.'
  return (
    <div style={{ height: '100%', display: 'grid', placeItems: 'center', padding: '40px' }}>
      <div style={{ width: '440px', maxWidth: '100%', display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '18px', textAlign: 'center' }}>
        <h2 className="tl-bal" style={EMPTY_H}>{headline}</h2>
        {index.totals?.subhead ? (
          <p className="tl-pretty" style={{ margin: 0, font: '400 13px/1.55 var(--sans)', color: 'var(--text-2)', maxWidth: '38ch' }}>{index.totals.subhead}</p>
        ) : null}
        {/* The remedy is named and real: the command that reads sessions in.
            The design's drop-zone is deferred — the server has no upload
            endpoint, and a chooser that swallowed a file would be a lie. */}
        <button
          type="button"
          onClick={() => {
            navigator.clipboard?.writeText('thruline ingest ~/.claude/projects')
            setCopied(true)
            setTimeout(() => setCopied(false), 1600)
          }}
          style={{ font: '400 12.5px/1.4 var(--mono)', color: 'var(--text-1)', padding: '10px 14px', borderRadius: 'var(--r-control)', background: 'var(--surface-2)', boxShadow: 'inset 0 0 0 1px var(--hairline-strong)' }}
        >
          {copied ? 'Copied' : 'thruline ingest ~/.claude/projects'}
        </button>
      </div>
    </div>
  )
}

function Palette({ onClose, commands }) {
  return (
    <div onClick={onClose} style={{ position: 'fixed', inset: 0, zIndex: 10, background: 'oklch(0 0 0 / .34)', display: 'flex', justifyContent: 'center', alignItems: 'flex-start', paddingTop: '132px' }}>
      <div onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Command palette" style={{ width: '520px', borderRadius: 'var(--r-panel)', overflow: 'hidden', background: 'var(--glass-fill)', backdropFilter: 'blur(28px) saturate(180%)', boxShadow: 'inset 0 0 0 1px var(--hairline-strong), var(--shadow-panel)' }}>
        <div style={{ padding: '6px' }}>
          {commands.map((c) => (
            <button
              key={c.label}
              type="button"
              onClick={() => {
                c.run()
                onClose()
              }}
              style={{ width: '100%', display: 'flex', alignItems: 'center', gap: '10px', padding: '9px 10px', borderRadius: '8px', textAlign: 'left' }}
            >
              <span style={{ flex: 1, font: '400 13px/1.3 var(--sans)', color: 'var(--text-1)' }}>{c.label}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}

function Settings({ onClose, theme, setTheme, reducedT, setReducedT }) {
  return (
    <div onClick={onClose} style={{ position: 'fixed', inset: 0, zIndex: 10, background: 'oklch(0 0 0 / .34)', display: 'flex', justifyContent: 'center', alignItems: 'flex-start', paddingTop: '56px' }}>
      <div onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Settings" style={{ width: '480px', borderRadius: 'var(--r-panel)', background: 'var(--glass-fill)', backdropFilter: 'blur(28px) saturate(180%)', boxShadow: 'inset 0 0 0 1px var(--hairline-strong), var(--shadow-panel)' }}>
        <div style={{ display: 'flex', alignItems: 'center', padding: '14px 16px', borderBottom: '1px solid var(--hairline)' }}>
          <h2 style={{ margin: 0, font: '590 15px/1 var(--sans)', color: 'var(--text-1)' }}>Settings</h2>
          <button type="button" onClick={onClose} aria-label="Close" style={{ marginLeft: 'auto', width: '24px', height: '24px', display: 'grid', placeItems: 'center', borderRadius: '6px', color: 'var(--text-3)' }}>
            ✕
          </button>
        </div>
        <div style={{ padding: '16px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
          {/* Appearance only. The design's provider / data / ledger sections are
              deferred: this page has no settings API behind it, and controls
              that pretended to save would be fabricated receipts (A145). */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
            <span style={{ width: '112px', font: '400 13px/1 var(--sans)', color: 'var(--text-2)' }}>Theme</span>
            <div role="group" style={{ display: 'flex', gap: '2px', padding: '2px', borderRadius: 'var(--r-control)', background: 'color-mix(in oklch, var(--surface-2) 76%, transparent)', boxShadow: 'inset 0 0 0 1px var(--hairline)' }}>
              {['auto', 'dark', 'light'].map((k) => (
                <button key={k} type="button" aria-pressed={theme === k} onClick={() => setTheme(k)} style={{ height: '24px', padding: '0 13px', borderRadius: '5px', font: '500 11.5px/1 var(--sans)', background: theme === k ? 'var(--surface-1)' : 'transparent', color: theme === k ? 'var(--text-1)' : 'var(--text-2)', textTransform: 'capitalize' }}>
                  {k}
                </button>
              ))}
            </div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
            <span style={{ width: '112px', font: '400 13px/1.3 var(--sans)', color: 'var(--text-2)' }}>Reduce transparency</span>
            <button type="button" onClick={() => setReducedT(!reducedT)} role="switch" aria-checked={reducedT} style={{ position: 'relative', width: '42px', height: '24px', borderRadius: '999px', background: reducedT ? 'var(--accent)' : 'var(--hairline-strong)', transition: 'background var(--dur-2) ease' }}>
              <span style={{ position: 'absolute', top: '2px', width: '20px', height: '20px', borderRadius: '50%', background: '#fff', boxShadow: '0 1px 3px oklch(0 0 0 / .35)', transition: 'left var(--dur-2) var(--ease-spring)', left: reducedT ? '20px' : '2px' }} />
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
