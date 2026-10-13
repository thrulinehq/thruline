import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { foldSteps, placementSeq, transportView, unfoldable } from './lod.js'
import { objType } from './logic.js'

/* The level-of-detail model, pinned against a synthetic corpus.
 *
 * DESIGN-readable-trace §6.0 sets a gate: the target session draws far fewer
 * cards after the pass, and STEP must keep ambiguous results visible. Five other
 * sessions carry their own floors so a rule that only works on snapshots cannot
 * pass: two of the six have no snapshots at all, and on those R3 is the only
 * rule that pays.
 *
 * `lod-corpus.fixture.json` is invented data. It holds six sessions of the same
 * `{seq, kind, text, risk}` rows the UI receives over `/api/explain`, produced
 * by a seeded program from made-up event grammars: no real session, project,
 * path or transcript text is in it. See the fixture's own `note`. */

const CORPUS = JSON.parse(readFileSync(fileURLToPath(new URL('./lod-corpus.fixture.json', import.meta.url)), 'utf8'))

const dec = (s) => {
  const out = []
  for (const t of String(s).split(',')) out.push(parseInt(t, 36))
  return out
}

/** Rehydrate one fixture session into the transported `{seq, kind, text, risk}` rows. */
function eventsOf(row) {
  const deltas = dec(row.seqDelta)
  const kinds = dec(row.kind)
  const texts = dec(row.text)
  const risks = dec(row.risk)
  const out = []
  let seq = 0
  for (let i = 0; i < kinds.length; i++) {
    seq += deltas[i]
    const e = { seq, kind: CORPUS.kinds[kinds[i]], text: CORPUS.texts[texts[i]] }
    if (risks[i]) e.risk = CORPUS.risks[risks[i] - 1]
    out.push(e)
  }
  return out
}

/* The adapter's own event-node shape, reduced to the fields the LOD reads.
 * Kept in step with `adapter.buildNodes` by `the transform reads what the
 * adapter writes` below. */
function nodesOf(events) {
  return events.map((e) => {
    const n = { id: 'ev' + e.seq, objType: 'event', kind: e.kind, seq: e.seq, type: 'EVENT', sub: e.kind, title: String(e.text), lines: [], pinsL: [], pinsR: [], foot: 'seq ' + e.seq + ' · ' + e.kind }
    if (e.risk) {
      n.risk = true
      n.lines = [e.risk]
    }
    return n
  })
}

const SESSIONS = CORPUS.sessions.map((row) => {
  const events = eventsOf(row)
  const result = foldSteps(nodesOf(events), events)
  return { key: row.key, cards: row.cards, events, result }
})
const target = SESSIONS.find((s) => s.key === '5e5a0001')

/* The per-shape floors for the synthetic corpus: the card count each session
 * must stay under after the pass, set a little above what the rules reach so
 * the assertion has headroom. */
// Ambiguous concurrent calls stay separate. Readability comes from viewport
// windowing and sections; compression must not invent command/result bindings.
const FLOOR = { '5e5a0001': 1000, '5e5a0002': 1300, '5e5a0003': 1000, '5e5a0004': 950, '5e5a0005': 1200, '5e5a0006': 1100 }

/* ── T8 — the reduction ───────────────────────────────────────────────── */

test('T8 · the measured reduction: every session lands under its floor, and the target loses at least 40% of its cards', () => {
  const report = []
  SESSIONS.forEach((s) => {
    const before = s.cards
    const after = s.result.stats.after
    const pct = ((100 * (before - after)) / before).toFixed(1)
    report.push(`${s.key}  ${String(before).padStart(6)} → ${String(after).padStart(6)}  (−${pct}%)  floor ${FLOOR[s.key]}`)
  })
  console.log('\n  cards drawn, before and after the level-of-detail pass\n  ' + report.join('\n  ') + '\n')

  SESSIONS.forEach((s) => {
    assert.ok(s.result.stats.after <= FLOOR[s.key], `${s.key}: ${s.result.stats.after} cards is over the ${FLOOR[s.key]} floor`)
  })
  const cut = (target.cards - target.result.stats.after) / target.cards
  assert.ok(cut >= 0.40, `the target session lost only ${(100 * cut).toFixed(1)}% of its cards; the gate is 40%`)
})

