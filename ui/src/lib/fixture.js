import { captureProjectView, captureReceiptView } from './capture.js'

/* Turns a bare §9 document into everything the view needs.
 *
 * The engine emits facts only — no geometry (A7). Layout, band placement and
 * the presentational extras the M0 demo fixture happens to ship in `_ui` are
 * computed here when they are absent, so the app renders identically whether
 * `_ui` is present or not. */

/** Above this many nodes the view opens on the most recent sessions only.
 *  Real projects reach thousands of nodes; 2,500 absolutely-positioned nodes
 *  with per-node transitions is not something a browser should be asked to
 *  paint on first load. */
export const NODE_BUDGET = 400
/** The classes still in shadow. A class graduates by leaving this set — which
 *  is the single gate every shadow-aware surface asks (see `board.graduated`). */
export const SHADOW_STATES = new Set(['unsupported', 'dropped'])
const LIFECYCLE_STATES = new Set(['proposed', 'accepted', 'active', 'completed', 'abandoned'])
const EVIDENCE_STATES = new Set(['unknown', 'observed', 'supported', 'verified', 'contradicted', 'stale'])
const CURRENCY_STATES = new Set(['current', 'superseded', 'out_of_scope'])

const NODE_W = 186
const NODE_H = 44
const COL_W = 220
const COL_X0 = 150
const ROW_H = 78
/** A real session's parent chain is usually strictly linear, which lays out as
 *  one endless column. Wrapping keeps the band readable without reordering
 *  anything: reading order is still top-to-bottom, then next column. */
const ROWS_PER_COL = 12
const BAND_PAD_TOP = 40
const BAND_PAD_BOTTOM = 26

/** Longest-path depth within a session — deterministic, cycle-safe. */
function depths(nodeIds, edges) {
  const inSet = new Set(nodeIds)
  const parents = new Map(nodeIds.map((id) => [id, []]))
  for (const e of edges) {
    if (inSet.has(e.from) && inSet.has(e.to)) parents.get(e.to).push(e.from)
  }
  const depth = new Map()
  const visiting = new Set()
  const walk = (id) => {
    if (depth.has(id)) return depth.get(id)
    if (visiting.has(id)) return 0 // defensive: a cycle must not hang the UI
    visiting.add(id)
    let d = 0
    for (const p of parents.get(id) || []) d = Math.max(d, walk(p) + 1)
    visiting.delete(id)
    depth.set(id, d)
    return d
  }
  // Iterate the given order, never a Set/Map insertion accident.
  for (const id of nodeIds) walk(id)
  return depth
}

/** Layered DAG layout: session bands are layers, depth drives rows. */
export function computeLayout(nodes, edges, sessions) {
  const positions = {}
  const bands = {}
  let y = 0
  let bandWidest = 0

  for (const s of sessions) {
    const ids = nodes.filter((n) => n.session_id === s.id).map((n) => n.id)
    const depth = depths(ids, edges)

    // Group by depth, preserving the export's canonical node order inside each
    // row so two runs place nodes identically.
    const rows = new Map()
    for (const id of ids) {
      const d = depth.get(id) ?? 0
      if (!rows.has(d)) rows.set(d, [])
      rows.get(d).push(id)
    }
    const rowKeys = [...rows.keys()].sort((a, b) => a - b)

    // Flatten depth-major, then wrap into columns. Siblings at one depth sit
    // side by side; a long linear chain snakes instead of running off-screen.
    const placed = []
    for (const k of rowKeys) for (const id of rows.get(k)) placed.push([id, k])

    let row = 0
    let maxRow = 0
    let lastDepth = null
    let widest = 0
    for (let i = 0; i < placed.length; i++) {
      const [id, d] = placed[i]
      if (lastDepth !== null && d !== lastDepth) row++
      lastDepth = d
      const col = Math.floor(row / ROWS_PER_COL)
      const r = row % ROWS_PER_COL
      widest = Math.max(widest, col)
      maxRow = Math.max(maxRow, r)
      positions[id] = [COL_X0 + col * COL_W, y + BAND_PAD_TOP + r * ROW_H]
    }
    bandWidest = Math.max(bandWidest, widest)
    const height = Math.max(ROW_H, BAND_PAD_TOP + (maxRow + 1) * ROW_H + BAND_PAD_BOTTOM)
    bands[s.id] = { top: y, h: height }
    y += height + 12
  }
  return {
    positions,
    bands,
    // No authored edge geometry outside the demo fixture; every edge is routed
    // from its endpoints.
    edge_paths: {},
    canvas: { w: COL_X0 + (bandWidest + 1) * COL_W + NODE_W, h: Math.max(y, 200) },
    node: { w: NODE_W, h: NODE_H },
  }
}

