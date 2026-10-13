/* ThruLine workspace — the Trace Focus set (HQ-VERDICT-PANEL-PLAN §1.4, Step 5).
 *
 * PURE. No fetch, no `window`, no React, no clock, no locale, no randomness.
 * One transform over the node and wire arrays `adapter.buildNodes` and
 * `adapter.buildWires` produced. It changes no node, no wire, no colour and no
 * edge: Focus is a VIEW — which ids to show, and how the rest are accounted —
 * and Everything is today's graph unchanged.
 *
 * THE RULE. A seed is any object whose state is short of Proven: a claim that
 * is not_proven / couldnt / fog, a promise that is loose, a finding whose
 * severity ends in fail or warn, an ask that is not_proven / loose / broken.
 * The set kept is the union, over every seed, of nodes reachable over the
 * derivation wires — `ref` (documented reference) and `inf` (inferred) — within
 * `depth` hops, walked undirected. `order` wires (chronology) never extend the
 * set: being next to a problem is not being part of its evidence.
 *
 * THE ACCOUNTING. Nothing is dropped. Every node not kept lands in exactly one
 * group card by category, each with a count-bearing label, so the dim-not-hide
 * invariant is STATED ("7 of 26 objects shown · 19 merged") rather than broken.
 *
 * DETERMINISM. Seeds and kept are sorted; BFS follows wire input order; the
 * same input yields the same output byte for byte. Dangling wire endpoints are
 * skipped, never thrown on. Nodes lacking fields are tolerated.
 */

import { countPhrase } from '../lib/voice.js'
import { objType } from './logic.js'

/** Wire kinds that carry derivation. Everything else — `order` included — is not followed. */
const DERIVATION = Object.freeze({ ref: true, inf: true })

const CLAIM_SEED_WORDS = Object.freeze({ not_proven: true, couldnt: true, fog: true })
const ASK_SEED_WORDS = Object.freeze({ not_proven: true, loose: true, broken: true })

/** Event kinds that, with no risk flag, "hold up" and merge into one card. */
const HOLDS_KINDS = Object.freeze({ exec: true, tool_call: true, tool_result: true })

/**
 * Whether a node's state is short of Proven — the seed predicate.
 *
 * @param {object} n @returns {boolean}
 */
export function isSeed(n) {
  if (!n || n.id == null) return false
  const t = objType(n)
  const word = n.word == null ? '' : String(n.word)
  if (t === 'claim') return !!CLAIM_SEED_WORDS[word]
  if (t === 'promise') return word === 'loose'
  if (t === 'ask') return !!ASK_SEED_WORDS[word]
  if (t === 'finding') {
    const sub = n.sub == null ? '' : String(n.sub)
    return /(fail|warn)$/i.test(sub)
  }
  return false
}

/** Ids of every well-formed node, and their adjacency over derivation wires only. */
function graphOf(nodes, wires) {
  const src = Array.isArray(nodes) ? nodes : []
  const ws = Array.isArray(wires) ? wires : []
  const present = new Map()
  src.forEach((n) => {
    if (n && n.id != null && !present.has(n.id)) present.set(n.id, n)
  })
  const adj = new Map()
  const push = (a, b) => {
    let list = adj.get(a)
    if (!list) adj.set(a, (list = []))
    list.push(b)
  }
  ws.forEach((w) => {
    if (!w || !DERIVATION[w.kind]) return
    const a = Array.isArray(w.from) ? w.from[0] : undefined
    const b = Array.isArray(w.to) ? w.to[0] : undefined
    /* A dangling endpoint is skipped, not thrown on: a wire into a node that
     * was never built is a fact about the record, and the walk simply
     * cannot cross it. */
    if (a == null || b == null || !present.has(a) || !present.has(b)) return
    push(a, b)
    push(b, a)
  })
  return { present, adj }
}

/** Multi-source BFS; returns min-hop depth per reached id, bounded by `depth`. */
function walk(adj, starts, depth) {
  const depthOf = {}
  const queue = []
  starts.forEach((id) => {
    if (depthOf[id] != null) return
    depthOf[id] = 0
    queue.push(id)
  })
  let head = 0
  while (head < queue.length) {
    const id = queue[head++]
    const d = depthOf[id]
    if (d >= depth) continue
    const next = adj.get(id) || []
    for (let i = 0; i < next.length; i++) {
      const m = next[i]
      if (depthOf[m] != null) continue
      depthOf[m] = d + 1
      queue.push(m)
    }
  }
  return { depthOf, order: queue }
}

const byId = (a, b) => (a < b ? -1 : a > b ? 1 : 0)

