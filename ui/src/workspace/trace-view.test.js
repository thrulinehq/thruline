/* P6-3 — the Trace's pure derivations (trace-view.js).
 *
 * Every fixture is synthetic: `trace-fixture.js` adapted through the real
 * adapter, plus small hand-made node and wire lists. One test per rule the
 * card states:
 *  - four lanes, from object type or event kind, never from a title;
 *  - three line kinds read off the wire's own `kind`, and a fourth that is
 *    its own legend entry — never "inferred" — when the record has one;
 *  - the thread: one card per stage, the nearest, over reference and
 *    inferred links only, never through the signed record;
 *  - the guided walk never wraps;
 *  - where a run ran, from typed fields only, every gap named;
 *  - the Plain face never shows seq, ids or full hashes.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { HQ_COPY } from './hq-copy.js'
import { adaptSession } from './adapter.js'
import { estH } from './logic.js'
import { TRACE_DOCS } from './trace-fixture.js'
import {
  EDGE_KINDS, EDGE_STYLE, LANES, LANE_BY_KIND, LANE_BY_OBJECT, cardHeight, discrepancyAnchor, edgeKindOf, extraLines,
  factsLines, laneBasis, laneLayout, laneOf, legendEntries, outlineRows, plainFace, plainText, recordOrder, runFacts,
  stageOf, stepLabel, threadOf, walkOrder, walkStep,
} from './trace-view.js'

const T = HQ_COPY.trace
const S = adaptSession(TRACE_DOCS)
const byObj = (t) => S.nodes.filter((n) => n.objType === t)
const ev = (seq) => S.nodes.find((n) => n.seq === seq)
const claim = byObj('claim')[0]
const finding = byObj('finding')[0]
const [chk1, chk2] = byObj('check')
const BANNED_PLAIN = /\bseq\b|clm_|chk_|[0-9a-f]{40}|session id/

/* ── lanes ─────────────────────────────────────────────────────────────── */

test('four lanes, top to bottom, and every kind and object the adapter makes has one', () => {
  assert.deepEqual([...LANES], ['request', 'work', 'execution', 'claims'])
  assert.deepEqual(LANES.map((l) => T.lanes[l]), ['Request', 'Work', 'Execution', 'Claims & evidence'])
  const expect = {
    session_start: 'request', user_prompt: 'request', fs_change: 'work', snapshot: 'work', note: 'work',
    tool_call: 'execution', tool_result: 'execution', exec: 'execution', stop: 'claims',
  }
  assert.deepEqual({ ...LANE_BY_KIND }, expect)
  assert.deepEqual({ ...LANE_BY_OBJECT }, { ask: 'request', check: 'execution', claim: 'claims', promise: 'claims', finding: 'claims', receipt: 'claims', ending: 'claims' })
  for (const n of S.nodes) assert.ok(LANES.includes(laneOf(n)), n.id)
  for (const [seq, lane] of [[1, 'request'], [2, 'request'], [3, 'execution'], [5, 'work'], [6, 'work'], [7, 'execution'], [11, 'work'], [12, 'claims']]) {
    assert.equal(laneOf(ev(seq)), lane, `seq ${seq}`)
  }
})

test('a lane is read from object type or kind — never from the title — and an unknown kind is placed in Work and says so', () => {
  assert.equal(laneOf({ id: 'e1', objType: 'event', kind: 'exec', title: 'Request: please' }), 'execution')
  assert.deepEqual(laneBasis({ id: 'e2', objType: 'event', kind: 'teleport', title: 'Command ran' }), { lane: 'work', basis: 'fallback' })
  assert.deepEqual(laneBasis({ id: 'd1', frame: 'request', title: 'x' }), { lane: 'request', basis: 'frame' })
  /* a Focus group sits with its members when they agree, else by category */
  assert.equal(laneOf({ id: 'g1', objType: 'group', kind: 'other', memberKinds: ['user_prompt', 'session_start'] }), 'request')
  assert.equal(laneOf({ id: 'g2', objType: 'group', kind: 'holds', memberKinds: ['tool_call', 'user_prompt'] }), 'execution')
})

