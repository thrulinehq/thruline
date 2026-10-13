/* The Trace, turned up (P6-3; design record §3.3, §4, §5).
 *
 * PURE. No React, no DOM, no `window`, no state. Every function here is a
 * derivation over the Session shape `adapter.js` already produces — its
 * `nodes`, its record wires and its typed fields — and nothing here changes
 * that shape. `shell.jsx` and `canvas.jsx` draw what these functions return;
 * `trace-view.test.js` walks every one of them.
 *
 * WHAT IT DERIVES.
 *
 *  - `laneOf`       — which of the four lanes (Request · Work · Execution ·
 *                     Claims & evidence) a card sits in, from its object type
 *                     or event kind. Never from its title.
 *  - `laneLayout`   — cards laid into those four horizontal lanes, left to
 *                     right in record order. Position is a view; it is never
 *                     stored on a node.
 *  - `edgeKindOf`   — which of the three line kinds (recorded order ·
 *                     documented reference · inferred) a wire is, from the
 *                     wire's own `kind`. A kind this view does not know is its
 *                     OWN fourth entry, never folded into "inferred".
 *  - `threadOf`     — the single causal thread of one card, over the record's
 *                     own wires: at most one card per stage (request ·
 *                     statement · change · run · finding), each the nearest.
 *  - `walkOrder`    — the guided walk: along the thread, or record order.
 *  - `runFacts`     — where a run ran: command, exit code, counts, duration,
 *                     working directory, version — each only from a typed
 *                     field, every missing one named in words.
 *  - `plainFace`    — the card in Plain words: no `seq`, no ids, no tree
 *                     hashes, no session ids.
 *  - `outlineRows`  — the Outline twin: the same cards and the same thread
 *                     as a list.
 *  - `outlineTree`  — the same rows with every card folded inside a drawn
 *                     group listed under it, and `laneCounts`, the four lanes
 *                     with what each holds (T-6-4).
 *
 * WHAT IT MAY SAY. Every word is `HQ_COPY.trace`; numbers and versions are
 * typed record fields filled into those templates.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { C } from './tokens.js'
import { estH, objType, WORST_FIRST } from './logic.js'
import { placementSeq } from './lod.js'

const T = HQ_COPY.trace

/* ── lanes ─────────────────────────────────────────────────────────────── */

/** The four lanes, top to bottom. */
export const LANES = Object.freeze(['request', 'work', 'execution', 'claims'])

/* By object type first — a derived object has no event kind. */
export const LANE_BY_OBJECT = Object.freeze({
  ask: 'request',
  check: 'execution',
  claim: 'claims',
  promise: 'claims',
  finding: 'claims',
  receipt: 'claims',
  ending: 'claims',
})

/* Then by event kind. `tool_result` is the answer to a `tool_call` and sits
 * beside it in Execution (the old column layout put it under Changes). `note`
 * is the agent working, so Work. `stop` is the agent handing back what it
 * says it did, so Claims & evidence. */
export const LANE_BY_KIND = Object.freeze({
  session_start: 'request',
  user_prompt: 'request',
  fs_change: 'work',
  snapshot: 'work',
  note: 'work',
  tool_call: 'execution',
  tool_result: 'execution',
  exec: 'execution',
  stop: 'claims',
})

/* A Focus group card stands for the category it merged (`focus-set.js`),
 * unless every member it holds sits in one lane (then it sits there). */
export const LANE_BY_GROUP = Object.freeze({
  holds: 'execution',
  fs: 'work',
  trees: 'work',
  record: 'claims',
  other: 'work',
  all: 'work',
})

/* Last before the fallback: the column a record authored for the card itself
 * (the bundled demo records place their cards and carry no event kind). */
export const LANE_BY_FRAME = Object.freeze({
  request: 'request',
  changes: 'work',
  execution: 'execution',
  claims: 'claims',
})

/** Where a card of no known type or kind goes: a recorded step of the work. */
export const FALLBACK_LANE = 'work'

/**
 * The lane a card sits in.
 *
 * @param {object} n a graph node
 * @returns {'request'|'work'|'execution'|'claims'}
 */
export function laneOf(n) {
  return laneBasis(n).lane
}

/**
 * The lane and the rule that put it there — `object`, `kind`, `group`,
 * `frame` or `fallback` — so a card that landed in the fallback lane can say so.
 *
 * @param {object} n @returns {{lane: string, basis: string}}
 */