/** Derive counts from nodes — the honesty check the brief asks for. */
export function countsFromNodes(nodes) {
  const out = { recorded: 0, supported: 0, pending: 0, contradicted: 0, refused: 0, unknown: 0 }
  for (const n of nodes) out[n.state] = (out[n.state] || 0) + 1
  return out
}

const PUBLIC_COUNT_KEYS = ['contradicted', 'pending', 'recorded', 'refused', 'supported', 'unknown']
const DIAGNOSTIC_COUNT_KEYS = [...PUBLIC_COUNT_KEYS, 'dropped', 'unsupported'].sort()
const COUNT_REPORT_LIMIT = 12
const COUNT_KEY_REPORT_LIMIT = 64

const hasOwn = (value, key) => Object.prototype.hasOwnProperty.call(value, key)
const reportCount = (value) => Number.isSafeInteger(value) && value >= 0 ? value : null
const reportKey = (value) => {
  const codePoints = [...String(value)]
  return codePoints.length <= COUNT_KEY_REPORT_LIMIT
    ? codePoints.join('')
    : `${codePoints.slice(0, COUNT_KEY_REPORT_LIMIT - 1).join('')}…`
}

/**
 * Recompute the export's flat count map from its nodes without changing the
 * fixture. Public exports exclude full-shadow rows; diagnostic exports count
 * their explicit `sort_key`, even when the dimensional presentation is calmer.
 */
function exportedCountsFromNodes(raw) {
  const diagnostic = raw?.diagnostics === true
  const contractVersion = Number(raw?.contract_version || 1)
  const normalized = (raw?.nodes || []).map((node) => normalizeNode(node, contractVersion))
  const visible = diagnostic
    ? normalized
    : normalized.filter((node) => !SHADOW_STATES.has(node.sort_key))
  const domain = diagnostic ? DIAGNOSTIC_COUNT_KEYS : PUBLIC_COUNT_KEYS
  // A fixture key is data, never an Object prototype operation. In
  // particular, `__proto__` on a normal object invokes legacy prototype
  // semantics instead of creating an own count key. Use a null-prototype
  // accumulator and consult only explicit own keys so every exported sort key
  // participates in the mismatch check fail-loudly.
  const counts = Object.create(null)
  for (const key of domain) counts[key] = 0
  for (const node of visible) {
    const key = node.sort_key
    counts[key] = (hasOwn(counts, key) ? counts[key] : 0) + 1
  }
  return counts
}

/**
 * Exact, presentation-only honesty check for `counts` versus exported nodes.
 * `missing` means a derived key is absent from the export, `extra` means the
 * export names a key outside the applicable public/diagnostic domain, and
 * `unequal` means both sides name the key but disagree on its count.
 */
export function exportedCountDiscrepancy(raw) {
  const exported = raw?.counts && typeof raw.counts === 'object' && !Array.isArray(raw.counts)
    ? raw.counts
    : {}
  const derived = exportedCountsFromNodes(raw)
  const exportedKeys = Object.keys(exported).sort()
  const derivedKeys = Object.keys(derived).sort()
  const missing = derivedKeys
    .filter((key) => !hasOwn(exported, key))
    .map((key) => ({ key, nodes: derived[key] }))
  const extra = exportedKeys
    .filter((key) => !hasOwn(derived, key))
    .map((key) => ({ key, exported: reportCount(exported[key]) }))
  const unequal = derivedKeys
    .filter((key) => hasOwn(exported, key) && exported[key] !== derived[key])
    .map((key) => ({ key, exported: reportCount(exported[key]), nodes: derived[key] }))
  return missing.length || extra.length || unequal.length ? { missing, extra, unequal } : null
}

/** Attach a discrepancy to the in-memory fixture without altering source truth. */
export function attachExportedCountDiscrepancy(raw) {
  const { _count_discrepancy: _stale, ...fixture } = raw || {}
  const discrepancy = exportedCountDiscrepancy(fixture)
  return discrepancy ? { ...fixture, _count_discrepancy: discrepancy } : fixture
}

/**
 * Keep console/DOM evidence bounded even if a malformed local fixture carries
 * an arbitrarily large count map. The exact discrepancy remains attached.
 */