test('the lane layout reads left to right in record order: X strictly increases, a lane never overlaps itself, all four lanes are drawn', () => {
  const L = laneLayout(S.nodes, { heightOf: (n, w) => cardHeight(n, w, S) })
  assert.equal(L.frames.length, 4)
  assert.deepEqual(L.frames.map((f) => f.lane), [...LANES])
  const order = recordOrder(S.nodes)
  const xs = order.map((id) => L.pos.get(id).x)
  for (let i = 1; i < xs.length; i++) assert.ok(xs[i] > xs[i - 1], `${order[i]} is not right of ${order[i - 1]}`)
  for (const lane of LANES) {
    const band = L.lanes.find((l) => l.id === lane)
    const cards = order.filter((id) => laneOf(S.nodes.find((n) => n.id === id)) === lane).map((id) => L.pos.get(id))
    for (let i = 1; i < cards.length; i++) assert.ok(cards[i].x >= cards[i - 1].x + cards[i - 1].w, `${lane} overlaps itself`)
    for (const c of cards) assert.ok(c.y > band.y && c.y < band.y + band.h, `${lane}: a card sits outside its band`)
  }
  /* derived cards the record does not place come after every placed card */
  const lastPlaced = Math.max(...S.nodes.filter((n) => n.seq != null).map((n) => L.pos.get(n.id).x))
  assert.ok(L.unplacedX > lastPlaced)
  assert.ok(L.pos.get(claim.id).x >= L.unplacedX)
})

/* ── line kinds ────────────────────────────────────────────────────────── */

test('three line kinds come from the wire\'s own kind; anything else is its own fourth entry, never "inferred"', () => {
  assert.equal(edgeKindOf({ kind: 'order' }), 'order')
  assert.equal(edgeKindOf({ kind: 'ref' }), 'reference')
  assert.equal(edgeKindOf({ kind: 'inf' }), 'inferred')
  for (const k of ['xref', 'reference', 'inferred', undefined, '']) assert.equal(edgeKindOf({ kind: k }), 'unclassified', String(k))
  assert.deepEqual(legendEntries([{ kind: 'ref' }, { kind: 'order' }]).map((e) => e.kind), ['order', 'reference', 'inferred'])
  assert.deepEqual(legendEntries([]).map((e) => e.label), [T.edges.order, T.edges.reference, T.edges.inferred], 'the legend is permanent: three entries even with no lines')
  const four = legendEntries([{ kind: 'ref' }, { kind: 'xref' }])
  assert.deepEqual(four.map((e) => e.kind), ['order', 'reference', 'inferred', 'unclassified'])
  assert.equal(four[3].label, T.edges.unclassified)
})

test('no line kind is told apart by colour alone: each has its own stroke pattern', () => {
  const dashes = EDGE_KINDS.map((k) => EDGE_STYLE[k].dash)
  assert.equal(new Set(dashes).size, EDGE_KINDS.length, dashes.join(' | '))
  assert.equal(EDGE_STYLE.order.dash, 'none', 'recorded order is solid')
  assert.match(EDGE_STYLE.reference.dash, /^\d+ \d+$/, 'documented reference is dashed')
  assert.equal(EDGE_STYLE.inferred.cap, 'round', 'inferred is dotted (round caps on a short dash)')
})

/* ── the thread ────────────────────────────────────────────────────────── */

test('the claim\'s thread: one card per stage, nearest first, in stage order, and the stage the record links nothing to is named', () => {
  const t = threadOf(S, claim.id)
  assert.ok(t)
  assert.deepEqual(t.steps.map((s) => s.stage), ['statement', 'change', 'run', 'finding'])
  assert.deepEqual(t.ids, [claim.id, ev(9).id, chk1.id, finding.id])
  assert.deepEqual(t.missing, ['request'])
  for (const s of t.steps.slice(1)) assert.ok(s.via && ['reference', 'inferred'].includes(s.via.kind), s.id)
  assert.equal(new Set(t.steps.map((s) => s.stage)).size, t.steps.length, 'one card per stage')
})