export function laneBasis(n) {
  if (!n) return { lane: FALLBACK_LANE, basis: 'fallback' }
  const t = objType(n)
  if (t === 'group') {
    /* A Focus group whose members all sit in one lane sits there too; a mixed
     * one sits where its category does. */
    const lanes = new Set((n.memberKinds || []).map((k) => LANE_BY_KIND[k] || null))
    if (lanes.size === 1 && !lanes.has(null)) return { lane: [...lanes][0], basis: 'group' }
    const g = LANE_BY_GROUP[n.kind]
    return g ? { lane: g, basis: 'group' } : { lane: FALLBACK_LANE, basis: 'fallback' }
  }
  if (LANE_BY_OBJECT[t]) return { lane: LANE_BY_OBJECT[t], basis: 'object' }
  if (n.kind && LANE_BY_KIND[n.kind]) return { lane: LANE_BY_KIND[n.kind], basis: 'kind' }
  if (n.frame && LANE_BY_FRAME[n.frame]) return { lane: LANE_BY_FRAME[n.frame], basis: 'frame' }
  return { lane: FALLBACK_LANE, basis: 'fallback' }
}

/** A lane's label, from the copy. */
export const laneLabel = (lane) => T.lanes[lane] || T.lanes[FALLBACK_LANE]

/* The lane geometry, stated once. */
export const LANE_METRICS = Object.freeze({ HEADER: 30, PAD: 18, GAP: 22, LANE_GAP: 16, STEP: 36, LEFT: 20 })

/**
 * Lay cards into the four lanes, left to right in record order.
 *
 * Order is `placementSeq` ascending; a card the record does not place (a
 * derived claim, finding or check with no position) comes after every placed
 * card, in the order the record lists them (`recordOrder`, the same order
 * the guided walk and the Outline read), and the first of them opens a section marked "not placed in
 * record order". X is STRICTLY increasing along that order — each card starts
 * at least `STEP` to the right of the one before it, whatever lane it is in —
 * and a lane never overlaps itself, so reading left to right is reading the
 * record in order. Every lane is drawn, empty or not, so the four lanes are
 * always the same four.
 *
 * @param {object[]} nodes every node (hidden ones get their group's box)
 * @param {{visible?: (n) => boolean, widthOf?: (n) => number, heightOf?: (n, w) => number, lane?: (n) => string}} [opts]
 * @returns {{pos: Map<string,{x:number,y:number,w:number}>, frames: object[], frameH: Map<string,number>,
 *   chapters: object[], lanes: Array<{id:string,label:string,y:number,h:number,n:number}>, unplacedX: number|null}}
 */
export function laneLayout(nodes, opts) {
  const o = opts || {}
  const visible = o.visible || (() => true)
  const widthOf = o.widthOf || ((n) => n.w || 260)
  const heightOf = o.heightOf || ((n, w) => estH(n, w))
  const lane = o.lane || laneOf
  const M = LANE_METRICS
  const list = (nodes || []).filter((n) => n && visible(n))
  const shown = list
    .map((n, i) => ({ n, i, s: placementSeq(n) }))
    .sort((a, b) => {
      if (a.s == null && b.s != null) return 1
      if (a.s != null && b.s == null) return -1
      if (a.s != null && b.s != null && a.s !== b.s) return a.s - b.s
      return a.i - b.i
    })
    .map((e) => e.n)

  const laneEnd = {}
  const laneH = {}
  const count = {}
  LANES.forEach((l) => {
    laneEnd[l] = M.LEFT
    laneH[l] = 0
    count[l] = 0
  })
  const xs = new Map()
  let prevX = -Infinity
  let unplacedX = null
  shown.forEach((n) => {
    const l = LANES.includes(lane(n)) ? lane(n) : FALLBACK_LANE
    const w = widthOf(n)
    const x = Math.max(prevX + M.STEP, laneEnd[l] + (count[l] ? M.GAP : 0))
    if (unplacedX == null && placementSeq(n) == null) unplacedX = x
    xs.set(n.id, { x, w, l })
    prevX = x
    laneEnd[l] = x + w
    count[l] += 1
    laneH[l] = Math.max(laneH[l], heightOf(n, w))
  })
  const right = Math.max(M.LEFT, ...LANES.map((l) => laneEnd[l])) + M.PAD

  const top = {}
  let y = 0
  const lanes = []
  const frames = []
  const frameH = new Map()
  LANES.forEach((l) => {
    const h = M.HEADER + Math.max(laneH[l], 40) + M.PAD
    top[l] = y
    lanes.push({ id: l, label: laneLabel(l), y, h, n: count[l] })
    frames.push({
      id: 'lane-' + l,
      lane: l,
      bare: true,
      title: laneLabel(l),
      count: count[l] ? (count[l] === 1 ? T.lanes.count_one : fill(T.lanes.count_many, { n: count[l] })) : T.lanes.empty,
      n: count[l],
      x: 0,
      y,
      w: right,
      h,
    })
    frameH.set('lane-' + l, h)
    y += h + M.LANE_GAP
  })

  const pos = new Map()
  xs.forEach((v, id) => pos.set(id, { x: v.x, y: top[v.l] + M.HEADER, w: v.w }))
  for (const n of nodes || []) {
    if (!n || pos.has(n.id)) continue
    const parent = n.member ? pos.get(n.member) : null
    pos.set(n.id, parent ? { ...parent, w: widthOf(n) } : { x: n.x || 0, y: n.y || 0, w: widthOf(n) })
  }

  /* Sections for the Workbench "trace section" controls: twelve cards each, in the
   * same order the lanes read, plus where the unplaced cards begin. */
  const chapters = []
  for (let i = 0; i < shown.length; i += 12) {
    const part = shown.slice(i, i + 12)
    const seqs = part.map(placementSeq).filter((s) => s != null)
    const lastSeqs = part.map((n) => (n.lastSeq != null ? n.lastSeq : placementSeq(n))).filter((s) => s != null)
    chapters.push({
      id: 'lanes-part-' + (chapters.length + 1),
      firstNode: part[0].id,
      firstSeq: seqs.length ? seqs[0] : null,
      lastSeq: lastSeqs.length ? lastSeqs[lastSeqs.length - 1] : null,
      alerts: part.filter((n) => n.risk || n.finding || n.fam === 'red').length,
      x: xs.get(part[0].id).x,
    })
  }
  return { pos, frames, frameH, chapters, lanes, unplacedX }
}

