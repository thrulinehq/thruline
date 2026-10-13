/* Graph-hero derivation (RA5, design v2) — one project as session lanes.
 *
 * The design's canvas holds hand-placed mock nodes; this file replaces the
 * hand with a rule so a real fixture lands in the same picture. Everything is
 * a deterministic function of the export's own order: lanes are the sessions
 * as exported, columns walk each session's nodes in recorded order, and every
 * edge on screen exists in the fixture's `edges` — lineage the engine derived,
 * never geometry invented here (A7: the engine emits facts and no geometry;
 * the layout is presentation and carries no information of its own).
 *
 * Shadow classes never arrive: the node list comes through surfaceProjection,
 * the same gate every other screen reads through. */

import { normalizeNode, surfaceProjection } from './fixture.js'
import { countPhrase } from './voice.js'

export const NW = 186
export const NH = 44
const COLS = 5
const COL_PITCH = 220
const ROW_PITCH = 78
const LANE_HEAD = 42
const LANE_FOOT = 26
const LANE_GAP = 12
const X0 = 150

/** Work actually done — the states NIB's trail may traverse. */
const EATEN = new Set(['supported', 'verified', 'recorded'])

/**
 * The whole picture for one project fixture.
 *
 * @param {object} raw  the §9 project document, exactly as exported
 * @returns {{bands, nodes, edges, trails, size, counts}}
 */
export function layout(raw) {
  const data = surfaceProjection(raw)
  const contractVersion = Number(data.contract_version || 1)
  const all = (data.nodes || []).map((n) => normalizeNode(n, contractVersion))
  const sessions = data.sessions || []
  const orderOf = new Map(sessions.map((s, i) => [s.id, i]))

  const bySession = new Map(sessions.map((s) => [s.id, []]))
  /* A node whose session the export does not list still renders — in a lane of
     its own at the end, labeled from the id. Dropping it would be a silent
     omission (Rule 12). */
  const orphanIds = []
  for (const n of all) {
    if (bySession.has(n.session_id)) bySession.get(n.session_id).push(n)
    else orphanIds.push(n.session_id)
  }
  const lanes = sessions.map((s) => ({ session: s, own: bySession.get(s.id) }))
  for (const id of [...new Set(orphanIds)]) {
    lanes.push({
      session: { id, label: 'Unlisted run', date: '', msgs: null },
      own: all.filter((n) => n.session_id === id),
    })
  }

  const bands = []
  const nodes = []
  const trails = []
  let top = 0
  for (const { session, own } of lanes) {
    const rows = Math.max(1, Math.ceil(own.length / COLS))
    const h = LANE_HEAD + rows * ROW_PITCH + LANE_FOOT
    const parts = [session.label, session.date].filter(Boolean)
    if (session.msgs != null) parts.push(countPhrase(session.msgs, 'msg', 'msgs'))
    bands.push({
      id: session.id,
      label: parts.join(' · '),
      top,
      h,
      count: own.length,
    })
    const placed = own.map((n, i) => ({
      ...n,
      x: X0 + (i % COLS) * COL_PITCH,
      y: top + LANE_HEAD + Math.floor(i / COLS) * ROW_PITCH,
    }))
    nodes.push(...placed)

    /* NIB's trail: this lane's eaten nodes, in recorded order. A lane with
       fewer than two eaten nodes has no path to walk — no trail, no sprite. */
    const eaten = placed.filter((n) => EATEN.has(n.state))
    if (eaten.length >= 2) {
      trails.push({
        session: session.id,
        points: eaten.map((n) => ({ x: n.x + NW / 2, y: n.y + NH / 2 })),
      })
    }
    top += h + LANE_GAP
  }

  const byId = new Map(nodes.map((n) => [n.id, n]))
  /* Only edges the export derived. `kind` travels with each one — that is the
     provenance the tooltip shows. An edge whose ends were filtered (shadow) or
     unknown is skipped and counted, never guessed at. */
  let edgesSkipped = 0
  const edges = []
  for (const e of data.edges || []) {
    const a = byId.get(e.from)
    const b = byId.get(e.to)
    if (!a || !b) {
      edgesSkipped++
      continue
    }
    const crossSession = a.session_id !== b.session_id
    const x1 = a.x + NW / 2
    const y1 = a.y + NH
    const x2 = b.x + NW / 2
    const y2 = b.y
    const dy = Math.max(26, (y2 - y1) * 0.5)
    edges.push({
      from: e.from,
      to: e.to,
      kind: e.kind || 'seq',
      crossSession,
      d: `M${x1},${y1} C${x1},${y1 + dy} ${x2},${y2 - dy} ${x2},${y2}`,
    })
  }

  const counts = {}
  for (const n of nodes) counts[n.state] = (counts[n.state] || 0) + 1

  const width = X0 + COLS * COL_PITCH
  return {
    bands,
    nodes,
    edges,
    edgesSkipped,
    trails,
    counts,
    size: { w: Math.max(width, 900), h: Math.max(top, 300) },
  }
}

/** Upstream lineage of a node, following exported edges only. */
export function ancestors(edges, id) {
  const out = new Set()
  const stack = [id]
  while (stack.length) {
    const cur = stack.pop()
    for (const e of edges) {
      if (e.to === cur && !out.has(e.from)) {
        out.add(e.from)
        stack.push(e.from)
      }
    }
  }
  return out
}