export function boundedCountDiscrepancy(discrepancy) {
  if (!discrepancy) return null
  let remaining = COUNT_REPORT_LIMIT
  const bounded = { missing: [], extra: [], unequal: [] }
  for (const kind of ['missing', 'extra', 'unequal']) {
    const rows = Array.isArray(discrepancy[kind]) ? discrepancy[kind] : []
    bounded[kind] = rows.slice(0, remaining).map((row) => ({ ...row, key: reportKey(row.key) }))
    remaining -= bounded[kind].length
  }
  const total = ['missing', 'extra', 'unequal']
    .reduce((sum, kind) => sum + (Array.isArray(discrepancy[kind]) ? discrepancy[kind].length : 0), 0)
  bounded.omitted = total - (COUNT_REPORT_LIMIT - remaining)
  return bounded
}

/**
 * M4's dimensions are authoritative for presentation. `sort_key` survives as
 * diagnostic metadata, but it can never repaint a node: doing so would quietly
 * restore the overloaded flat state M4.1 removes.
 *
 * A v1 node has no `dimensions`, so its old `state` remains the compatibility
 * input. A v2 node with missing or unknown dimension values fails closed to
 * `unknown`; its legacy state is deliberately not consulted.
 */
export function presentationState(node, contractVersion = 1) {
  const d = node?.dimensions
  if (!d) return contractVersion >= 2 ? 'unknown' : (node?.state || 'unknown')
  if (
    !LIFECYCLE_STATES.has(d.lifecycle) ||
    !EVIDENCE_STATES.has(d.evidence) ||
    !CURRENCY_STATES.has(d.currency)
  ) return 'unknown'

  if (d.evidence === 'contradicted') return 'contradicted'
  if (d.evidence === 'supported' || d.evidence === 'verified') return 'supported'
  if (d.evidence === 'observed') return 'recorded'
  if (d.lifecycle === 'proposed' || d.lifecycle === 'accepted' || d.lifecycle === 'active') {
    return 'pending'
  }
  // `stale` has no dedicated M4 badge yet. Until M6 gives currency/freshness
  // its own visual treatment, the existing honest presentation is Unknown.
  return 'unknown'
}

export function normalizeNode(node, contractVersion = 1) {
  const state = presentationState(node, contractVersion)
  return {
    ...node,
    // v1 fixtures had only `state`; retaining it as the diagnostic key keeps
    // the committed demo and pre-M4 local exports inspectable.
    sort_key: node.sort_key || node.state || state,
    state,
  }
}

function sortCountsFromNodes(nodes) {
  const out = {}
  for (const n of nodes) out[n.sort_key] = (out[n.sort_key] || 0) + 1
  return out
}

/** §5s defence in depth for old/local fixtures. Diagnostic exports opt in. */
export function surfaceProjection(raw) {
  const contractVersion = Number(raw.contract_version || 1)
  const normalized = (raw.nodes || []).map((n) => normalizeNode(n, contractVersion))
  const diagnostic = raw.diagnostics === true
  // Shadow membership follows the explicit diagnostic key, not the badge
  // projection. A v2 `unsupported` node presents as Unknown but must still be
  // absent from the ordinary product surface.
  const nodes = diagnostic
    ? normalized
    : normalized.filter((n) => !SHADOW_STATES.has(n.sort_key))
  const ids = new Set(nodes.map((n) => n.id))
  const edges = diagnostic
    ? (raw.edges || [])
    : (raw.edges || []).filter((e) => ids.has(e.from) && ids.has(e.to))
  const receipts = diagnostic
    ? (raw.receipts || {})
    : Object.fromEntries(Object.entries(raw.receipts || {}).filter(([id]) => ids.has(id)))
  const counts = countsFromNodes(nodes)
  delete counts.unsupported
  delete counts.dropped
  const ui = raw._ui && !diagnostic
    ? {
        ...raw._ui,
        projects: (raw._ui.projects || []).map((p) => ({ ...p, unsupported: 0, dropped: 0 })),
      }
    : raw._ui
  return {
    ...raw,
    diagnostics: diagnostic,
    nodes,
    edges,
    receipts,
    counts,
    accusations: diagnostic
      ? nodes.filter((n) => n.sort_key === 'contradicted' || n.sort_key === 'dropped').length
      : nodes.filter((n) => n.state === 'contradicted').length,
    ...(ui ? { _ui: ui } : {}),
  }
}

function shortHash(s) {
  if (!s) return '—'
  const t = String(s)
  return t.length > 10 ? `${t.slice(0, 4)}…${t.slice(-4)}` : t
}

