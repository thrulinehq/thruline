import React from 'react'
import { SLUG_FOR_WORD } from '../lib/board.js'
import { calmIsEarned, hasReceipt, inboxQueue } from '../lib/inbox.js'
import { chipsFromRow, svText } from '../lib/macmap.js'
import { projectSearchResult } from '../lib/project-search.js'
import { ledgerLine } from '../lib/receipt.js'
import { countPhrase, word } from '../lib/voice.js'
import MacShell from '../mac/shell.jsx'
import Refresh from '../refresh.jsx'

/* ThruLine — the Recovery OS shell (RA6, design v3; IA reconciled at C7).
 * The Inbox is home and the graph tab is Trace, per NORTHSTAR-v3.
 *
 * Every tab here renders real data: agent-lane presence (a read the foreground
 * server makes per poll, while this page is open, M6.R4), watched projects
 * from the §9 index, the Audit Ledger tab from real fixtures, and the Trace
 * tab retained from RA5. The recovery-layer shadow surfaces (Incidents,
 * Recovery Runs, and Mission Control's outlook card) rendered review notes
 * instead of content and were deleted; their designed versions live in the
 * marked mock gallery, and a pin walks these files to keep placeholders out. */

const PROJECT = (fixture) => `/fixtures/projects/${fixture}.json`

/* Home is the Inbox (C7). The v3 north star names the home surface an incident
 * inbox whose healthy state is "Nothing needs you", and the one attention queue
 * in this app lives there — Mission Control's "Needs attention" card was its
 * ancestor and does not survive alongside it. */
const TABS = [
  ['inbox', 'Inbox'],
  ['mission', 'Mission Control'],
  ['graph', 'Trace'],
  ['ledger', 'Audit Ledger'],
]

function Icon({ d, size = 16 }) {
  return (
    <svg className="os-icon" width={size} height={size} viewBox="0 0 24 24">
      <path d={d} />
    </svg>
  )
}

const I = {
  inbox: 'M4 4h16l1 9v6a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-6zM3 13h5l2 3h4l2-3h5',
  grid: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z',
  graph: 'M5 6m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0M19 6m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0M12 18m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0M7 7l4 9M17 7l-4 9M7 6h10',
  ledger: 'M4 3h16v18H4zM8 7h8M8 11h8M8 15h5',
  activity: 'M3 12h4l2-7 4 14 2-7h6',
}

/* Tab key → icon. A map rather than a ternary chain: adding a tab must be one
 * edit in one place, and a missing entry renders nothing instead of silently
 * borrowing the last branch's icon. */
const TAB_ICON = {
  inbox: 'inbox',
  mission: 'grid',
  graph: 'graph',
  ledger: 'ledger',
}

function useLanes() {
  const [lanes, setLanes] = React.useState(null)
  React.useEffect(() => {
    let live = true
    const poll = async () => {
      try {
        const res = await fetch('/api/lanes', { cache: 'no-store' })
        if (live && res.ok) setLanes(await res.json())
      } catch {
        /* npm-dev or server gone: the lanes region simply does not render. */
      }
    }
    poll()
    /* The foreground loop: alive only while this page is open, cleared on
       unmount, nothing on the server side outlives the request it answers. */
    const t = setInterval(poll, 10_000)
    return () => {
      live = false
      clearInterval(t)
    }
  }, [])
  return lanes
}

function Lane({ label, live, provenance, roadmap }) {
  return (
    <div
      title={provenance}
      style={{ display: 'flex', alignItems: 'center', gap: '9px', padding: '7px 10px', borderRadius: '8px', background: 'var(--s1)' }}
    >
      <span style={{ width: '22px', height: '22px', borderRadius: '6px', background: 'var(--s3)', display: 'grid', placeItems: 'center', font: '600 9px/1 var(--mono)', color: 'var(--t2)', flex: 'none' }}>
        {label === 'Claude Code' ? 'C' : label === 'Codex' ? 'CX' : 'G'}
      </span>
      <span style={{ flex: 1, minWidth: 0 }}>
        <span style={{ display: 'block', font: '500 12px/1.3 var(--sans)', color: 'var(--t1)' }}>{label}</span>
        <span style={{ display: 'block', font: '400 10.5px/1.3 var(--mono)', color: 'var(--t3)' }}>
          {roadmap ? 'on the roadmap' : live > 0 ? countPhrase(live, 'recording written recently', 'recordings written recently') : 'quiet in the last 5 minutes'}
        </span>
      </span>
      {/* Runtime vocabulary: the dot, and only the two runtime colours. */}
      {!roadmap ? <span className="os-runtime-dot" style={{ background: live > 0 ? 'var(--g)' : 'var(--s3)' }} /> : null}
    </div>
  )
}