test('T8 · R3 is the rule that pays on every session shape, including the two with no snapshots', () => {
  /* 5e5a0005 and 5e5a0006 are shaped like legacy imports and hold no snapshot at all, so
   * R1′ does nothing for them. If pairing ever stopped paying, these two would
   * be the first to say so. */
  SESSIONS.forEach((s) => {
    assert.ok(s.result.stats.paired > 0, `${s.key} paired nothing`)
  })
  const legacy = SESSIONS.filter((s) => s.key === '5e5a0005' || s.key === '5e5a0006')
  assert.equal(legacy.length, 2)
  legacy.forEach((s) => {
    assert.equal(s.result.stats.folded, 0, `${s.key} has no snapshots, so nothing may fold as one`)
    assert.ok(s.result.stats.after < s.cards, `${s.key} must still lose cards to pairing alone`)
  })
})

/* ── T9 — nothing dropped ─────────────────────────────────────────────── */

test('T9 · nothing is dropped: members plus ungrouped record events reconcile to the event count', () => {
  SESSIONS.forEach((s) => {
    const nodes = s.result.nodes
    const members = nodes.filter((n) => n.member).length
    const ungrouped = nodes.filter((n) => objType(n) === 'event' && !n.synthetic && !n.member).length
    assert.equal(members + ungrouped, s.cards, `${s.key}: ${members} + ${ungrouped} ≠ ${s.cards}`)
  })
})

test('T9 · every member is reachable from exactly one group, and every group member exists', () => {
  SESSIONS.forEach((s) => {
    const byId = new Map(s.result.nodes.map((n) => [n.id, n]))
    const claimed = new Map()
    s.result.nodes
      .filter((n) => n.group)
      .forEach((g) => {
        g.members.forEach((m) => {
          assert.ok(byId.has(m), `${s.key}: group ${g.id} names a member that does not exist`)
          assert.ok(!claimed.has(m), `${s.key}: member ${m} is claimed by two groups`)
          claimed.set(m, g.id)
          assert.equal(byId.get(m).member, g.id, `${s.key}: member ${m} does not point back at its group`)
        })
      })
    s.result.nodes.filter((n) => n.member).forEach((n) => assert.ok(claimed.has(n.id), `${s.key}: ${n.id} claims a group that does not list it`))
  })
})

/* ── T10 — the group card's copy ──────────────────────────────────────── */

test('T10 · every group states what it contains: a count with its noun, an aria label, a seq range, and a representative', () => {
  SESSIONS.forEach((s) => {
    const groups = s.result.nodes.filter((n) => n.group)
    assert.ok(groups.length > 0, `${s.key} produced no groups`)
    groups.forEach((g) => {
      assert.match(g.toggleLabel, /^[▸▾] \d+ [a-z]/, `${s.key}: ${g.id} has a toggle with no noun — "${g.toggleLabel}"`)
      assert.ok(g.toggleAria && g.toggleAria.length > 0, `${s.key}: ${g.id} has no toggle aria`)
      assert.equal(typeof g.firstSeq, 'number', `${s.key}: ${g.id} has no firstSeq`)
      assert.equal(typeof g.lastSeq, 'number', `${s.key}: ${g.id} has no lastSeq`)
      assert.ok(g.lastSeq >= g.firstSeq)
      assert.ok(g.rep && g.members.indexOf(g.rep) >= 0, `${s.key}: ${g.id} has no representative member`)
    })
  })
})

test('T10 · the snapshot group states the invariant first and the count second, and its seq range is both ends', () => {
  const g = target.result.nodes.find((n) => n.synthetic)
  assert.equal(g.title, 'Repeated code markers · captured on import')
  assert.equal(g.toggleLabel, '▸ 800 snapshot events')
  assert.match(g.lines[0], /^tree 7e3a9c01b5d2 · seq \d+–\d+ · taken at fold$/)
  assert.notEqual(g.firstSeq, g.lastSeq, 'seq is sparse and global; a range with one end is not a range')
})

test('T10 · a paired step names its result with a noun, and carries the recorded outcome off the title', () => {
  const step = target.result.nodes.find((n) => n.group && !n.synthetic)
  assert.equal(step.toggleLabel, '▸ 1 result')
  assert.equal(step.outcome, 'ok')
  assert.match(step.foot, /nearby result ok · inferred link$/)
  assert.equal(step.lastSeq - step.firstSeq >= 1, true)
})

/* ── T11 — NF, the never-fold rule ────────────────────────────────────── */

test('T11 · nothing on the never-fold list is ever a member, on any of the six sessions', () => {
  SESSIONS.forEach((s) => {
    s.result.nodes.forEach((n) => {
      if (!n.member) return
      assert.ok(!n.risk, `${s.key}: a risk-flagged node was folded away`)
      assert.ok(!n.severity && !n.finding, `${s.key}: a finding was folded away`)
      assert.ok(n.kind !== 'stop' && n.kind !== 'session_end', `${s.key}: an ending was folded away`)
      assert.equal(objType(n), 'event', `${s.key}: a derived object was event-folded`)
    })
  })
})