/**
 * Fill in everything `_ui` would have carried. Returns the fixture plus a
 * `_view` block; the original `_ui` (demo fixture only) wins where present,
 * so M0's rendering is untouched.
 */
export function prepare(raw, { all = false } = {}) {
  const data = surfaceProjection(raw)
  const hadUi = !!data._ui
  const ui = data._ui || {}

  // Session subsetting for large projects — most recent first.
  const allSessions = data.sessions || []
  const overBudget = (data.nodes || []).length > NODE_BUDGET
  let visibleSessions = allSessions
  if (overBudget && !hadUi && !all) {
    const keep = []
    let budget = NODE_BUDGET
    for (let i = allSessions.length - 1; i >= 0 && budget > 0; i--) {
      keep.unshift(allSessions[i])
      budget -= (data.nodes || []).filter((n) => n.session_id === allSessions[i].id).length
    }
    visibleSessions = keep.length ? keep : allSessions.slice(-1)
  }

  const visibleIds = new Set(visibleSessions.map((s) => s.id))
  const nodes = (data.nodes || []).filter((n) => visibleIds.has(n.session_id))
  const nodeIds = new Set(nodes.map((n) => n.id))
  const edges = (data.edges || []).filter((e) => nodeIds.has(e.from) && nodeIds.has(e.to))

  const layout = ui.layout || computeLayout(nodes, edges, visibleSessions)

  // Presentational extras. For a real export these are derived from §9 itself.
  const receipts = {}
  for (const n of nodes) {
    const fromUi = ui.receipts?.[n.id]
    if (fromUi) {
      receipts[n.id] = fromUi
      continue
    }
    const r = data.receipts?.[n.id]
    const freshnessNotes = [...new Set(
      (r?.claim?.claims || [])
        .flatMap((claim) => claim.evidence_scopes || [])
        .filter((scope) => scope.freshness === 'stale')
        .map((scope) => scope.freshness_note)
        .filter(Boolean),
    )]
    receipts[n.id] = {
      // No highlight span in a real export — the whole claim is the claim.
      highlight: [0, 0],
      hash: shortHash(n.anchor_event_id),
      empty_text:
        n.state === 'pending'
          ? 'Proposed in this session; no file edit or command recorded in the same turn.'
          : 'Nothing in the record ties a command or file edit to this statement.',
      evidence_stats: (r?.evidence || []).map(() => ''),
      // Candidate proof history is deliberately separate from the deciding
      // verdict line: a stale nearby run is context, not automatically the
      // evidence that governed this claim.
      freshness_notes: freshnessNotes,
      capture: captureReceiptView(data.capture_receipts?.[n.anchor_event_id]),
    }
  }

  const projects =
    ui.projects ||
    [
      {
        name: data.project?.name || data.project?.id || 'project',
        source: data.project?.source === 'codex' ? 'Codex' : 'Claude Code',
        sessions: allSessions.length,
        when: allSessions[allSessions.length - 1]?.date || '',
        rank: 1,
        ...countsFromNodes(data.nodes || []),
      },
    ]

  /* The demo fixture's vocabulary calls `done` "Verified". At the deterministic
     tier nothing has been entailment-checked, so a real export must not borrow
     that word — the receipt would overclaim in exactly the way this project
     exists to catch. M3 earns "Verified". */
  const receiptStateLabels = hadUi
    ? null
    : {
        supported: 'Action recorded',
        pending: 'Proposed',
        unknown: 'Unclassified',
        contradicted: 'Claimed, not verified',
        dropped: 'Thread dropped',
        refused: 'Refused',
      }

  data._view = {
    hadUi,
    // App/UI consumers use these presentation counts. Diagnostic callers can
    // separately inspect `sort_counts` without letting a shadow key become a
    // user-facing badge total.
    counts: countsFromNodes(data.nodes || []),
    sort_counts: sortCountsFromNodes(data.nodes || []),
    receiptStateLabels,
    layout,
    receipts,
    projects,
    nodes,
    edges,
    sessions: visibleSessions,
    hiddenSessions: allSessions.length - visibleSessions.length,
    hiddenNodes: (data.nodes || []).length - nodes.length,
    default_selected: ui.default_selected || nodes[0]?.id || null,
    default_model: ui.default_model || 'deterministic',
    models: ui.models || ['deterministic'],
    annotating: ui.annotating || { start: 0, total: Math.max(1, nodes.length), shim_nodes: [] },
    capture: captureProjectView(data),
  }
  return data
}