export default function OsShell({ index }) {
  const projects = index.projects || []
  /* ?project=<fixture> deep-links Trace on that project, and ?node=<id>
     narrows that to one claim (A188). The route key and the `project`
     parameter both stay 'graph'/'project': they are addresses other things
     already hold, and renaming an address to match a label breaks the links
     without making one sentence truer.

     `node` is the URL form of what an Inbox row does on click, so a receipt is
     linkable and not only clickable — the same capability through the door a
     person can copy. */
  const [deepLink, deepNode] = React.useMemo(() => {
    const q = new URLSearchParams(window.location.search)
    return [q.get('project'), q.get('node')]
  }, [])
  const [tab, setTab] = React.useState(deepLink ? 'graph' : 'inbox')
  /* Which project Trace opens on. Seeded by the deep link, and reset when an
     Inbox row is followed — "see this in history" has to land on the project
     the row named, or it is a tab switch wearing a link's clothes. */
  const [traceFixture, setTraceFixture] = React.useState(deepLink)
  /* A188: which claim Trace opens on. Null for rows that have none — the row
     degrades to its project rather than inventing a claim to point at. */
  const [traceNode, setTraceNode] = React.useState(deepNode)
  const [projectSearch, setProjectSearch] = React.useState('')
  const [sidebarOpen, setSidebarOpen] = React.useState(false)
  const selectedProjectRef = React.useRef(null)
  const lanes = useLanes()

  const closeSidebar = React.useCallback(() => {
    setSidebarOpen(false)
  }, [])
  /* The app's one attention queue. Every count of "what needs me" on every
     surface below is read from this — one derivation, one number, nowhere for
     two views of the same fact to disagree. */
  const queue = React.useMemo(() => inboxQueue(index), [index])
  const projectResults = React.useMemo(
    () => projectSearchResult(projects, projectSearch, traceFixture),
    [projectSearch, projects, traceFixture],
  )

  React.useEffect(() => {
    selectedProjectRef.current?.scrollIntoView({ block: 'nearest' })
  }, [traceFixture, projectSearch, sidebarOpen])

  React.useEffect(() => {
    const onKey = (event) => {
      if (event.key === 'Escape') closeSidebar()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [closeSidebar])

  const openTraceProject = (project) => {
    setTraceFixture(project.fixture)
    setTraceNode(null)
    setTab('graph')
    setSidebarOpen(false)
  }

  return (
    <div className="os-root">
      {/* ── Sidebar ── */}
      <aside className={`os-sidebar os-scroll${sidebarOpen ? ' open' : ''}`} aria-label="Application and projects">
        <div style={{ display: 'flex', gap: '6px', padding: '14px 14px 8px' }} aria-hidden="true">
          <i style={{ width: '11px', height: '11px', borderRadius: '50%', background: '#f26c67' }} />
          <i style={{ width: '11px', height: '11px', borderRadius: '50%', background: '#e8bf53' }} />
          <i style={{ width: '11px', height: '11px', borderRadius: '50%', background: '#62db8b' }} />
        </div>
        <div className="os-sidebar-brand" style={{ display: 'flex', alignItems: 'center', gap: '9px', padding: '4px 14px 12px' }}>
          <span style={{ width: '26px', height: '26px', borderRadius: '8px', background: 'var(--as)', color: 'var(--a)', display: 'grid', placeItems: 'center' }}>
            <Icon d={I.activity} />
          </span>
          <span>
            <span style={{ display: 'block', font: '600 13px/1.2 var(--sans)' }}>ThruLine</span>
            <span style={{ display: 'block', font: '400 10px/1.3 var(--mono)', color: 'var(--t3)' }}>AI Session OS</span>
          </span>
          {sidebarOpen ? (
            <button type="button" className="os-sidebar-close" aria-label="Close navigation" onClick={closeSidebar}>
              <span aria-hidden="true">×</span>
              <span>Close</span>
            </button>
          ) : null}
        </div>

        <nav style={{ padding: '0 10px', display: 'flex', flexDirection: 'column', gap: '2px' }}>
          {TABS.map(([key, label]) => (
            <button key={key} type="button" className={`os-nav${tab === key ? ' active' : ''}`} onClick={() => { setTab(key); setSidebarOpen(false) }}>
              <Icon d={I[TAB_ICON[key]]} />
              <span style={{ flex: 1 }}>{label}</span>
              {key === 'inbox' && queue.rows.length > 0 ? (
                <span data-machine="evidence" className="os-evidence-count" title={countPhrase(queue.rows.length, 'thing worth a look', 'things worth a look')}>
                  {queue.rows.length}
                </span>
              ) : null}
            </button>
          ))}
        </nav>

        {/* ── Runtime health region ── */}
        <div data-machine="runtime" style={{ padding: '14px 10px 4px' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', padding: '0 4px 6px', font: '600 9.5px/1 var(--sans)', letterSpacing: '.08em', textTransform: 'uppercase', color: 'var(--t3)' }}>
            <span>Agent lanes</span>
            {lanes ? <span title={lanes.measured}>{`last ${lanes.window_minutes} min`}</span> : null}
          </div>
          {lanes ? (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
              <Lane label="Claude Code" live={lanes.claude_code?.live || 0} provenance={`${lanes.measured} · ${lanes.claude_code?.root}`} />
              <Lane label="Codex" live={lanes.codex?.live || 0} provenance={`${lanes.measured} · ${lanes.codex?.root}`} />
              <Lane label="Gemini CLI" roadmap provenance="planned lane — nothing is read for it yet" />
            </div>
          ) : (
            <p style={{ margin: 0, padding: '0 4px', font: '400 10.5px/1.5 var(--mono)', color: 'var(--t3)' }}>
              Lane presence needs the ThruLine app serving this page.
            </p>
          )}
        </div>

        <div style={{ padding: '14px 10px', flex: 1 }}>
          <div className="os-project-heading">
            <span>Watched projects</span>
            <span>{projectResults.matchCount} / {projectResults.totalCount}</span>
          </div>
          <label className="os-project-search">
            <span aria-hidden="true">⌕</span>
            <input value={projectSearch} onChange={(event) => setProjectSearch(event.target.value)} type="search" placeholder="Search projects" aria-label="Search projects" />
          </label>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
            {projectResults.visible.map((p) => (
              <button
                key={p.fixture}
                ref={p.fixture === traceFixture ? selectedProjectRef : null}
                type="button"
                className={`os-nav${p.fixture === traceFixture ? ' active' : ''}`}
                onClick={() => openTraceProject(p)}
                title={p.chip_line || p.name}
              >
                <span style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', font: '500 12px/1.3 var(--mono)' }}>{p.name}</span>
                {p.recency ? <span style={{ font: '400 10px/1 var(--mono)', color: 'var(--t3)' }}>{p.recency}</span> : null}
              </button>
            ))}
            {projectResults.matchCount === 0 ? <span className="os-project-empty">No project names match this search. {projectResults.currentPinned ? 'The selected project stays visible.' : ''}</span> : null}
          </div>
        </div>

        <div data-machine="runtime" style={{ margin: '0 10px 12px', padding: '10px 12px', borderRadius: 'var(--rad)', background: 'var(--s1)', border: '1px solid var(--line)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '7px', font: '500 11.5px/1.3 var(--sans)' }}>
            <span className="os-runtime-dot" style={{ background: 'var(--g)' }} />
            Local — nothing leaves this device
          </div>
          <div style={{ marginTop: '5px', font: '400 10.5px/1.5 var(--mono)', color: 'var(--t3)' }}>
            {countPhrase(index.totals?.projects ?? 0, 'project read', 'projects read')}
            {index.totals?.newest_record ? <> · records through {index.totals.newest_record}</> : null}
          </div>
        </div>
      </aside>
      {sidebarOpen ? <div className="os-sidebar-backdrop" aria-hidden="true" onClick={closeSidebar} /> : null}

      {/* ── Right column ── */}
      <section className="os-main">
        <header className="os-topbar">
          <button type="button" className="os-menu-button" aria-label="Open navigation and projects" onClick={() => setSidebarOpen(true)}>
            <span aria-hidden="true">☰</span>
          </button>
          <span className="os-topbar-title">{TABS.find(([k]) => k === tab)?.[1]}</span>
          <span style={{ flex: 1 }} />
          {/* The Inbox states the queue in its own body; repeating it in the
              chrome above puts one idea on the screen twice (doctrine §6). */}
          {tab === 'inbox' ? null : queue.rows.length > 0 ? (
            <span data-machine="evidence" className="os-evidence-line">
              {countPhrase(queue.rows.length, 'thing worth a look', 'things worth a look')}
            </span>
          ) : calmIsEarned(queue) && index.totals?.headline ? (
            /* The calm sentence renders only when the queue says it was earned.
               The index composes this headline from a needing-attention count
               that a project returning NO item would leave at zero, so trusting
               the string alone is how a calm screen gets told over projects
               nobody looked at. */
            <span data-machine="evidence" className="os-evidence-line calm">{index.totals.headline}</span>
          ) : null}
          <Refresh />
        </header>

        <div className="os-scroll" style={{ flex: 1, minHeight: 0, overflow: tab === 'graph' ? 'hidden' : 'auto' }}>
          {tab === 'inbox' ? (
            <Inbox
              index={index}
              queue={queue}
              onTrace={(fixture, nodeId) => {
                setTraceFixture(fixture)
                setTraceNode(nodeId || null)
                setTab('graph')
              }}
            />
          ) : null}
          {tab === 'mission' ? <Mission index={index} lanes={lanes} /> : null}
          {tab === 'graph' ? (
            <div style={{ height: '100%', position: 'relative' }}>
              <MacShell index={index} embedded initialFixture={traceFixture} initialNode={traceNode} />
            </div>
          ) : null}
          {tab === 'ledger' ? <LedgerTab index={index} /> : null}
        </div>
      </section>
    </div>
  )
}

function Card({ title, children, note, machine }) {
  return (
    <section data-machine={machine} style={{ borderRadius: 'var(--rad)', background: 'var(--s1)', border: '1px solid var(--line)', padding: '13px 15px' }}>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '10px', marginBottom: '9px' }}>
        <h2 style={{ margin: 0, font: '600 11.5px/1.3 var(--sans)' }}>{title}</h2>
        {note ? <span style={{ font: '400 10.5px/1.3 var(--mono)', color: 'var(--t3)' }}>{note}</span> : null}
      </div>
      {children}
    </section>
  )
}

/* The Inbox — home, and the app's only attention queue.
 *
 * One line per item: project · sentence · action. Both strings are composed by
 * the Rust voice authority and carried verbatim in the §9 index; nothing on
 * this surface writes English about a finding. Ordering is C7-O1, named and
 * asserted in lib/inbox.js — the queue may not reshuffle under a person.
 *
 * Calm here is a *finding*, not a blank screen, and it is one the record has to
 * earn: `calmIsEarned` is false the moment any project came back unevaluated,
 * and then the surface says the weaker true thing instead. */
function Inbox({ index, queue, onTrace }) {
  const earned = calmIsEarned(queue)
  return (
    <div data-machine="evidence" style={{ padding: '22px 24px', display: 'flex', flexDirection: 'column', gap: '14px', maxWidth: '860px' }}>
      <div>
        <h1 style={{ margin: '0 0 4px', font: '600 22px/1.2 var(--sans)' }}>Inbox</h1>
        <p style={{ margin: 0, font: '400 13px/1.5 var(--sans)', color: 'var(--t2)' }}>
          {queue.rows.length > 0
            ? countPhrase(queue.rows.length, 'thing worth a look', 'things worth a look')
            : earned
              ? index.totals?.headline
              : countPhrase(queue.silent.length, 'project reported nothing', 'projects reported nothing')}
        </p>
      </div>

      {queue.rows.length > 0 ? (
        <div style={{ display: 'flex', flexDirection: 'column' }}>
          {queue.rows.map((r) => (
            <div key={r.id} style={{ display: 'grid', gridTemplateColumns: '170px 1fr', gap: '14px', alignItems: 'baseline', padding: '10px 0', borderTop: '1px solid var(--line)' }}>
              <button
                type="button"
                onClick={() => onTrace(r.fixture)}
                title={`Open ${r.project} in Trace`}
                style={{ background: 'none', textAlign: 'left', color: 'var(--t3)', font: '400 11px/1.5 var(--mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              >
                {r.project}
              </button>
              <span style={{ font: '400 13px/1.6 var(--sans)', color: 'var(--t1)' }}>
                {r.sentence}{' '}
                {/* A188: the action becomes the receipt link ONLY when the row
                    has a claim to point at. A row without one keeps the plain
                    sentence and its project link — it does not offer a receipt
                    it cannot open, because an action that says "Look at the
                    receipt" and lands nowhere is worse than one that stays
                    text. The suggested action still never executes anything
                    (doctrine): this opens a view, it does not run the project's
                    work. */}
                {hasReceipt(r) ? (
                  <button
                    type="button"
                    onClick={() => onTrace(r.fixture, r.nodeId)}
                    title={`Open this claim in Trace · ${r.project}`}
                    style={{ background: 'none', padding: 0, color: 'var(--a)', font: 'inherit', textDecoration: 'underline', textUnderlineOffset: '2px' }}
                  >
                    {r.action}
                  </button>
                ) : (
                  <span style={{ color: 'var(--t2)' }}>{r.action}</span>
                )}
              </span>
            </div>
          ))}
        </div>
      ) : earned ? (
        <p style={{ margin: 0, font: '400 12.5px/1.6 var(--sans)', color: 'var(--t3)' }}>
          Nothing needs you. {countPhrase(queue.calm, 'project was read and checked', 'projects were read and checked')}.
        </p>
      ) : (
        <p style={{ margin: 0, font: '400 12.5px/1.6 var(--sans)', color: 'var(--o)' }}>
          {queue.silent.length > 0
            ? 'Some projects came back with no result at all, so nothing can be said about them yet.'
            : 'No records have been read on this machine yet.'}
        </p>
      )}

      {queue.unrecognized.length > 0 ? (
        <p style={{ margin: 0, font: '400 11px/1.5 var(--mono)', color: 'var(--t3)' }}>
          {countPhrase(queue.unrecognized.length, 'item kind is newer than this app', 'item kinds are newer than this app')}
          {'; they are listed last rather than hidden.'}
        </p>
      ) : null}
    </div>
  )
}

function Mission({ index, lanes }) {
  const projects = index.projects || []
  return (
    <div style={{ padding: '22px 24px', display: 'flex', flexDirection: 'column', gap: '14px', maxWidth: '1080px' }}>
      <div>
        <div style={{ font: '600 10px/1 var(--sans)', letterSpacing: '.09em', textTransform: 'uppercase', color: 'var(--a)' }}>Local orchestration</div>
        <h1 style={{ margin: '6px 0 4px', font: '600 22px/1.2 var(--sans)' }}>Mission Control</h1>
        {index.totals?.headline ? (
          <p style={{ margin: 0, font: '400 13px/1.5 var(--sans)', color: 'var(--t2)' }}>{index.totals.headline}</p>
        ) : null}
      </div>

      {/* The attention queue that stood here is now the Inbox, and is not
          duplicated: two queues over one derivation are two places for the same
          fact to disagree, and the one that is not home is the one that goes
          stale unnoticed. `there is exactly one attention queue in the app`
          (inbox.test.js) fails if it comes back. */}

      <Card machine="runtime" title="Agent lanes" note={lanes ? `read on request · last ${lanes.window_minutes} min` : undefined}>
        {lanes ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
            <Lane label="Claude Code" live={lanes.claude_code?.live || 0} provenance={`${lanes.measured} · ${lanes.claude_code?.root}`} />
            <Lane label="Codex" live={lanes.codex?.live || 0} provenance={`${lanes.measured} · ${lanes.codex?.root}`} />
            <Lane label="Gemini CLI" roadmap provenance="planned lane — nothing is read for it yet" />
          </div>
        ) : (
          <p style={{ margin: 0, font: '400 11.5px/1.5 var(--mono)', color: 'var(--t3)' }}>
            Lane presence needs the ThruLine app serving this page.
          </p>
        )}
      </Card>

      <Card title="Watched projects" note={countPhrase(projects.length, 'project', 'projects')}>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(230px, 1fr))', gap: '8px' }}>
          {projects.slice(0, 12).map((p) => (
            <div key={p.fixture} style={{ padding: '9px 11px', borderRadius: '8px', background: 'var(--s2)' }}>
              <div style={{ font: '500 12px/1.3 var(--mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.name}</div>
              <div style={{ marginTop: '3px', font: '400 10.5px/1.4 var(--sans)', color: 'var(--t3)' }}>
                {p.chip_line || 'nothing tallied yet'}
                {p.recency ? ` · ${p.recency}` : ''}
              </div>
            </div>
          ))}
        </div>
        {projects.length > 12 ? (
          <p style={{ margin: '8px 0 0', font: '400 10.5px/1.4 var(--mono)', color: 'var(--t3)' }}>
            {countPhrase(projects.length - 12, 'more project in Trace', 'more projects in Trace')}
          </p>
        ) : null}
      </Card>
    </div>
  )
}

/* The Audit Ledger tab — real fixtures only: pick a project, see its chain
 * condition (the A150 split, spoken by the authority) and its claim stream
 * with the anchor event id every row traces to. */
function LedgerTab({ index }) {
  const projects = index.projects || []
  const [row, setRow] = React.useState(projects[0] || null)
  const [doc, setDoc] = React.useState(null)
  const [error, setError] = React.useState(null)

  React.useEffect(() => {
    if (!row) return undefined
    let live = true
    setDoc(null)
    setError(null)
    fetch(PROJECT(row.fixture), { cache: 'no-store' })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((d) => {
        if (live) setDoc(d)
      })
      .catch((e) => {
        if (live) setError(String(e.message || e))
      })
    return () => {
      live = false
    }
  }, [row])

  const rows = React.useMemo(() => {
    if (!doc) return []
    return (doc.nodes || []).slice(0, 200).map((n) => {
      const r = doc.receipts?.[n.id]
      return {
        id: n.id,
        word: word(n.sort_key || 'unknown'),
        state: n.sort_key || 'unknown',
        text: r?.claim?.text || n.label || '',
        anchor: n.anchor_event_id,
        ts: r?.claim?.ts || '',
      }
    })
  }, [doc])

  return (
    <div style={{ padding: '22px 24px', display: 'flex', flexDirection: 'column', gap: '12px', maxWidth: '1080px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '12px', flexWrap: 'wrap' }}>
        <h1 style={{ margin: 0, font: '600 20px/1.2 var(--sans)' }}>Audit Ledger</h1>
        <select
          value={row?.fixture || ''}
          onChange={(e) => setRow(projects.find((p) => p.fixture === e.target.value) || null)}
          style={{ background: 'var(--s2)', border: '1px solid var(--line2)', borderRadius: '7px', padding: '5px 8px', color: 'var(--t1)', font: '400 12px/1 var(--mono)' }}
        >
          {projects.map((p) => (
            <option key={p.fixture} value={p.fixture}>
              {p.name}
            </option>
          ))}
        </select>
      </div>
      {error ? (
        <p style={{ margin: 0, font: '400 12px/1.5 var(--mono)', color: 'var(--r)' }}>The record could not be read. {error}</p>
      ) : !doc ? (
        <p style={{ margin: 0, font: '400 12px/1.5 var(--mono)', color: 'var(--t3)' }}>Reading the record…</p>
      ) : (
        <>
          <Card title="Chain condition" note={doc.ledger?.head ? `head ${String(doc.ledger.head).slice(0, 12)}…` : undefined}>
            {/* The project-level ledger speaks `events`; the receipt-level
                dialect ledgerLine reads speaks `total`. Adapt the field name,
                never the facts. */}
            <p style={{ margin: 0, font: '400 12.5px/1.5 var(--mono)', color: 'var(--t1)' }}>
              {ledgerLine(doc.ledger ? { ...doc.ledger, total: doc.ledger.events ?? 0 } : null)}
            </p>
          </Card>
          <Card title="Claim stream" note={countPhrase(rows.length, 'row shown', 'rows shown')}>
            <div style={{ display: 'flex', flexDirection: 'column' }}>
              {rows.map((r) => (
                <div key={r.id} style={{ display: 'grid', gridTemplateColumns: '92px 1fr 240px', gap: '12px', alignItems: 'baseline', padding: '6px 0', borderTop: '1px solid var(--line)' }}>
                  <span style={{ font: '500 10.5px/1.4 var(--mono)', color: svText(r.state) }}>{r.word}</span>
                  <span style={{ font: '400 12px/1.5 var(--sans)', color: 'var(--t2)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.text}</span>
                  <span title="the event this row traces to" style={{ font: '400 10px/1.4 var(--mono)', color: 'var(--t3)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {r.anchor}
                  </span>
                </div>
              ))}
            </div>
          </Card>
        </>
      )}
    </div>
  )
}

/* Keep the mapped-word import honest: the ledger tab keys colours through the
 * same authority mapping every other surface uses. */
export { SLUG_FOR_WORD, chipsFromRow }