/* ── the three line kinds ─────────────────────────────────────────────── */

/** The record's wire `kind` → the legend entry. Nothing else is mapped. */
export const EDGE_BY_KIND = Object.freeze({ order: 'order', ref: 'reference', inf: 'inferred' })

/** The three kinds, and the fourth that is only drawn when the data has one. */
export const EDGE_KINDS = Object.freeze(['order', 'reference', 'inferred', 'unclassified'])

/**
 * The stroke of each line kind. Solid, dashed, dotted — and a dash-dot for a
 * kind this view does not know — so no kind is told apart by colour alone.
 * The lit thread keeps its kind's pattern and takes amber.
 */
export const EDGE_STYLE = Object.freeze({
  order: Object.freeze({ color: C.inkSoft, dash: 'none', cap: 'butt' }),
  reference: Object.freeze({ color: C.ink, dash: '7 4', cap: 'butt' }),
  inferred: Object.freeze({ color: C.violet, dash: '0.5 4.5', cap: 'round' }),
  unclassified: Object.freeze({ color: C.neutral, dash: '9 3 2 3', cap: 'butt' }),
})

/**
 * Which line kind a wire is. Read off the wire's own `kind`; a kind with no
 * row in `EDGE_BY_KIND` is `unclassified` — shown as itself, never as
 * inferred.
 *
 * @param {{kind?: string}} w @returns {'order'|'reference'|'inferred'|'unclassified'}
 */
export function edgeKindOf(w) {
  return (w && EDGE_BY_KIND[w.kind]) || 'unclassified'
}

/** The words for a line kind. */
export const edgeWord = (kind) => T.edges[kind] || T.edges.unclassified

/**
 * The legend: the three kinds always, the fourth only when a drawn wire is one.
 *
 * @param {Array<{kind?: string}>} wires @returns {Array<{kind:string,label:string,hint:string,color:string,dash:string,cap:string}>}
 */
export function legendEntries(wires) {
  const present = new Set((wires || []).map(edgeKindOf))
  return EDGE_KINDS.filter((k) => k !== 'unclassified' || present.has('unclassified')).map((k) => ({
    kind: k,
    label: T.edges[k],
    hint: T.edges[k + '_hint'],
    color: EDGE_STYLE[k].color,
    dash: EDGE_STYLE[k].dash,
    cap: EDGE_STYLE[k].cap,
  }))
}

/* ── the lit thread ───────────────────────────────────────────────────── */

/** The thread's stages, in the order the thread is read. */
export const THREAD_STAGES = Object.freeze(['request', 'statement', 'change', 'run', 'finding', 'other'])

/**
 * A card's stage on a thread: request · statement (what the agent said) ·
 * change (what changed) · run (what ran) · finding (what review found), or
 * `other` for a card that is part of the record but none of those (the
 * signed record, the ending, the hand-back).
 *
 * @param {object} n @returns {string}
 */
export function stageOf(n) {
  const t = objType(n)
  if (t === 'claim' || t === 'promise') return 'statement'
  if (t === 'finding') return 'finding'
  if (t === 'receipt' || t === 'ending') return 'other'
  const l = laneOf(n)
  if (l === 'request') return 'request'
  if (l === 'work') return 'change'
  if (l === 'execution') return 'run'
  return 'other'
}