test('T11 · the target session keeps all 40 destructive commands and every non-success at the top level', () => {
  const risky = target.result.nodes.filter((n) => n.risk)
  assert.equal(risky.length, 40, 'the record flags 40 destructive commands in this session')
  risky.forEach((n) => assert.equal(n.member, undefined, 'a destructive command was folded away'))

  const nonSuccess = target.events.filter((e) => e.kind === 'tool_result' && !/ ok$/.test(e.text))
  assert.ok(nonSuccess.length > 0)
  const byId = new Map(target.result.nodes.map((n) => [n.id, n]))
  nonSuccess.forEach((e) => assert.equal(byId.get('ev' + e.seq).member, undefined, 'a non-success result was folded into an ok band'))

  target.result.nodes.filter((n) => n.kind === 'stop').forEach((n) => assert.equal(n.member, undefined))
})

test('T11 · a run splits around an unfoldable node — two runs, never one run with a hole', () => {
  const events = []
  for (let i = 0; i < 10; i++) events.push({ seq: 100 + i, kind: 'snapshot', text: 'tree aaaaaaaaaaaa (taken at fold)' })
  const nodes = nodesOf(events)
  nodes[5].risk = true

  const out = foldSteps(nodes, events)
  const groups = out.nodes.filter((n) => n.group)
  assert.equal(groups.length, 2, 'the risky node must split the run in two')
  assert.deepEqual(
    groups.map((g) => g.members.length).sort((a, b) => a - b),
    [4, 5],
  )
  assert.equal(out.nodes.find((n) => n.id === 'ev105').member, undefined, 'the risky node stays a card of its own')
  assert.equal(out.nodes.filter((n) => n.member).length, 9)
})

test('T11 · a run of one does not fold: an ellipsis that elides nothing is a lie about length', () => {
  const events = [
    { seq: 1, kind: 'snapshot', text: 'tree aaaaaaaaaaaa' },
    { seq: 2, kind: 'snapshot', text: 'tree bbbbbbbbbbbb' },
    { seq: 3, kind: 'snapshot', text: 'tree cccccccccccc' },
  ]
  const out = foldSteps(nodesOf(events), events)
  assert.equal(out.nodes.filter((n) => n.group).length, 0)
  assert.equal(out.stats.after, 3)
})

/* ── The pairing proof (§2.1 R3, §1.9 rule 5) ─────────────────────────── */

test('concurrent calls of the same tool never acquire each other’s result', () => {
  const events = [
    { seq: 1, kind: 'tool_call', text: 'Bash {"command":"slow-check"}' },
    { seq: 2, kind: 'tool_call', text: 'Bash {"command":"fast-check"}' },
    { seq: 3, kind: 'tool_result', text: 'Bash ok' },
    { seq: 4, kind: 'tool_result', text: 'Bash FAILED' },
  ]
  const out = foldSteps(nodesOf(events), events)
  assert.equal(out.stats.paired, 0)
  assert.ok(out.nodes.every(n => !n.member))
})

test('a result whose tool differs from the nearest preceding call, and is not adjacent by seq, does not pair', () => {
  const events = [
    { seq: 10, kind: 'tool_call', text: 'Bash …' },
    { seq: 14, kind: 'tool_result', text: 'Read ok' },
  ]
  const out = foldSteps(nodesOf(events), events)
  assert.equal(out.stats.paired, 0, 'an unprovable join is not a receipt')
  assert.equal(out.nodes.find((n) => n.id === 'ev14').member, undefined)
})

test('a result pairs on a matching tool name even when the seq gap is wide', () => {
  const events = [
    { seq: 10, kind: 'tool_call', text: 'Bash …' },
    { seq: 12, kind: 'snapshot', text: 'tree aaaaaaaaaaaa' },
    { seq: 19, kind: 'tool_result', text: 'Bash ok' },
  ]
  const out = foldSteps(nodesOf(events), events)
  assert.equal(out.stats.paired, 1)
  assert.equal(out.nodes.find((n) => n.id === 'ev19').member, 'ev10')
})

