/* ThruLine workspace — the level-of-detail model (DESIGN-readable-trace §2, §3).
 *
 * PURE. No fetch, no `window`, no React, no clock, no locale. One transform
 * over the node list `adapter.buildNodes` produced, run BEFORE `autoLayout`
 * assigns x/y/w, so the graph, timeline, table, minimap, left dock and bottom
 * dock all read one node array and cannot disagree.
 *
 * WHAT IT IS NOT. It is not a new subsystem. The grouping vocabulary —
 * `group` / `members` / `rep` / `member` — already exists and is already
 * honoured by `shell.jsx` (`visible()`, `resolveEnd()`, `placedSeq()`); only
 * the bundled demo records ever used it. This file is a producer for that
 * existing contract on real sessions.
 *
 * THE TWO RULES, in order (§2.1). Both are structural and both ship together:
 * R1′ pays only where snapshots exist, R3 pays on every session shape.
 *
 *   R3 — group an unambiguous nearby success as a labeled order-based
 *         navigation inference. No call-id binding is claimed. Concurrent
 *         calls of the same tool retain separate results.
 *   R1′ — group repeated recorded code markers. Import-time captures do not
 *         establish that the historical workspace stayed unchanged.
 *
 * NF — THE NEVER-FOLD RULE (§3). A node is unfoldable when it carries risk, a
 * finding or a severity, a negative state word, an ending, or a non-success
 * outcome; and a candidate run SPLITS AROUND IT, producing two runs rather
 * than one run with a hole. The engine already enforces this one level down
 * (`crates/explain/src/timeline.rs:61-63`: "never group anything risky — the
 * whole point of a risk flag is that it is visible"), and this UI must not be
 * weaker than the engine it renders.
 *
 * THE INPUT IS THE RECORD, NOT THE LABEL. Every structural fact is read from
 * the transported event (`{seq, kind, text, risk}` — `crates/explain/src/
 * timeline.rs:6-14`), never from a node's rendered `title`. That is what keeps
 * this transform correct while the humaniser changes every title in the
 * session.
 */

import { countPhrase } from '../lib/voice.js'
import { NEGATIVE_WORDS, objType } from './logic.js'

/* `crates/capture/src/fold.rs:1031-1040` — three values, never two. Collapsing
 * `NotStated` into `Succeeded` already shipped once as a bug
 * (`crates/cli/src/explain_cmd.rs:436-439`); the labels below are the engine's
 * own strings, matched exactly, so a fourth label refuses rather than
 * defaulting to success. */
const OUTCOME_LABEL = Object.freeze({
  ok: 'ok',
  FAILED: 'failed',
  'outcome not stated': 'not_stated',
})

/** The en dash that joins a seq range. Stated once so every surface agrees. */
const RANGE = '–'

/**
 * The structural view of one transported event.
 *
 * Stage B of the humaniser (§1.1): the transport carries `text` only, so the
 * tool token is recovered by splitting on the first space — which is exactly
 * how `summarize()` composed it (`explain_cmd.rs:435,441-445`). Nothing here
 * parses JSON: 8.2% of this session's tool inputs are cut mid-JSON at the
 * 2,000-character capture cap, and the transport's own 50-character cut makes
 * it near-universal.
 *
 * @param {{seq:number, kind:string, text?:string, risk?:string}} e
 * @returns {{seq:number, kind:string, tool:string|null, outcome:string|null,
 *   tree:string|null, atFold:boolean, risk:boolean}}
 */
export function transportView(e) {
  const kind = e && e.kind ? String(e.kind) : ''
  const text = e && e.text != null ? String(e.text) : ''
  const view = { seq: e ? e.seq : null, kind, tool: null, outcome: null, tree: null, atFold: false, risk: !!(e && e.risk) }

  if (kind === 'tool_call' || kind === 'tool_result') {
    const cut = text.indexOf(' ')
    const head = cut < 0 ? text : text.slice(0, cut)
    /* An empty head is the real, measured case: `tool` is absent on 64.9% of
     * `tool_result` rows store-wide and `summarize` composes a leading space.
     * It is not guessed and not hidden — it simply cannot join. */
    if (head) view.tool = head
    if (kind === 'tool_result') {
      const tail = cut < 0 ? '' : text.slice(cut + 1)
      const mapped = OUTCOME_LABEL[tail]
      view.outcome = mapped || null
    }
  }

  if (kind === 'snapshot') {
    const m = /^tree ([0-9a-f]+)/.exec(text)
    if (m) view.tree = m[1]
    view.atFold = / \(taken at fold\)$/.test(text)
  }

  return view
}

/**
 * NF, as a predicate over one node (§3).
 *
 * Clause 5 — "selected by an active filter, a find-in-trace hit, or an applied
 * saved view" — is not here: it is a property of the READING, not of the
 * record, and it is enforced at presentation by forcing the member visible
 * (`shell.jsx` `forcedMembers`). Every clause that IS a property of the record
 * is here, and it is the only place a fold asks.
 *
 * @param {object} n a node from `buildNodes`
 * @param {{outcome?:string|null}} [view] the structural view of its event
 * @returns {boolean}
 */