/**
 * The Focus set.
 *
 * @param {object[]} nodes every node `buildNodes` produced, in its order
 * @param {object[]} wires every wire `buildWires` produced
 * @param {{depth?:number}} [opts]
 * @returns {{seeds:string[], kept:string[], depthOf:Record<string,number>}}
 */
export function focusSet(nodes, wires, { depth = 3 } = {}) {
  const hops = Number.isFinite(depth) && depth >= 0 ? Math.floor(depth) : 3
  const { present, adj } = graphOf(nodes, wires)
  const seeds = []
  present.forEach((n, id) => {
    if (isSeed(n)) seeds.push(String(id))
  })
  seeds.sort(byId)
  const { depthOf } = walk(adj, seeds, hops)
  const kept = Object.keys(depthOf).sort(byId)
  return { seeds, kept, depthOf }
}

/**
 * The category a non-kept node merges under, and the card copy for each.
 * Each category is a closed rule over the record's own fields (`objType`,
 * `kind`, `risk`, `word`) — never over a rendered title.
 */
function categoryOf(n) {
  const t = objType(n)
  if (t === 'event') {
    const kind = n.kind == null ? '' : String(n.kind)
    if (HOLDS_KINDS[kind] && !n.risk) return 'holds'
    if (kind === 'fs_change') return 'fs'
    if (kind === 'snapshot') return 'trees'
    return 'other'
  }
  if (t === 'claim') return n.word === 'proven' ? 'record' : 'other'
  if (t === 'check' || t === 'receipt') return 'record'
  return 'other'
}

/** Category order on the card rail. Stated once so every surface agrees. */
const CATEGORIES = Object.freeze(['holds', 'fs', 'trees', 'record', 'other'])

const LABEL = Object.freeze({
  holds: (n) => countPhrase(n, 'event that holds up', 'events that hold up'),
  fs: (n) => countPhrase(n, 'file change', 'file changes'),
  trees: (n) => countPhrase(n, 'tree recorded', 'trees recorded'),
  record: (n) => countPhrase(n, 'for the record', 'for the record'),
  other: (n) => countPhrase(n, 'other step', 'other steps'),
})

/** The card label for a category and a count. `null` for an unknown category. */
export function groupLabel(kind, n) {
  return LABEL[kind] ? LABEL[kind](n) : null
}

/** Tree ids as the record spells them (`tree <hex>`), from a snapshot's text, title or first line. */
function treeIdOf(n) {
  const candidates = [n.text, n.title, Array.isArray(n.lines) ? n.lines[0] : undefined]
  for (let i = 0; i < candidates.length; i++) {
    const s = candidates[i]
    if (s == null) continue
    const m = /\btree ([0-9a-f]+)/.exec(String(s))
    if (m) return m[1]
  }
  return null
}

function lineOf(cat, members) {
  if (cat === 'holds') return 'reads, edits and runs with nothing against them'
  if (cat === 'fs') return countPhrase(members.length, 'recorded change to the working tree', 'recorded changes to the working tree')
  if (cat === 'record') return 'proven claims and the checks behind them'
  if (cat === 'trees') {
    const ids = []
    for (let i = 0; i < members.length && ids.length < 3; i++) {
      const id = treeIdOf(members[i])
      if (id && ids.indexOf(id) < 0) ids.push(id)
    }
    return ids.length ? ids.map((id) => 'tree ' + id).join(' · ') : 'recorded code checkpoints'
  }
  return 'steps outside every chain'
}

/**
 * The Focus view: which ids to draw, and the group cards that account for the rest.
 *
 * `degraded` is the recorder's condition (`S.capture.degraded`). With no seed
 * AND a degraded capture, "Everything holds up" would be a confident phrase
 * over a record that was not fully watched (ADJUDICATIONS item C, loose
 * thread): the single group is then emitted with `kind:'all'` but WITHOUT that
 * label — `summary.allClear` is false and the caller supplies the recording-gap
 * copy. This module never words a gap it cannot see the cause of.
 *
 * @param {object[]} nodes @param {object[]} wires @param {{depth?:number, degraded?:boolean}} [opts]
 * @returns {{mode:'focus', shown:string[], groups:Array<{id:string,label:string,
 *   count:number,line:string,members:string[],kind:string}>,
 *   summary:{shown:number,total:number,merged:number,allClear:boolean,degraded:boolean}}}
 */