test('the thread never walks through the signed record, which joins every claim and check', () => {
  const t = threadOf(S, claim.id)
  assert.ok(!t.ids.includes(chk2.id), 'the second check is linked to the claim only through the signed record')
  assert.ok(!t.ids.includes(byObj('receipt')[0].id))
  /* the second check reaches the claim only through the signed record, so
   * its thread is the version it ran on and itself — never the claim */
  const t2 = threadOf(S, chk2.id)
  assert.deepEqual(t2.ids, [ev(10).id, chk2.id])
  assert.ok(!t2.ids.includes(claim.id))
  /* selected, the signed record is on its own thread as the first card */
  assert.equal(threadOf(S, byObj('receipt')[0].id).anchor, byObj('receipt')[0].id)
})

test('the thread walks only documented-reference and inferred links — never recorded order, never an unknown kind', () => {
  const nodes = [{ id: 'a', objType: 'claim' }, { id: 'b', objType: 'check' }, { id: 'c', objType: 'finding', sub: 'Finding · fail' }]
  const via = (kind) => threadOf({ nodes, wires: [{ id: 'w1', from: ['b', 'x'], to: ['a', 'x'], kind }] }, 'a')
  assert.equal(via('order'), null)
  assert.equal(via('xref'), null)
  assert.deepEqual(via('ref').ids, ['a', 'b'])
  assert.deepEqual(via('inf').ids, ['a', 'b'])
  assert.equal(threadOf({ nodes, wires: [] }, 'a'), null, 'a card linked to nothing has no thread')
  assert.equal(threadOf(S, 'no-such-card'), null)
})

test('a finding picks its own stage; among findings the worst and nearest wins', () => {
  const nodes = [
    { id: 'c', objType: 'claim' },
    { id: 'f1', objType: 'finding', sub: 'Finding · info' },
    { id: 'f2', objType: 'finding', sub: 'Finding · fail' },
  ]
  const wires = [{ id: 'w1', from: ['f1', 'x'], to: ['c', 'x'], kind: 'ref' }, { id: 'w2', from: ['f2', 'x'], to: ['c', 'x'], kind: 'ref' }]
  assert.deepEqual(threadOf({ nodes, wires }, 'c').ids, ['c', 'f2'])
  assert.deepEqual(threadOf({ nodes, wires }, 'f1').ids, ['c', 'f1'], 'the selected card keeps its own stage')
  assert.equal(stageOf(nodes[0]), 'statement')
})

test('the current discrepancy is the worst claim', () => {
  const nodes = [{ id: 'p', objType: 'claim', word: 'proven' }, { id: 'n', objType: 'claim', word: 'not_proven' }]
  assert.equal(discrepancyAnchor({ nodes }), 'n')
  assert.equal(discrepancyAnchor({ nodes: [] }), null)
  assert.equal(discrepancyAnchor(S), claim.id)
})

/* ── the walk ──────────────────────────────────────────────────────────── */

test('the walk goes along the thread when one is lit, else record order, and never wraps', () => {
  const t = threadOf(S, claim.id)
  assert.deepEqual(walkOrder(t, S.nodes), { along: 'thread', ids: t.ids })
  const rec = walkOrder(null, S.nodes)
  assert.equal(rec.along, 'record')
  assert.deepEqual(rec.ids.slice(0, 3), [ev(1).id, ev(2).id, ev(3).id])
  const ids = ['a', 'b', 'c']
  assert.deepEqual(walkStep(ids, null, 1), { id: 'a', index: 0, total: 3, edge: null })
  assert.deepEqual(walkStep(ids, 'a', 1), { id: 'b', index: 1, total: 3, edge: null })
  assert.deepEqual(walkStep(ids, 'c', 1), { id: 'c', index: 2, total: 3, edge: 'end' })
  assert.deepEqual(walkStep(ids, 'a', -1), { id: 'a', index: 0, total: 3, edge: 'start' })
  assert.equal(walkStep([], 'a', 1), null)
})

/* ── where a run ran ───────────────────────────────────────────────────── */