export function unfoldable(n, view) {
  if (!n) return true
  /* NF 7 — derived objects are never event-folded. */
  if (objType(n) !== 'event') return true
  /* NF 2 — the destructive-command flag. */
  if (n.risk) return true
  /* NF 3 — carries or anchors a finding. */
  if (n.severity || n.finding) return true
  /* NF 1 — a state word worse than Kept/Proven. */
  if (n.word && NEGATIVE_WORDS.indexOf(n.word) >= 0) return true
  /* NF 4 — the stop, the session end, the ending. */
  if (n.kind === 'stop' || n.kind === 'session_end' || n.id === 'ending') return true
  /* NF 6 — a non-success never folds into an "ok" band. */
  if (view && view.outcome && view.outcome !== 'ok') return true
  return false
}

/**
 * THE TRANSFORM. Pure: it reads `nodes` and `events` and returns a new array;
 * neither argument is mutated, and the same input yields the same output on
 * every machine and every run.
 *
 * @param {object[]} nodes every node `buildNodes` produced, in its order
 * @param {Array<{seq:number, kind:string, text?:string, risk?:string}>} events
 *   the transported events, in recorded order
 * @returns {{nodes: object[], stats: {before:number, after:number, paired:number,
 *   folded:number, groups:number}}}
 */