export function focusView(nodes, wires, opts) {
  const o = opts || {}
  const degraded = o.degraded === true
  const src = (Array.isArray(nodes) ? nodes : []).filter((n) => n && n.id != null)
  const total = src.length
  const set = focusSet(src, wires, o)

  if (set.seeds.length === 0) {
    const members = src.map((n) => String(n.id))
    const allClear = !degraded
    const groups = total
      ? [
          allClear
            ? { id: 'grp_focus_all', label: 'Everything holds up', count: total, line: 'no claim, promise, finding or ask is short of proven', members, kind: 'all' }
            : { id: 'grp_focus_all', label: countPhrase(total, 'step recorded', 'steps recorded'), count: total, line: '', members, kind: 'all' },
        ]
      : []
    return { mode: 'focus', shown: [], groups, summary: { shown: 0, total, merged: total, allClear, degraded } }
  }

  const keptSet = new Set(set.kept)
  const shown = []
  const buckets = new Map()
  src.forEach((n) => {
    const id = String(n.id)
    if (keptSet.has(id)) {
      shown.push(id)
      return
    }
    const cat = categoryOf(n)
    let list = buckets.get(cat)
    if (!list) buckets.set(cat, (list = []))
    list.push(n)
  })

  const groups = []
  CATEGORIES.forEach((cat) => {
    const members = buckets.get(cat)
    if (!members || !members.length) return
    groups.push({
      id: 'grp_focus_' + cat,
      label: LABEL[cat](members.length),
      count: members.length,
      line: lineOf(cat, members),
      members: members.map((n) => String(n.id)),
      kind: cat,
    })
  })

  return { mode: 'focus', shown, groups, summary: { shown: shown.length, total, merged: total - shown.length, allClear: false, degraded } }
}

/** A focus group card's id prefix. `shell.jsx` keys `focusExpanded` on it. */
export const FOCUS_GROUP_PREFIX = 'grp_focus_'

/** @param {string} id @returns {boolean} whether the id names a focus group card */
export function isFocusGroupId(id) {
  return typeof id === 'string' && id.indexOf(FOCUS_GROUP_PREFIX) === 0
}

/**
 * THE PROJECTION the graph draws in Focus: the derived node list, per render.
 *
 * PURE, like everything above. `S.nodes` is never written through — every
 * node is a shallow copy — and Everything returns the caller's own array
 * untouched (same identity), so today's graph is today's graph.
 *
 * In Focus, every node outside the Focus set is a MEMBER of exactly one
 * synthetic group card (`member: <group id>`), which is the same relation the
 * level-of-detail fold states (`lod.js:222,282`), so `visible()`,
 * `resolveEnd()` and the group-card rendering path in `shell.jsx` treat a
 * focus group exactly as they treat a folded run: the card is drawn, its
 * members are not, and a wire into a member attaches to the card. A group
 * the reader has opened in place (`focusExpanded[id]`) is omitted and its
 * members render as today.
 *
 * A kept node whose level-of-detail group is NOT kept has that `member`
 * cleared, because a card in the Focus set must be on screen: hiding it
 * behind a folded card that is itself merged would be a silent drop.
 *
 * The summary counts OBJECTS OF THE RECORD (`!synthetic`), which is the
 * denominator every other surface uses; group cards stand for objects and
 * are not objects. An object is SHOWN unless it is behind a closed Focus
 * group card: a member of a level-of-detail fold (a call's results, a run of
 * snapshots) whose Focus group is open has a card on screen — its fold card —
 * and counts as shown, so `shown + Σ closed-group counts == total` holds
 * after any Expand (ADJUDICATIONS item F).
 *
 * @param {object[]} nodes @param {object[]} wires
 * @param {{focusMode?:'focus'|'everything', focusExpanded?:Record<string,boolean>,
 *   degraded?:boolean, gapLabel?:string, gapLine?:string, depth?:number}} [state]
 * @returns {{mode:'focus'|'everything', nodes:object[],
 *   groups:Array<{id:string,label:string,line:string,kind:string,members:string[],expanded:boolean}>,
 *   summary:{shown:number,total:number,merged:number,allClear:boolean,degraded:boolean}|null}}
 */