const SEVERITY_RANK = { fail: 0, warn: 1, info: 2 }
const severityRank = (n) => {
  const m = /(fail|warn|info)$/.exec(String((n && n.sub) || ''))
  return m ? SEVERITY_RANK[m[1]] : 3
}

/**
 * The single causal thread of one card, over the record's own wires.
 *
 * 1. Breadth-first from the card over `session.wires` — the record's
 *    documented-reference and inferred links only. Never recorded order
 *    (it joins every card to its neighbour) and never a link of a kind this
 *    view cannot classify (it cannot be presented as cause).
 * 2. The signed record joins every claim and every check; walking THROUGH it
 *    would make every thread the whole session, so it is an end, never a
 *    passage (it can still be the card you selected).
 * 3. Per stage, keep ONE card: the selected card for its own stage, else the
 *    nearest; ties go to the worst finding, then to record order.
 * 4. The thread is those cards plus every card and wire on the paths that
 *    reached them, read in stage order. A stage with no card is named as
 *    missing — "nothing in the record links a request to this" — never
 *    filled.
 *
 * @param {{nodes: object[], wires: object[]}} session
 * @param {string} nodeId
 * @returns {null | {anchor: string, ids: string[], wires: string[], missing: string[],
 *   steps: Array<{id: string, stage: string, lane: string, dist: number,
 *   via: null | {wire: string, kind: string, from: string}}>}}
 *   null when the card is not in the record or is linked to nothing.
 */
export function threadOf(session, nodeId) {
  const nodes = (session && session.nodes) || []
  const wires = (session && session.wires) || []
  const index = new Map()
  nodes.forEach((n, i) => {
    if (n && n.id != null && !index.has(n.id)) index.set(n.id, i)
  })
  if (!index.has(nodeId)) return null
  const node = (id) => nodes[index.get(id)]
  const adj = new Map()
  const push = (a, b, w) => {
    if (!index.has(a) || !index.has(b)) return
    if (!adj.has(a)) adj.set(a, [])
    adj.get(a).push({ to: b, w })
  }
  wires.forEach((w) => {
    if (!w || !w.from || !w.to) return
    /* Only the two kinds that say one card bears on another. Recorded order
     * joins everything to its neighbour, and a link of a kind this view does
     * not know cannot be presented as cause. */
    const k = edgeKindOf(w)
    if (k !== 'reference' && k !== 'inferred') return
    push(w.from[0], w.to[0], w)
    push(w.to[0], w.from[0], w)
  })

  const dist = new Map([[nodeId, 0]])
  const via = new Map([[nodeId, null]])
  const queue = [nodeId]
  while (queue.length) {
    const id = queue.shift()
    if (id !== nodeId && objType(node(id)) === 'receipt') continue
    const next = (adj.get(id) || []).slice().sort((a, b) => index.get(a.to) - index.get(b.to))
    for (const e of next) {
      if (dist.has(e.to)) continue
      dist.set(e.to, dist.get(id) + 1)
      via.set(e.to, { wire: e.w.id, kind: edgeKindOf(e.w), from: id })
      queue.push(e.to)
    }
  }
  if (dist.size < 2) return null

  const selStage = stageOf(node(nodeId))
  const chosen = new Map([[selStage, nodeId]])
  const better = (a, b) => {
    const da = dist.get(a)
    const db = dist.get(b)
    if (da !== db) return da < db
    const sa = severityRank(node(a))
    const sb = severityRank(node(b))
    if (sa !== sb) return sa < sb
    return index.get(a) < index.get(b)
  }
  dist.forEach((_, id) => {
    const st = stageOf(node(id))
    if (st === 'other') return
    if (!chosen.has(st) || (chosen.get(st) !== nodeId && better(id, chosen.get(st)))) chosen.set(st, id)
  })

  const ids = new Set()
  const threadWires = new Set()
  chosen.forEach((id) => {
    let cur = id
    while (cur != null && !ids.has(cur)) {
      ids.add(cur)
      const v = via.get(cur)
      if (!v) break
      threadWires.add(v.wire)
      cur = v.from
    }
  })

  const rank = (id) => THREAD_STAGES.indexOf(stageOf(node(id)))
  const steps = [...ids]
    .sort((a, b) => rank(a) - rank(b) || dist.get(a) - dist.get(b) || index.get(a) - index.get(b))
    .map((id) => ({ id, stage: stageOf(node(id)), lane: laneOf(node(id)), dist: dist.get(id), via: via.get(id) }))
  const have = new Set(steps.map((s) => s.stage))
  const missing = THREAD_STAGES.filter((s) => s !== 'other' && !have.has(s))
  return { anchor: nodeId, ids: steps.map((s) => s.id), wires: [...threadWires], missing, steps }
}

/**
 * The card the current discrepancy is about: the worst claim (by the ruled
 * worst-first order of its word), else the first claim, else null. It is the
 * claim the workspace's one primary action lines up against its check.
 *
 * @param {{nodes: object[]}} session @returns {string|null}
 */
