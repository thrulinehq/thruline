/* The Inbox queue — C7's attention surface, over the §9 summary index.
 *
 * This file *selects and orders*. It writes no English: every sentence and
 * every suggested action the Inbox renders is composed by the Rust voice
 * authority and carried verbatim in the index. A sentence composed in a view is
 * a sentence no lexicon gate has read (§5v), and that rule does not relax
 * because the view is a queue.
 *
 * What the index delivers here was measured before this file existed —
 * docs/recon/c7-attention-probe.json, pinned by
 * crates/core/tests/c7_attention_index_contract.rs. Two facts from that probe
 * are load-bearing below: an item carries exactly `action`, `id`, `kind`,
 * `sentence`, and the record separates *evaluated calm* (`all_clear`) from
 * *never evaluated* (`not_read`) rather than flattening both to an empty list.
 */

/**
 * Kinds that put something in front of a person, in rank order.
 *
 * The array's order IS the rank — see the order rule below. `all_clear` is
 * deliberately absent: it is a finding of calm, not a thing to do, and a queue
 * that lists it has turned "nothing needs you" into 90 rows of noise.
 */
export const ACTIONABLE_KINDS = ['contradiction', 'staleness', 'capture_attention', 'not_read']

/** Whether a row can reach the claim its sentence is about (A188). */
export const hasReceipt = (row) => typeof row.nodeId === 'string' && row.nodeId.length > 0

/** The one kind that reports calm rather than work. */
export const CALM_KIND = 'all_clear'

/**
 * THE INBOX ORDER RULE (C7-O1) — the queue is sorted by, in this order:
 *
 *   1. **kind rank** — the index of the kind in `ACTIONABLE_KINDS`; a kind this
 *      build does not recognise ranks last rather than vanishing.
 *   2. **project name**, code-unit ascending.
 *   3. **item id**, code-unit ascending.
 *
 * Every component is total and the last one is unique, so no pair of rows is
 * ever left in whatever order the export happened to emit. This is named and
 * asserted (`inbox.test.js`) rather than left to `Array.prototype.sort`,
 * because an incidental order is an order that moves when the export's row
 * order moves — and a queue that reshuffles under a person is a queue they stop
 * reading.
 *
 * The rank is by **what the row asserts**, not by how loud it is. A
 * `contradiction` is an accusation the record can prove. A `staleness` is a
 * proof that has decayed. A `capture_attention` is about the recording
 * machinery rather than the work itself. A `not_read` is the *absence* of a
 * check — real work for a person, but not a finding, so it sits below the three
 * findings rather than above them.
 */
export const ORDER_RULE = 'C7-O1'

const rank = (kind) => {
  const i = ACTIONABLE_KINDS.indexOf(kind)
  return i === -1 ? ACTIONABLE_KINDS.length : i
}

const byOrderRule = (a, b) =>
  rank(a.kind) - rank(b.kind) ||
  (a.project < b.project ? -1 : a.project > b.project ? 1 : 0) ||
  (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)

/**
 * The queue, plus everything needed to answer the empty state honestly.
 *
 * `rows` are the things to do. `calm` counts projects that were evaluated and
 * came back clean. `silent` names projects that returned no item at all — a
 * §9 contract violation rather than a product state, surfaced rather than
 * counted as calm, because counting it as calm is exactly how "everything
 * checks out" once shipped over 43 unexamined projects. `unrecognized` names
 * kinds this build has no rank for; they still render.
 */
export function inboxQueue(index) {
  const projects = (index && index.projects) || []
  const rows = []
  const silent = []
  const unrecognized = new Set()
  let calm = 0

  for (const p of projects) {
    const items = (p && p.attention && p.attention.items) || []
    if (items.length === 0) {
      silent.push(p.fixture || p.name || '')
      continue
    }
    let actionable = 0
    for (const it of items) {
      if (it.kind === CALM_KIND) continue
      actionable += 1
      if (rank(it.kind) === ACTIONABLE_KINDS.length) unrecognized.add(it.kind)
      rows.push({
        id: it.id,
        kind: it.kind,
        sentence: it.sentence,
        action: it.action,
        project: p.name,
        fixture: p.fixture,
        /* A188: present only on claim-bearing kinds. `undefined` here is
           meaningful — it is the export saying this row has no claim to point
           at, and the row must degrade to the project link rather than invent
           one. Never coerce it to '' or null. */
        nodeId: it.node_id,
      })
    }
    if (actionable === 0) calm += 1
  }

  rows.sort(byOrderRule)
  return {
    rows,
    calm,
    silent,
    unrecognized: [...unrecognized].sort(),
    projects: projects.length,
  }
}

/**
 * Whether the Inbox has earned the strong sentence.
 *
 * "Everything checks out" is a claim. It is true only when every project in the
 * index came back *evaluated* and clean: nothing to do, nothing silent, and at
 * least one project actually read. Anything else and the empty state says the
 * weaker true thing instead — the record can tell these apart (Part 0), so the
 * surface has no excuse not to.
 */
export function calmIsEarned(queue) {
  return (
    queue.projects > 0 &&
    queue.rows.length === 0 &&
    queue.silent.length === 0 &&
    queue.calm === queue.projects
  )
}