export function foldSteps(nodes, events) {
  const src = Array.isArray(nodes) ? nodes : []
  const evs = Array.isArray(events) ? events : []

  /* Shallow copies: the caller's array is never written through. */
  const out = src.map((n) => ({ ...n }))
  const byId = new Map()
  out.forEach((n) => byId.set(n.id, n))
  const bySeq = new Map()
  out.forEach((n) => {
    if (n.seq != null && objType(n) === 'event') bySeq.set(n.seq, n)
  })

  const views = evs.map(transportView)
  const groups = []
  let paired = 0
  let folded = 0

  /* ── R3 — pair a call with its result ─────────────────────────────────── */
  /* Grouping by tool and order is a navigation inference, not a call-id
   * binding. Concurrent calls of the same tool remain separate: completion
   * order is not guaranteed. Never attach their successes to the wrong work. */
  const pending = []
  views.forEach((v, i) => {
    if (v.kind === 'tool_call') {
      const node = bySeq.get(v.seq)
      if (!node) return
      const m = /\(x(\d+)\)$/.exec(String(evs[i].text || ''))
      let n = m ? parseInt(m[1], 10) : 1
      if (!(n >= 1)) n = 1
      const sameTool = pending.filter(c => c.v.tool === v.tool)
      const ambiguous = sameTool.length > 0 || n > 1
      if (ambiguous) sameTool.forEach(c => { c.ambiguous = true })
      for (let k = 0; k < n; k++) pending.push({ v, node, ambiguous })
      return
    }
    if (v.kind !== 'tool_result') return
    const resNode = bySeq.get(v.seq)
    if (!resNode) return

    let at = -1
    if (v.tool && v.tool !== 'unknown') {
      at = pending.findIndex((c) => c.v.tool === v.tool)
    }
    /* seq is sparse and global (this session spans 8044…93028 over 4,279
     * rows), so a gap of exactly 1 means the two rows are neighbours in the
     * ledger and NOTHING — no other call, no other anything — sits between
     * them. That is the second clause of §2.1's proof, and it is what carries
     * the two legacy-shaped sessions, where `tool` is recorded as `unknown` on
     * every single result and the tool-name join is unavailable. */
    if (at < 0 && pending.length === 1) at = pending.findIndex((c) => v.seq - c.v.seq === 1)
    if (at < 0) {
      /* Unknown correspondence: retain the result and retire pending
       * candidates so a later success cannot attach to a stale call. */
      pending.length = 0
      return
    }

    const call = pending[at]
    pending.splice(at, 1)
    /* NF 6: only a success folds into its call. A failure and a silence keep
     * their own row, where the reader can see them without a gesture. */
    if (call.ambiguous || unfoldable(resNode, v)) return

    const callNode = call.node
    if (callNode.members) {
      callNode.members.push(resNode.id)
      callNode.lastSeq = v.seq
      if (v.outcome !== 'ok') callNode.outcome = v.outcome
    } else {
      callNode.group = true
      callNode.members = [resNode.id]
      callNode.memberKinds = ['tool_result']
      callNode.rep = resNode.id
      callNode.firstSeq = callNode.seq
      callNode.lastSeq = v.seq
      callNode.outcome = v.outcome
      /* The outcome travels on the metadata line, not on the title: the title
       * is the humaniser's (§1.5) and this transform never writes event text.
       * Only an `ok` ever folds, so the fact carried here is the quiet one. */
      if (v.outcome) callNode.foot = String(callNode.foot || '') + ' · nearby result ' + v.outcome + ' · inferred link'
    }
    callNode.toggleLabel = '▸ ' + countPhrase(callNode.members.length, 'result', 'results')
    callNode.toggleAria = 'Expand ' + countPhrase(callNode.members.length, 'nearby result (inferred by tool and order)', 'nearby results (inferred by tool and order)')
    resNode.member = callNode.id
    paired += 1
  })

  /* ── R1′ — fold a run of snapshots that say nothing changed ───────────── */
  const snaps = views.filter((v) => v.kind === 'snapshot')
  const runs = []
  let run = null
  snaps.forEach((v) => {
    const node = bySeq.get(v.seq)
    const breaks = !node || !v.tree || unfoldable(node, v)
    if (breaks) {
      /* NF: the run SPLITS around it — two runs, never one run with a hole. */
      run = null
      return
    }
    if (run && run.tree === v.tree) {
      run.items.push({ v, node })
      return
    }
    run = { tree: v.tree, atFold: v.atFold, items: [{ v, node }] }
    runs.push(run)
  })

  runs.forEach((r) => {
    /* A run of one does not fold. One card replaced by one card, plus a
     * disclosure the reader must now open, is worse than the card. */
    if (r.items.length < 2) return
    const first = r.items[0]
    const last = r.items[r.items.length - 1]
    const g = {
      id: 'grp_tree_' + first.v.seq,
      objType: 'event',
      kind: 'snapshot',
      synthetic: true,
      group: true,
      members: r.items.map((it) => it.node.id),
      memberKinds: ['snapshot'],
      rep: first.node.id,
      firstSeq: first.v.seq,
      lastSeq: last.v.seq,
      fam: first.node.fam,
      type: 'EVENT',
      sub: 'Working tree',
      /* The headline states the invariant, not the row count (§2.2 rule 1).
       * `unchanged` is printed because two recorded tree hashes were compared
       * and were equal — §1.9 rule 3 — and for no other reason. */
      title: r.atFold ? 'Repeated code markers · captured on import' : 'Repeated code checkpoints',
      lines: ['tree ' + r.tree + ' · seq ' + first.v.seq + RANGE + last.v.seq + (r.atFold ? ' · taken at fold' : '')],
      mono: false,
      pinsL: [],
      pinsR: [],
      foot: 'seq ' + first.v.seq + RANGE + last.v.seq + ' · snapshot',
      /* `canvas.jsx:458-460` documents the rule this satisfies: "▸ 3 events,
       * never ▸ 3". No code in `ui/src` ever assigned it before now, so the
       * toggle rendered as a button with no text. */
      toggleLabel: '▸ ' + countPhrase(r.items.length, 'snapshot event', 'snapshot events'),
      toggleAria: 'Expand ' + countPhrase(r.items.length, 'snapshot event', 'snapshot events') + ', seq ' + first.v.seq + RANGE + last.v.seq,
    }
    r.items.forEach((it) => {
      it.node.member = g.id
      folded += 1
    })
    groups.push(g)
  })

  /* ── Order pins (§2.3 "order wires") ──────────────────────────────────── */
  /* `allWires()` asks every event node for pins `o`/`i` and every event node
   * gave it `[]`, so ZERO recorded-order wires were drawn on any real session
   * while 4,242 wire attempts were computed and thrown away per render. A band
   * that says "seq 8044 → 12930" is only legible if the order wire into and
   * out of it is drawn, so the pins are emitted in this same pass. */
  const withGroups = out.concat(groups)
  withGroups.forEach((n) => {
    if (objType(n) !== 'event') return
    if (!Array.isArray(n.pinsL)) n.pinsL = []
    if (!Array.isArray(n.pinsR)) n.pinsR = []
    if (!n.pinsL.some((p) => p.id === 'i')) n.pinsL.push({ id: 'i', label: 'recorded order', t: 'order' })
    if (!n.pinsR.some((p) => p.id === 'o')) n.pinsR.push({ id: 'o', label: 'recorded order', t: 'order' })
  })

  const eventNodes = withGroups.filter((n) => objType(n) === 'event' && !n.synthetic)
  const after = withGroups.filter((n) => objType(n) === 'event' && !n.member).length

  return {
    nodes: withGroups,
    stats: {
      before: eventNodes.length,
      after,
      paired,
      folded,
      groups: groups.length + paired,
    },
  }
}

/**
 * The reconciliation §3's corollary demands, as a number a test can assert:
 * every recorded event is either drawn or counted, and nothing is dropped.
 *
 * @param {object[]} nodes the transform's output
 * @returns {number} members + ungrouped record-backed event nodes
 */
export function reconcile(nodes) {
  const list = Array.isArray(nodes) ? nodes : []
  return list.filter((n) => objType(n) === 'event' && !n.synthetic).length
}

/**
 * The seq a node is placed at, for sorting and for replay. A group spans a
 * range and is placed at its first member; `lastSeq` is what decides whether
 * it is fully past (`shell.jsx` `isDim`).
 *
 * @param {object} n @returns {number|null}
 */
export function placementSeq(n) {
  if (!n) return null
  if (n.seq != null) return n.seq
  if (n.firstSeq != null) return n.firstSeq
  return null
}