test('a check says where it ran from typed fields: exit code, counts, version — and the band when the record says it ran on an earlier version', () => {
  const f = runFacts(chk1, S)
  assert.deepEqual(f.rows.map((r) => [r.key, r.value]), [
    ['exit', '1'],
    ['counts', '3 passed · 1 failed · 0 skipped · 4 in total'],
    ['version', '1111111'],
  ])
  assert.deepEqual(f.absent, ['command', 'duration', 'cwd'])
  assert.deepEqual(f.band, { ran: '1111111', final: '2222222' })
  const plain = factsLines(f, 'plain')
  assert.equal(plain.band, T.run.earlier)
  assert.equal(plain.absentLine, 'Not recorded: command, how long, where it ran')
  const tech = factsLines(f, 'technical')
  assert.match(tech.rows[0].value, /^argv hash sha256:/, 'Technical names the argv hash the record carries in the command row')
  assert.equal(tech.rows.find((r) => r.label === T.run.version).value, 'tree ' + '1'.repeat(40))
})

test('a check with no exit code is hollow and says so; a missing count is absent, never zero', () => {
  const f = runFacts(chk2, S)
  assert.equal(f.hollow, true)
  assert.deepEqual(f.rows.map((r) => r.key), ['version'])
  assert.ok(f.absent.includes('exit') && f.absent.includes('counts'))
  assert.equal(f.band, null, 'it ran on the last version')
  assert.equal(factsLines(f, 'plain').hollowLine, T.run.no_result)
  /* counts only from numbers the record carries */
  const s2 = { ...S, checks: [{ node: 'k', parsed: { passed: 2 }, exit: 0 }] }
  assert.deepEqual(runFacts({ id: 'k', objType: 'check' }, s2).rows.find((r) => r.key === 'counts').value, '2 passed')
})

test('a command step names its command only from the typed input field — never from its title or text', () => {
  assert.deepEqual(runFacts(ev(7), S).rows, [{ key: 'command', value: 'npm test' }])
  const s2 = { ...S, events: [{ seq: 99, kind: 'exec', text: 'cargo test --release' }] }
  const f = runFacts({ id: 'ev99', objType: 'event', kind: 'exec', seq: 99, title: 'cargo test · exit 0' }, s2)
  assert.deepEqual(f.rows, [])
  assert.deepEqual(f.absent, ['command', 'exit', 'counts', 'duration', 'cwd', 'version'])
  assert.equal(runFacts(ev(5), S), null, 'a file change is not a run')
})

test('a card is as tall as its tallest face plus the lines the Trace adds, whichever face shows', () => {
  for (const n of S.nodes) {
    const h = cardHeight(n, 260, S)
    assert.ok(h >= estH(n, 260), n.id)
    const x = extraLines(n, S)
    if (x) assert.ok(h >= estH(n, 260) + x * 16, n.id)
  }
  assert.equal(extraLines(claim, S), 1, 'the agent-reported line')
})

/* ── the Plain face ────────────────────────────────────────────────────── */

test('the Plain face of every card carries no seq, no id, no full hash and no session id', () => {
  for (const n of S.nodes) {
    const p = plainFace(n)
    for (const [k, v] of Object.entries(p)) assert.doesNotMatch(String(v), BANNED_PLAIN, `${n.id}.${k}: ${v}`)
  }
  assert.equal(plainText('seq 12–40 · tree ' + 'a'.repeat(40) + ' · chk_ABC'), 'steps 12–40 · version aaaaaaa')
  assert.equal(stepLabel(ev(9)), 'step 9')
  assert.equal(plainFace(finding).title, HQ_COPY.detectors.tests_ran_before_final_edit)
  assert.equal(plainFace(claim).title, claim.title, 'the agent\'s words are quoted as said')
})

test('the Outline lists the drawn cards in record order with lane words and line-kind words', () => {
  const rows = outlineRows(S, S.nodes, 'plain')
  assert.equal(rows.length, S.nodes.length)
  assert.equal(rows[0].laneLabel, 'Request')
  const c = rows.find((r) => r.id === claim.id)
  assert.ok(c.links.some((l) => l.word === T.edges.reference))
  for (const r of rows) assert.doesNotMatch(JSON.stringify(r).replace(/"(id|to|lane|kind)":"[^"]*"/g, ''), BANNED_PLAIN, r.id)
})