export function discrepancyAnchor(session) {
  const claims = ((session && session.nodes) || []).filter((n) => n && objType(n) === 'claim')
  if (!claims.length) return null
  const rank = (n) => {
    const i = WORST_FIRST.indexOf(n.word)
    return i < 0 ? WORST_FIRST.length : i
  }
  return claims.map((n, i) => ({ n, i })).sort((a, b) => rank(a.n) - rank(b.n) || a.i - b.i)[0].n.id
}

/* ── the guided walk ──────────────────────────────────────────────────── */

/**
 * Every card in record order: placed cards by their place, then the unplaced
 * ones in the order the record lists them.
 *
 * @param {object[]} nodes @returns {string[]}
 */
export function recordOrder(nodes) {
  return (nodes || [])
    .map((n, i) => ({ n, i, s: placementSeq(n) }))
    .filter((e) => e.n && e.n.id != null)
    .sort((a, b) => {
      if (a.s == null && b.s != null) return 1
      if (a.s != null && b.s == null) return -1
      if (a.s != null && b.s != null && a.s !== b.s) return a.s - b.s
      return a.i - b.i
    })
    .map((e) => e.n.id)
}

/**
 * What J and K walk: the thread when there is one, else record order.
 *
 * @param {ReturnType<typeof threadOf>} thread @param {object[]} nodes
 * @returns {{along: 'thread'|'record', ids: string[]}}
 */
export function walkOrder(thread, nodes) {
  if (thread && thread.ids.length > 1) return { along: 'thread', ids: thread.ids.slice() }
  return { along: 'record', ids: recordOrder(nodes) }
}

/**
 * One step of the walk. Never wraps: at an end it stays and says so.
 *
 * @param {string[]} ids @param {string|null} current @param {1|-1} dir
 * @returns {null | {id: string, index: number, total: number, edge: null|'end'|'start'}}
 */
export function walkStep(ids, current, dir) {
  if (!ids || !ids.length) return null
  const i = ids.indexOf(current)
  if (i < 0) return { id: dir < 0 ? ids[ids.length - 1] : ids[0], index: dir < 0 ? ids.length - 1 : 0, total: ids.length, edge: null }
  const j = i + (dir < 0 ? -1 : 1)
  if (j < 0) return { id: ids[0], index: 0, total: ids.length, edge: 'start' }
  if (j >= ids.length) return { id: ids[ids.length - 1], index: ids.length - 1, total: ids.length, edge: 'end' }
  return { id: ids[j], index: j, total: ids.length, edge: null }
}

/* ── the Plain face ───────────────────────────────────────────────────── */

/** "step 46", "steps 12–40", or null when the record gives the card no place. */
export function stepLabel(n) {
  if (!n) return null
  if (n.group && n.firstSeq != null && n.lastSeq != null && n.firstSeq !== n.lastSeq) return fill(T.face.steps, { a: n.firstSeq, b: n.lastSeq })
  const s = placementSeq(n)
  return s == null ? null : fill(T.face.step, { n: s })
}

/** A version as Plain says it: the first seven characters. */
export const shortVersion = (h) => String(h || '').replace(/^[a-z0-9]+:/, '').slice(0, 7)

/* The technical tokens a Plain face never shows, and what it says instead. */
const PLAIN_RULES = [
  [/\bseq (\d+)\s*[–-]\s*(?:seq )?(\d+)/g, (_m, a, b) => fill(T.face.steps, { a, b })],
  [/\bseq (\d+)/g, (_m, a) => fill(T.face.step, { n: a })],
  [/\btree ([0-9a-f]{7,64})\b…?/g, (_m, h) => fill(T.face.version, { version: shortVersion(h) })],
  [/\bsha256:[0-9a-f]*…?/g, () => ''],
  [/\b[0-9a-f]{40,64}\b/g, (h) => shortVersion(h)],
  [/\b(?:clm|chk|prm|ask|evt|ses|rcpt|lnk)_[A-Za-z0-9]+/g, () => ''],
  [/\bsession id\b/gi, () => 'session'],
]

/**
 * A ThruLine-composed line in Plain words. The quoted words of the person and
 * of the agent do NOT come through here: those are quotations, and only a
 * full hash inside one is shortened (`quoteFace`).
 *
 * @param {string} s @returns {string}
 */
export function plainText(s) {
  let out = String(s == null ? '' : s)
  for (const [re, to] of PLAIN_RULES) out = out.replace(re, to)
  return out
    .replace(/\s*·\s*(?=·|$)/g, '')
    .replace(/^\s*·\s*/, '')
    .replace(/\s{2,}/g, ' ')
    .trim()
}

