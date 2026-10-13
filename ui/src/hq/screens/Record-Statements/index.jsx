/* The read-only destination for Record over time's statement KPIs.
 * It reads the same per-project summaries as Record, then names each counted
 * receipt and its exact statement in its Change. The summary has no sentence
 * text, so this list does not invent it. */
import React, { useEffect, useState } from 'react'
import { API, readJson, useResource } from '../../data/api.js'
import { fmtCount } from '../../data/count.js'
import { href } from '../../routes.js'
import { FrameState, Reading, Failed } from '../../frame-state.jsx'
import { readStatementDocuments, statementList } from './model.js'

const PAGE_SIZE = 100

function useDocuments(projects) {
  const ids = projects ? projects.map((p) => p.id).join('\0') : null
  const [state, setState] = useState({ ids: null, docs: null, error: null })
  useEffect(() => {
    if (!projects) return undefined
    const controller = new AbortController()
    readStatementDocuments(projects, readJson, controller.signal).then(
      (docs) => {
        if (!controller.signal.aborted) setState({ ids, docs, error: null })
      },
      (error) => {
        if (!controller.signal.aborted) setState({ ids, docs: null, error })
      },
    )
    return () => controller.abort()
  }, [ids]) // eslint-disable-line react-hooks/exhaustive-deps
  return state.ids === ids ? state : { ids, docs: null, error: null }
}

const subdued = { color: '#A1A1AA' }

/** Pure markup seam for the route and the post-click product test. The frame (top bar, sections, privacy line) is the app's own. */
export function Page({ view, at, shown = PAGE_SIZE, onMore }) {
  const back = href('record', { project: at?.project })
  const v = view
  return <FrameState page>
    <div style={{ maxWidth: '980px', margin: '0 auto' }}>
      <h1 style={{ margin: '0 0 8px', fontSize: '26px', fontWeight: 500 }}>{v.title}</h1>
      <p style={{ ...subdued, margin: '0 0 18px' }}>Last {v.range || at?.range || '30'} days · {v.agent || 'Both agents'} · {v.scope || 'All projects'}</p>
      {v.served ? <>
        <p role="status" style={{ margin: '0 0 8px', fontSize: '18px' }}>{fmtCount(v.count)} {v.count === 1 ? 'statement' : 'statements'}</p>
        <p style={{ ...subdued, margin: '0 0 22px', lineHeight: 1.5 }}>This summary names recorded statements by receipt, project, agent, time and verdict. Open a statement in its Change to read its words and proof.</p>
        {v.count === 0 ? <p>No statements in this selection.</p> : <ol style={{ listStyle: 'none', display: 'grid', gap: '8px', margin: 0, padding: 0 }}>
          {v.rows.slice(0, shown).map((row, i) => <li key={`${row.id}:${i}`} style={{ padding: '12px 14px', border: '1px solid #2A2A30', borderRadius: '7px', background: '#0A0A0B' }}>
            <div style={{ display: 'flex', gap: '10px', alignItems: 'baseline', flexWrap: 'wrap' }}><strong style={{ fontWeight: 500 }}>Statement {i + 1}</strong><span style={subdued}>{row.project} · {row.agent} · {row.at} · {row.verdict.replaceAll('_', ' ')}</span></div>
            <code title={row.id} style={{ display: 'block', marginTop: '5px', fontSize: '12px', color: '#82828C', overflowWrap: 'anywhere' }}>{row.id}</code>
            {row.href ? <a href={row.href} style={{ display: 'inline-block', marginTop: '8px' }}>Open this statement in its Change ›</a> : <span style={{ ...subdued, display: 'block', marginTop: '8px' }}>{row.reason}</span>}
          </li>)}
        </ol>}
        {shown < v.count ? <button type="button" onClick={onMore} style={{ marginTop: '16px', padding: '8px 12px', background: '#16161A', border: '1px solid #66666F', borderRadius: '6px', color: '#EDEDEF', cursor: 'pointer' }}>Show {fmtCount(Math.min(PAGE_SIZE, v.count - shown))} more</button> : null}
      </> : <p role={v.notFound ? 'alert' : 'status'}>{v.reason}</p>}
      <p style={{ marginTop: '28px' }}><a href={back}>Back to Record over time ›</a></p>
    </div>
  </FrameState>
}

export default function Screen({ at }) {
  const projects = useResource(API.projects)
  const all = projects.data?.projects || null
  const selected = all && (at?.scope && at.scope !== 'all' ? all.filter((p) => p.id === at.scope) : all)
  const docs = useDocuments(selected)
  const [shown, setShown] = useState(PAGE_SIZE)
  useEffect(() => setShown(PAGE_SIZE), [at?.range, at?.agent, at?.scope, at?.subset])
  if (projects.error && !all) return <Failed what="The project list could not be read just now. Reload this page to try again." />
  if (all && docs.error) return <Failed what="The statements could not be read just now. Reload this page to try again." />
  if (!all || !docs.docs) return <Reading />
  return <Page view={statementList({ projects: all, documents: docs.docs, at, now: Date.now() })} at={at} shown={shown} onMore={() => setShown((n) => n + PAGE_SIZE)} />
}