test('a result whose tool was not recorded pairs only on an adjacent seq, and never guesses', () => {
  const adjacent = [
    { seq: 10, kind: 'tool_call', text: 'Bash …' },
    { seq: 11, kind: 'tool_result', text: ' ok' },
  ]
  assert.equal(foldSteps(nodesOf(adjacent), adjacent).stats.paired, 1)

  const apart = [
    { seq: 10, kind: 'tool_call', text: 'Bash …' },
    { seq: 13, kind: 'tool_result', text: ' ok' },
  ]
  assert.equal(foldSteps(nodesOf(apart), apart).stats.paired, 0)
})

test('the outcome slot keeps three values, never two: a silence is not a success', () => {
  const events = [
    { seq: 10, kind: 'tool_call', text: 'Bash …' },
    { seq: 11, kind: 'tool_result', text: 'Bash outcome not stated' },
  ]
  const out = foldSteps(nodesOf(events), events)
  assert.equal(out.stats.paired, 0, '"outcome not stated" is not "ok" and must keep its own row')
  assert.equal(transportView(events[1]).outcome, 'not_stated')
  assert.equal(transportView({ seq: 1, kind: 'tool_result', text: 'Bash FAILED' }).outcome, 'failed')
  assert.equal(transportView({ seq: 1, kind: 'tool_result', text: 'Bash something else' }).outcome, null)
})

/* ── Determinism (§1.9 rule 7) ────────────────────────────────────────── */

test('the transform is deterministic and does not write through its arguments', () => {
  const events = target.events.slice(0, 400)
  const a = nodesOf(events)
  const b = nodesOf(events)
  const ra = foldSteps(a, events)
  const rb = foldSteps(b, events)
  assert.deepEqual(
    ra.nodes.map((n) => [n.id, n.member || '', n.group ? n.toggleLabel : '']),
    rb.nodes.map((n) => [n.id, n.member || '', n.group ? n.toggleLabel : '']),
  )
  assert.ok(
    a.every((n) => n.member === undefined && n.group === undefined),
    'the caller‘s node array must come back untouched',
  )

  const source = readFileSync(fileURLToPath(new URL('./lod.js', import.meta.url)), 'utf8')
  assert.ok(!/toLocaleString|Intl\.|Math\.random|new Date/.test(source), 'a view that cannot be regenerated byte-for-byte is not a receipt')
})

/* ── The order pins §2.3 requires ─────────────────────────────────────── */

test('every event card carries the order pins the recorded-order wires need', () => {
  const nodes = target.result.nodes.filter((n) => objType(n) === 'event')
  nodes.forEach((n) => {
    assert.ok(n.pinsL.some((p) => p.id === 'i'), `${n.id} has no incoming order pin`)
    assert.ok(n.pinsR.some((p) => p.id === 'o'), `${n.id} has no outgoing order pin`)
  })
})

/* ── The view the transform reads ─────────────────────────────────────── */

test('the structural view is read from the record, never from a rendered title', () => {
  assert.deepEqual(transportView({ seq: 3, kind: 'snapshot', text: 'tree 7e3a9c01b5d2 (taken at fold)' }), {
    seq: 3,
    kind: 'snapshot',
    tool: null,
    outcome: null,
    tree: '7e3a9c01b5d2',
    atFold: true,
    risk: false,
  })
  assert.equal(transportView({ seq: 1, kind: 'tool_call', text: 'Bash {"command":"cat -n crates/policy…' }).tool, 'Bash')
  assert.equal(transportView({ seq: 1, kind: 'tool_call', text: 'mcp__Claude_Browser__navigate {…' }).tool, 'mcp__Claude_Browser__navigate')
  assert.equal(transportView({ seq: 1, kind: 'snapshot', text: 'tree' }).tree, null)
})

test('placement puts a group at its first member, so a span is never sorted to the end of its column', () => {
  const g = target.result.nodes.find((n) => n.synthetic)
  assert.equal(placementSeq(g), g.firstSeq)
  assert.equal(placementSeq({ seq: 7 }), 7)
  assert.equal(placementSeq({}), null)
})

test('unfoldable names every clause the record can answer, and none it cannot', () => {
  assert.equal(unfoldable({ objType: 'claim' }), true)
  assert.equal(unfoldable({ objType: 'event', kind: 'snapshot' }), false)
  assert.equal(unfoldable({ objType: 'event', kind: 'tool_call', risk: true }), true)
  assert.equal(unfoldable({ objType: 'event', kind: 'stop' }), true)
  assert.equal(unfoldable({ objType: 'event', kind: 'tool_result' }, { outcome: 'failed' }), true)
  assert.equal(unfoldable({ objType: 'event', kind: 'tool_result' }, { outcome: 'ok' }), false)
  assert.equal(unfoldable({ objType: 'event', kind: 'tool_call', word: 'not_proven' }), true)
})