/** A quotation in Plain: kept word for word, with a full hash shortened. */
export const quoteFace = (s) => String(s == null ? '' : s).replace(/\b[0-9a-f]{40,64}\b/g, (h) => shortVersion(h) + '…')

const QUOTED = { claim: true, promise: true, ask: true }

/** The words for what a card is, in Plain. */
export function kindWord(n) {
  const t = objType(n)
  if (t === 'group') return T.kinds.group
  if (t !== 'event' && T.kinds[t]) return T.kinds[t]
  if (n && n.group) return T.kinds.group
  if (n && T.kinds[n.kind]) return T.kinds[n.kind]
  /* A card with no kind this view knows keeps the label its record authored
   * (in Plain words), else says it is a step. */
  const own = n && n.sub ? plainText(n.sub) : ''
  return own || T.kinds.unknown
}

/**
 * The card as the Plain face shows it. Header, title, lines and foot; every
 * ThruLine-composed string goes through `plainText`, every quotation through
 * `quoteFace`, and a finding is named in the detector's plain phrase.
 *
 * @param {object} n a graph node
 * @returns {{type: string, sub: string, title: string, lines: string[], foot: string, hdrRight: string, aria: string}}
 */
export function plainFace(n) {
  const t = objType(n)
  const step = stepLabel(n)
  let title
  let lines
  if (t === 'finding') {
    title = HQ_COPY.detectors[n.finding] || String(n.finding || n.title || '').replace(/_/g, ' ')
    lines = (n.lines || []).map(plainText).filter(Boolean)
  } else if (QUOTED[t] || (t === 'event' && n.kind === 'user_prompt')) {
    title = quoteFace(n.title)
    lines = []
  } else if (t === 'receipt') {
    title = T.face.receipt_title
    lines = []
  } else if (t === 'check') {
    title = plainText(checkTitle(n))
    lines = []
  } else {
    title = plainText(n.title)
    lines = (n.lines || []).map(plainText).filter(Boolean)
  }
  const type = laneLabel(laneOf(n))
  const sub = kindWord(n)
  return {
    type,
    sub,
    title,
    lines,
    foot: step || T.face.not_placed,
    hdrRight: step || '',
    aria: type + ' · ' + sub + ': ' + title,
  }
}

/* ── where a run ran ──────────────────────────────────────────────────── */

const RUN_KEYS = Object.freeze(['command', 'exit', 'counts', 'duration', 'cwd', 'version'])

/* One seq → event index per events array, built once: `runFacts` runs for
 * every card on every layout, and a linear search per card is quadratic on a
 * four-thousand-step record. Keyed weakly on the array, so a new reading gets
 * a new index and an old one is let go. */
const EVENT_INDEX = new WeakMap()
function eventAt(events, seq) {
  if (!Array.isArray(events)) return null
  let ix = EVENT_INDEX.get(events)
  if (!ix) {
    ix = new Map()
    events.forEach((e) => {
      if (e && e.seq != null && !ix.has(e.seq)) ix.set(e.seq, e)
    })
    EVENT_INDEX.set(events, ix)
  }
  return ix.get(seq) || null
}
const isCount = (v) => Number.isInteger(v) && v >= 0
const clip = (s, n) => (String(s).length > n ? String(s).slice(0, n - 1) + '…' : String(s))

/**
 * Where a run ran, from typed fields only.
 *
 * A check (the signed record's `checks[]`) carries `exit_code`, `parsed`
 * counts, `input_tree_hash` and `argv_hash`; a recorded command step carries
 * `input` when the record kept it. Nothing is read from the agent's prose or
 * from a card's title. Duration and working directory have no field in this
 * record for either, so they are named as not recorded — never blank.
 *
 * @param {object} n a graph node
 * @param {object} session the adapted session (for `checks`, `events`, `finalTree`)
 * @returns {null | {rows: Array<{key: string, value: string, technical?: string}>, absent: string[],
 *   band: null | {ran: string, final: string}, hollow: boolean}}
 */
