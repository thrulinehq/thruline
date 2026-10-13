/* A read-only list of the exact claims counted by Record over time.
 * The summary route has receipt ids and verdicts, but no sentence text. Each
 * linked row opens its own statement in the Change Brief. */
import { href } from '../../routes.js'
import { recordFromStatements } from '../Record-Over-Time/from-projects.js'
import { sliceRows } from '../Record-Over-Time/metrics.js'

export const statementsAddress = (id) => `/api/hq/projects/${encodeURIComponent(id)}/statements`

/** Read the existing summaries, once per selected project. No write address. */
export async function readStatementDocuments(projects, get, signal) {
  const pairs = await Promise.all(projects.map(async (p) => [p.id, await get(statementsAddress(p.id), { signal })]))
  return Object.fromEntries(pairs)
}

const SUBSET = Object.freeze({
  all: 'Statements the agents made',
  proven: 'Proven on the version they were about',
  moved: 'Needed a re-check after a later edit',
  open: 'Need a re-check now',
  recorded: 'Recorded and settles nothing',
})
const AGENT = Object.freeze({ all: 'Both agents', cc: 'Claude Code', cx: 'Codex' })

/** The visible rows use Record's own counted rows and its range/agent/project slicer. */
export function statementList({ projects, documents, at, now }) {
  const scope = at?.scope || 'all'
  const selected = scope === 'all' ? projects : projects.filter((p) => p.id === scope)
  const base = {
    title: SUBSET[at?.subset] || SUBSET.all,
    range: at?.range || '30',
    agent: AGENT[at?.agent] || AGENT.all,
    scope: scope === 'all' ? 'All projects' : selected[0]?.name || scope,
    rows: [], count: null, served: false,
  }
  if (scope !== 'all' && !selected.length) return { ...base, notFound: true, reason: 'This project is not in the local record.' }
  if (selected.some((p) => !Array.isArray(documents?.[p.id]?.claims))) {
    return { ...base, reason: 'The statements could not be read just now. Reload this page to try again.' }
  }
  const data = recordFromStatements({ projects: selected, statements: documents, storage: null, now })
  const rows = sliceRows(data.rows, { range: at?.range || '30', agent: at?.agent || 'all', project: scope })
  const subset = at?.subset || 'all'
  if (subset === 'moved' && !rows.every((r) => typeof r.c.proof_moved === 'boolean')) {
    return { ...base, reason: 'Not counted. This record does not say whether a later edit moved every statement’s proof.' }
  }
  const matching = rows.filter((r) => {
    if (subset === 'proven') return r.c.verdict === 'proven'
    if (subset === 'moved') return r.c.proof_moved === true
    if (subset === 'open') return r.key === 'needsRecheck'
    if (subset === 'recorded') return r.key === 'recordedStopsAtSo'
    return true
  }).sort((a, b) => String(b.c.ts).localeCompare(String(a.c.ts)))
  const names = Object.fromEntries(data.projects.map((p) => [p.id, p.name]))
  return {
    ...base,
    served: true,
    count: matching.length,
    rows: matching.map(({ pid, a, c }) => ({
      id: c.id,
      project: names[pid] || pid,
      agent: AGENT[a],
      at: c.ts,
      verdict: c.verdict,
      href: c.thread_id ? href('brief', { change: c.thread_id, project: pid, statement: c.id }) : undefined,
      reason: c.thread_id ? undefined : 'No Change was recorded for this statement.',
    })),
  }
}