export function focusProjection(nodes, wires, state) {
  const st = state || {}
  const src = Array.isArray(nodes) ? nodes : []
  if (st.focusMode !== 'focus') return { mode: 'everything', nodes: src, groups: [], summary: null }

  const opened = st.focusExpanded || {}
  const view = focusView(src, wires, { depth: st.depth, degraded: st.degraded === true })
  const byId = new Map()
  let objects = 0
  src.forEach((n) => {
    if (n && n.id != null) {
      byId.set(String(n.id), n)
      if (!n.synthetic) objects += 1
    }
  })

  /* Which group each hidden id belongs to, and which groups are open. */
  const groupOf = new Map()
  const groups = view.groups.map((g) => {
    const expanded = !!opened[g.id]
    if (!expanded) g.members.forEach((m) => groupOf.set(m, g.id))
    let label = g.label
    let line = g.line
    /* A level-of-detail fold card (`grp_tree_*`) is merged like any member,
     * but it stands for objects and is not one: the count on the label is
     * OBJECTS, so it reconciles with the pill's denominator. */
    const count = g.members.filter((m) => {
      const n = byId.get(m)
      return n && !n.synthetic
    }).length
    if (g.kind === 'all' && !view.summary.allClear) {
      /* The caller words the gap; this module words the count. */
      label = (st.gapLabel || 'Recording gap') + ' · ' + countPhrase(objects, 'step recorded', 'steps recorded')
      line = st.gapLine || 'the recorder was not vouched for this session — cause not recorded'
    } else if (g.kind !== 'all' && count !== g.count) {
      label = groupLabel(g.kind, count) || label
    }
    return { id: g.id, label, line, kind: g.kind, members: g.members.slice(), count, expanded }
  })

  const out = []
  let shown = 0
  let total = 0
  src.forEach((n) => {
    if (!n || n.id == null) return
    const id = String(n.id)
    const c = { ...n }
    const gid = groupOf.get(id)
    if (gid) {
      c.member = gid
    } else if (c.member != null && groupOf.has(String(c.member))) {
      /* Kept, but its level-of-detail group is merged: the card must be seen. */
      delete c.member
    }
    if (!n.synthetic) {
      total += 1
      /* Behind a level-of-detail fold is on screen (the fold card is drawn);
       * behind a closed Focus group is merged. */
      if (!c.member || !isFocusGroupId(String(c.member))) shown += 1
    }
    out.push(c)
  })

  groups.forEach((g) => {
    if (g.expanded) return
    /* The card, in the shape `lod.js` gives a folded run so the existing
     * group-card path renders it: `group`, `members`, `rep`, `toggleLabel`,
     * `toggleAria`, a span, and the member kinds for the facet index. */
    const members = g.members.map((m) => byId.get(m)).filter(Boolean)
    const seqs = members.map((m) => (m.seq != null ? m.seq : m.firstSeq)).filter((s) => s != null)
    const kinds = []
    const frames = {}
    let w = 0
    members.forEach((m) => {
      if (m.kind != null && kinds.indexOf(String(m.kind)) < 0) kinds.push(String(m.kind))
      if (m.frame) frames[m.frame] = (frames[m.frame] || 0) + 1
      if (m.w > w) w = m.w
    })
    const frame = Object.keys(frames).sort((a, b) => frames[b] - frames[a] || (a < b ? -1 : 1))[0]
    /* Where a wire into a member attaches on the card: `shell.pinPos` reads
     * `pinSide` when the pin id is not the card's own. */
    const pinSide = {}
    members.forEach((m) => {
      ;(m.pinsL || []).forEach((p) => {
        if (p && p.id != null && !pinSide[p.id]) pinSide[p.id] = 'L'
      })
      ;(m.pinsR || []).forEach((p) => {
        if (p && p.id != null && !pinSide[p.id]) pinSide[p.id] = 'R'
      })
    })
    const card = {
      id: g.id,
      objType: 'group',
      synthetic: true,
      group: true,
      focusGroup: true,
      kind: g.kind,
      members: g.members.slice(),
      memberKinds: kinds,
      rep: g.members[0],
      fam: 'grey',
      type: 'MERGED',
      /* The count-bearing label is the TITLE, where it wraps; the header's
       * `sub` slot is shared with the seq span and the toggle and ellipsizes. */
      sub: 'Focus group',
      title: g.label,
      lines: g.line ? [g.line] : [],
      mono: false,
      pinsL: [],
      pinsR: [],
      pinSide,
      x: 0,
      y: 0,
      w: w || 280,
      foot: 'merged in Focus · ' + countPhrase(g.count, 'object', 'objects') + ' · expand to see each one',
      toggleLabel: '▸ Expand',
      toggleAria: 'Expand ' + g.label + ' in place',
    }
    if (frame) card.frame = frame
    if (seqs.length) {
      card.firstSeq = Math.min.apply(null, seqs)
      card.lastSeq = Math.max.apply(null, seqs)
    }
    out.push(card)
  })

  return {
    mode: 'focus',
    nodes: out,
    groups,
    summary: { shown, total, merged: total - shown, allClear: view.summary.allClear, degraded: view.summary.degraded },
  }
}

/**
 * One seed's chain: the seed first, then every node reachable over derivation
 * wires within `depth` hops, in BFS order. `[]` when the seed is not a node.
 *
 * @param {object[]} nodes @param {object[]} wires @param {string} seedId @param {number} [depth]
 * @returns {string[]}
 */
export function chainOf(nodes, wires, seedId, depth = 3) {
  const hops = Number.isFinite(depth) && depth >= 0 ? Math.floor(depth) : 3
  const { present, adj } = graphOf(nodes, wires)
  if (seedId == null || !present.has(seedId)) return []
  return walk(adj, [seedId], hops).order.map(String)
}