export function runFacts(n, session) {
  if (!n || n.group || laneOf(n) !== 'execution') return null
  const S = session || {}
  const t = objType(n)
  const rows = []
  const add = (key, value, technical) => rows.push(technical ? { key, value, technical } : { key, value })
  let band = null
  let hollow = false
  let argv = null
  if (t === 'check') {
    const k = (S.checks || []).find((c) => c && c.node === n.id)
    if (!k) return null
    argv = k.argvHash || null
    if (Number.isInteger(k.exit)) add('exit', String(k.exit))
    else hollow = true
    const p = k.parsed || {}
    const parts = []
    if (isCount(p.passed)) parts.push(fill(T.run.passed, { n: p.passed }))
    if (isCount(p.failed)) parts.push(fill(T.run.failed, { n: p.failed }))
    if (isCount(p.skipped)) parts.push(fill(T.run.skipped, { n: p.skipped }))
    if (isCount(p.tests_total)) parts.push(fill(T.run.total, { n: p.tests_total }))
    if (parts.length) add('counts', parts.join(' · '))
    if (k.inputTree) {
      add('version', shortVersion(k.inputTree), fill(HQ_COPY.trace.technical.tree, { hash: k.inputTree }))
      if (S.finalTree && k.inputTree !== S.finalTree) band = { ran: shortVersion(k.inputTree), final: shortVersion(S.finalTree) }
    }
  } else if (n.seq != null) {
    const e = eventAt(S.events, n.seq)
    if (e && (e.kind === 'exec' || e.kind === 'tool_call') && typeof e.input === 'string' && e.input.trim()) {
      add('command', clip(e.input.trim(), 120))
    }
  }
  const order = (a, b) => RUN_KEYS.indexOf(a.key) - RUN_KEYS.indexOf(b.key)
  rows.sort(order)
  const have = new Set(rows.map((r) => r.key))
  const absent = RUN_KEYS.filter((k) => !have.has(k))
  return { rows, absent, band, hollow, argv }
}

/**
 * The lines a run's facts draw on its card, in one face.
 *
 * @param {ReturnType<typeof runFacts>} f @param {'plain'|'technical'} face
 * @returns {{rows: Array<{label: string, value: string}>, absentLine: string|null, band: string|null, bandWhy: string|null, hollowLine: string|null}}
 */
export function factsLines(f, face) {
  if (!f) return { rows: [], absentLine: null, band: null, bandWhy: null, hollowLine: null }
  const technical = face === 'technical'
  const rows = f.rows.map((r) => ({ label: T.run[r.key], value: technical && r.technical ? r.technical : r.value }))
  let absent = f.absent.slice()
  if (technical && f.argv && absent.includes('command')) {
    rows.unshift({ label: T.run.command, value: fill(T.technical.argv_hash, { hash: f.argv }) })
    absent = absent.filter((k) => k !== 'command')
  }
  const words = absent.map((k) => T.run[k].toLowerCase())
  return {
    rows,
    absentLine: words.length ? fill(T.run.absent, { what: words.join(', ') }) : null,
    band: f.band ? T.run.earlier : null,
    bandWhy: f.band ? fill(T.run.earlier_why, { ran: f.band.ran, final: f.band.final }) : null,
    hollowLine: f.hollow ? T.run.no_result : null,
  }
}

/**
 * A check card's title without an exit code the record does not carry. The
 * adapter composes `runner · exit <code>`, and a check with no `exit_code`
 * came out as "exit undefined"; that is not a record value, so neither face
 * shows it (the card says "No result recorded" instead).
 *
 * @param {object} n @returns {string}
 */
export function checkTitle(n) {
  return String((n && n.title) || '').replace(/\s*·\s*exit (?:undefined|null)$/, '')
}

/** Whether a card is the agent's own words: a statement, a promise, a note. */
export function isAgentWords(n) {
  const t = objType(n)
  return t === 'claim' || t === 'promise' || (t === 'event' && n.kind === 'note')
}

/* The line budget of the extra rows a Trace card draws, in px — one line
 * each, never wrapped (the card sets `nowrap`), so the budget is exact. */
export const EXTRA_LINE_H = 16

/**
 * How many extra lines a card draws in the face that draws the most, so the
 * layout never depends on which face is showing.
 *
 * @param {object} n @param {object} session @returns {number}
 */
export function extraLines(n, session) {
  let lines = isAgentWords(n) ? 1 : 0
  const f = runFacts(n, session)
  if (f) {
    const most = Math.max(...['plain', 'technical'].map((face) => {
      const l = factsLines(f, face)
      return l.rows.length + (l.absentLine ? 1 : 0) + (l.band ? 1 : 0) + (l.hollowLine ? 1 : 0)
    }))
    lines += most
  }
  return lines
}

/**
 * A card's height in the Trace: the taller of its two faces, plus its extra
 * lines. Height is never stored; this is read wherever a box is needed.
 *
 * @param {object} n @param {number} w @param {object} session @returns {number}
 */
export function cardHeight(n, w, session) {
  const p = plainFace(n)
  const plain = estH({ ...n, title: p.title, lines: p.lines }, w)
  const x = extraLines(n, session)
  return Math.max(estH(n, w), plain) + (x ? x * EXTRA_LINE_H + 4 : 0)
}

/* ── the Outline twin ─────────────────────────────────────────────────── */

/**
 * The Outline: every drawn card in record order, each with its lane, its
 * words in the face showing, where it sits in the record, and the record's
 * links from it by kind.
 *
 * @param {{nodes: object[], wires: object[]}} session the full record (for links)
 * @param {object[]} shown the cards the picture draws, in any order
 * @param {'plain'|'technical'} face
 * @returns {Array<{id: string, lane: string, laneLabel: string, kind: string, title: string, where: string,
 *   links: Array<{kind: string, word: string, to: string, title: string}>}>}
 */
export function outlineRows(session, shown, face) {
  const S = session || {}
  const byId = new Map((S.nodes || []).map((n) => [n.id, n]))
  ;(shown || []).forEach((n) => byId.has(n.id) || byId.set(n.id, n))
  const titleOf = (n) => (face === 'technical' ? String(n.title || '') : plainFace(n).title)
  const links = new Map()
  ;(S.wires || []).forEach((w) => {
    if (!w || !w.from || !w.to) return
    const kind = edgeKindOf(w)
    const add = (a, b) => {
      if (!links.has(a)) links.set(a, [])
      links.get(a).push({ kind, to: b })
    }
    add(w.from[0], w.to[0])
    add(w.to[0], w.from[0])
  })
  const shownIds = new Set((shown || []).map((n) => n.id))
  return recordOrder(shown).filter((id) => shownIds.has(id)).map((id) => {
    const n = byId.get(id)
    const l = laneOf(n)
    const where = face === 'technical'
      ? (n.seq != null ? fill(T.technical.seq, { n: n.seq }) : String(n.foot || ''))
      : stepLabel(n) || T.face.not_placed
    return {
      id,
      lane: l,
      laneLabel: laneLabel(l),
      kind: face === 'technical' ? String(n.sub || n.kind || '') : kindWord(n),
      title: titleOf(n),
      where,
      links: (links.get(id) || [])
        .filter((k) => byId.has(k.to))
        .map((k) => ({ kind: k.kind, word: edgeWord(k.kind), to: k.to, title: titleOf(byId.get(k.to)) })),
    }
  })
}

/**
 * The Outline as the accessible EQUIVALENT of the picture, not a summary of
 * it (T-6-4; design record §3.3): `outlineRows` for the cards drawn, and under
 * every drawn group card the cards folded inside it — recursively, each id
 * once — so a closed Focus group hides nothing from a reader who cannot see
 * the canvas. A row's `members` is null when nothing is folded inside it.
 *
 * @param {object} session a Session (its `nodes` must hold the members)
 * @param {object[]} shown the cards the picture draws
 * @param {'plain'|'technical'} face
 * @param {Set<string>} [seen] ids already listed (recursion only)
 * @returns {Array<object>} rows, each with `members: rows[]|null`
 */
export function outlineTree(session, shown, face, seen) {
  const S = session || {}
  const byId = new Map((S.nodes || []).map((n) => [n.id, n]))
  ;(shown || []).forEach((n) => byId.has(n.id) || byId.set(n.id, n))
  const listed = seen || new Set((shown || []).map((n) => n.id))
  return outlineRows(S, shown, face).map((r) => {
    const n = byId.get(r.id)
    if (!n || !n.group || !Array.isArray(n.members)) return { ...r, members: null }
    const inner = n.members.filter((id) => !listed.has(id)).map((id) => byId.get(id)).filter(Boolean)
    inner.forEach((m) => listed.add(m.id))
    return { ...r, members: inner.length ? outlineTree(S, inner, face, listed) : null }
  })
}

/** Every id an outline tree lists, in document order. */
export function outlineIds(tree) {
  const out = []
  const walk = (rows) => (rows || []).forEach((r) => { out.push(r.id); walk(r.members) })
  walk(tree)
  return out
}

/**
 * The four lanes the picture always draws, each with how many drawn cards sit
 * in it — so the Outline names an empty lane as empty rather than leaving it
 * out.
 *
 * @param {object[]} shown @returns {Array<{lane: string, label: string, n: number, count: string}>}
 */
export function laneCounts(shown) {
  const n = {}
  LANES.forEach((l) => (n[l] = 0))
  ;(shown || []).forEach((c) => { const l = laneOf(c); n[l] = (n[l] || 0) + 1 })
  return LANES.map((l) => ({
    lane: l,
    label: laneLabel(l),
    n: n[l],
    count: n[l] === 0 ? T.lanes.empty : n[l] === 1 ? T.lanes.count_one : fill(T.lanes.count_many, { n: n[l] }),
  }))
}

/** The words for a thread's size, "1 step" / "4 steps". */
export const threadSteps = (n) => (n === 1 ? T.thread.steps_one : fill(T.thread.steps_many, { n }))

/** The sentence naming each stage the thread has no card for. */
export const missingLines = (thread) => (thread ? thread.missing.map((s) => fill(T.thread.missing, { stage: T.thread.stages[s] })) : [])

export default laneOf
