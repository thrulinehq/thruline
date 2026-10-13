import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { chainOf, focusSet, focusView, isSeed } from './focus-set.js'

/* The Trace Focus set (HQ-VERDICT-PANEL-PLAN Step 5), pinned on fixture graphs
 * in the adapter's own shapes: nodes `{id, objType, kind, word, sub, risk}`
 * (`adapter.js:876-1043`) and wires `{id, from:[node,pin], to:[node,pin],
 * kind:'ref'|'inf'|'order'}` (`adapter.js:1052-1119`). */

const ev = (seq, kind, extra) => ({ id: 'ev' + seq, objType: 'event', kind, seq, title: kind + ' ' + seq, lines: [], pinsL: [], pinsR: [], ...extra })
const claim = (id, word) => ({ id, objType: 'claim', word, sub: 'Claim · done', title: '“' + id + '”' })
const check = (id) => ({ id, objType: 'check', sub: 'Check · test', title: 'cargo test · exit 0' })
const finding = (id, severity) => ({ id, objType: 'finding', finding: 'k_' + id, sub: 'Finding · ' + severity, title: 'k_' + id })
const promise = (id, word) => ({ id, objType: 'promise', word, sub: 'Promise · later' })
const ask = (id, word) => ({ id, objType: 'ask', word, sub: 'Ask · request' })
const receipt = () => ({ id: 'rcpt', objType: 'receipt', sub: 'DSSE', title: 'Receipt intact' })
const wire = (id, from, to, kind) => ({ id, from: [from, 'a'], to: [to, 'b'], kind })

/* Fixture 1 — one not-proven claim with a four-link chain, plus proven noise.
 *
 *   c1 (not_proven) ← k1 (ref) ← ev1 exec (inf) ← ev2 fs_change (ref) ← ev3 fs_change (ref)
 *
 * Depth 3 from c1 reaches ev2 and stops; ev3 sits one hop too far. The proven
 * claim c2, its check k2 and the receipt are wired to each other and to nothing
 * on the chain; ev9/ev10 are an unwired call and result; ev11 a snapshot. */
function chainFixture() {
  const nodes = [
    ev(1, 'exec'),
    ev(2, 'fs_change', { file: 'src/a.rs' }),
    ev(3, 'fs_change', { file: 'src/b.rs' }),
    ev(9, 'tool_call'),
    ev(10, 'tool_result'),
    ev(11, 'snapshot', { title: 'tree 7e3a9c01b5d2', lines: ['tree 7e3a9c01b5d2'] }),
    claim('c1', 'not_proven'),
    claim('c2', 'proven'),
    check('k1'),
    check('k2'),
    receipt(),
  ]
  const wires = [
    wire('w1', 'k1', 'c1', 'ref'),
    wire('w2', 'ev1', 'k1', 'inf'),
    wire('w3', 'ev2', 'ev1', 'ref'),
    wire('w4', 'ev3', 'ev2', 'ref'),
    wire('w5', 'k2', 'c2', 'ref'),
    wire('w6', 'c2', 'rcpt', 'ref'),
    wire('w7', 'k2', 'rcpt', 'ref'),
  ]
  return { nodes, wires }
}

test('1 · the kept set at depth 3 is exactly the seed and its three-hop chain; depth 1 is smaller', () => {
  const { nodes, wires } = chainFixture()
  const d3 = focusSet(nodes, wires, { depth: 3 })
  assert.deepEqual(d3.seeds, ['c1'])
  assert.deepEqual(d3.kept, ['c1', 'ev1', 'ev2', 'k1'])
  assert.deepEqual(d3.depthOf, { c1: 0, k1: 1, ev1: 2, ev2: 3 })

  const d1 = focusSet(nodes, wires, { depth: 1 })
  assert.deepEqual(d1.kept, ['c1', 'k1'])
  assert.ok(d1.kept.length < d3.kept.length)

  const dflt = focusSet(nodes, wires)
  assert.deepEqual(dflt.kept, d3.kept, 'the default depth is 3')

  const view = focusView(nodes, wires)
  assert.equal(view.mode, 'focus')
  assert.deepEqual(view.shown, ['ev1', 'ev2', 'c1', 'k1'], 'shown keeps the original node order, not the sorted one')
  assert.deepEqual(view.summary, { shown: 4, total: 11, merged: 7, allClear: false, degraded: false })
  assert.deepEqual(
    view.groups.map((g) => [g.kind, g.label, g.members]),
    [
      ['holds', '2 events that hold up', ['ev9', 'ev10']],
      ['fs', '1 file change', ['ev3']],
      ['trees', '1 tree recorded', ['ev11']],
      ['record', '3 for the record', ['c2', 'k2', 'rcpt']],
    ],
  )
  assert.equal(view.groups[2].line, 'tree 7e3a9c01b5d2')
  assert.equal(view.groups[0].line, 'reads, edits and runs with nothing against them')
  assert.equal(view.groups[3].line, 'proven claims and the checks behind them')
  const accounted = view.shown.length + view.groups.reduce((s, g) => s + g.count, 0)
  assert.equal(accounted, nodes.length, 'every node is either shown or counted')
})

test('2 · order wires never extend the set', () => {
  const nodes = [claim('c1', 'not_proven'), ev(1, 'exec'), ev(2, 'exec'), check('k1')]
  const wires = [
    wire('o1', 'c1', 'ev1', 'order'),
    wire('o2', 'ev1', 'ev2', 'order'),
    wire('r1', 'k1', 'c1', 'ref'),
  ]
  const set = focusSet(nodes, wires, { depth: 5 })
  assert.deepEqual(set.kept, ['c1', 'k1'])
  assert.deepEqual(chainOf(nodes, wires, 'c1', 5), ['c1', 'k1'])
  const orderOnly = focusSet(nodes, wires.filter((w) => w.kind === 'order'))
  assert.deepEqual(orderOnly.kept, ['c1'], 'with only chronology wires, a seed keeps only itself')
})

test('3 · an all-Proven record shows nothing and merges everything into one "Everything holds up" card', () => {
  const nodes = [ev(1, 'exec'), ev(2, 'fs_change'), claim('c1', 'proven'), check('k1'), receipt(), finding('f1', 'info'), promise('p1', 'kept'), ask('a1', 'kept')]
  const wires = [wire('w1', 'k1', 'c1', 'ref'), wire('w2', 'c1', 'rcpt', 'ref')]
  const set = focusSet(nodes, wires)
  assert.deepEqual(set.seeds, [])
  assert.deepEqual(set.kept, [])

  const view = focusView(nodes, wires)
  assert.deepEqual(view.shown, [])
  assert.equal(view.groups.length, 1)
  assert.equal(view.groups[0].label, 'Everything holds up')
  assert.equal(view.groups[0].count, nodes.length)
  assert.deepEqual(view.groups[0].members, nodes.map((n) => n.id))
  assert.deepEqual(view.summary, { shown: 0, total: nodes.length, merged: nodes.length, allClear: true, degraded: false })
})

test('3b · the `degraded` option: no seeds over a degraded capture is NOT "Everything holds up"', () => {
  /* ADJUDICATIONS item C, loose thread: a Fog session with nothing extracted
   * has no seed, and a confident phrase over a record that was not fully
   * watched would be the product's own failure. */
  const nodes = [ev(1, 'exec'), ev(2, 'fs_change'), check('k1'), receipt()]
  const clear = focusView(nodes, [])
  assert.equal(clear.summary.allClear, true)
  assert.equal(clear.summary.degraded, false)
  assert.equal(clear.groups[0].label, 'Everything holds up')

  const fog = focusView(nodes, [], { degraded: true })
  assert.equal(fog.summary.allClear, false, 'degraded and seedless is not all clear')
  assert.equal(fog.summary.degraded, true)
  assert.equal(fog.groups.length, 1, 'still one group card')
  assert.equal(fog.groups[0].kind, 'all')
  assert.equal(fog.groups[0].id, 'grp_focus_all')
  assert.notEqual(fog.groups[0].label, 'Everything holds up')
  assert.equal(fog.groups[0].label, '4 steps recorded', 'the module words the count only; the caller words the gap')
  assert.deepEqual(fog.groups[0].members, nodes.map((n) => n.id), 'every node is still accounted for')
  assert.deepEqual({ shown: fog.summary.shown, total: fog.summary.total, merged: fog.summary.merged }, { shown: 0, total: 4, merged: 4 })

  /* With a seed present, `degraded` changes nothing about the set — the walk
   * is over the record's wires, not over the recorder's condition. */
  const { nodes: cn, wires: cw } = chainFixture()
  const a = focusView(cn, cw)
  const b = focusView(cn, cw, { degraded: true })
  assert.deepEqual(a.shown, b.shown)
  assert.deepEqual(a.groups, b.groups)
  assert.equal(a.summary.allClear, false, 'a record with a seed is never all clear')
  assert.equal(b.summary.degraded, true)

  assert.equal(focusView([], [], { degraded: true }).groups.length, 0, 'zero nodes still yields no card')
})

test('4 · group labels carry the count with its noun, singular and plural', () => {
  /* One seed with no wires, so every other node merges by category. */
  const one = [claim('c1', 'couldnt'), ev(1, 'exec'), ev(2, 'fs_change'), ev(3, 'snapshot'), check('k1'), promise('p1', 'kept')]
  const singular = focusView(one, [])
  assert.deepEqual(
    singular.groups.map((g) => g.label),
    ['1 event that holds up', '1 file change', '1 tree recorded', '1 for the record', '1 other step'],
  )
  assert.equal(singular.groups[2].line, 'recorded code checkpoints', 'a snapshot with no tree id in its text falls back to the generic line')

  const many = [
    claim('c1', 'fog'),
    ev(1, 'exec'), ev(2, 'tool_call'), ev(3, 'tool_result'),
    ev(4, 'fs_change'), ev(5, 'fs_change'),
    ev(6, 'snapshot', { title: 'tree aaaaaaaaaaaa' }), ev(7, 'snapshot', { title: 'tree bbbbbbbbbbbb' }), ev(8, 'snapshot', { title: 'tree cccccccccccc' }), ev(9, 'snapshot', { title: 'tree dddddddddddd' }),
    claim('c2', 'proven'), check('k1'), receipt(),
    promise('p1', 'kept'), ev(10, 'exec', { risk: true }),
  ]
  const plural = focusView(many, [])
  assert.deepEqual(
    plural.groups.map((g) => [g.label, g.count]),
    [['3 events that hold up', 3], ['2 file changes', 2], ['4 trees recorded', 4], ['3 for the record', 3], ['2 other steps', 2]],
  )
  assert.equal(plural.groups[2].line, 'tree aaaaaaaaaaaa · tree bbbbbbbbbbbb · tree cccccccccccc', 'at most three tree ids are listed')
  assert.deepEqual(plural.groups[4].members, ['p1', 'ev10'], 'a risk-flagged exec never "holds up"')
})

test('5 · the same input twice is deep-equal, and the source names no clock, locale or random', () => {
  const a = chainFixture()
  const b = chainFixture()
  assert.deepEqual(focusSet(a.nodes, a.wires), focusSet(b.nodes, b.wires))
  assert.deepEqual(focusView(a.nodes, a.wires), focusView(b.nodes, b.wires))
  assert.deepEqual(chainOf(a.nodes, a.wires, 'c1'), chainOf(b.nodes, b.wires, 'c1'))
  assert.deepEqual(a, chainFixture(), 'the caller‘s arrays are not written through')

  const source = readFileSync(fileURLToPath(new URL('./focus-set.js', import.meta.url)), 'utf8')
  assert.ok(!/toLocaleString|Intl\.|Math\.random|new Date|Date\.now/.test(source), 'a view that cannot be regenerated byte-for-byte is not a receipt')
})

test('6 · dangling wire endpoints are skipped, never thrown on', () => {
  const nodes = [claim('c1', 'not_proven'), check('k1')]
  const wires = [
    wire('w1', 'k1', 'c1', 'ref'),
    wire('w2', 'ghost', 'c1', 'ref'),
    wire('w3', 'c1', 'nobody', 'inf'),
    { id: 'w4', kind: 'ref' },
    { id: 'w5', from: ['c1'], kind: 'ref' },
    null,
  ]
  const set = focusSet(nodes, wires)
  assert.deepEqual(set.kept, ['c1', 'k1'])
  assert.deepEqual(chainOf(nodes, wires, 'c1'), ['c1', 'k1'])
  assert.deepEqual(chainOf(nodes, wires, 'ghost'), [], 'a chain from a node that does not exist is empty')

  /* Nodes lacking fields, and a null node, are tolerated too. */
  const sparse = [{ id: 'x' }, { objType: 'claim', word: 'not_proven' }, null, claim('c9', 'not_proven')]
  const view = focusView(sparse, wires)
  assert.deepEqual(view.shown, ['c9'])
  assert.deepEqual(view.summary, { shown: 1, total: 2, merged: 1, allClear: false, degraded: false })
})

test('7 · chainOf returns the seed first, then its chain in BFS order, bounded by depth', () => {
  const { nodes, wires } = chainFixture()
  assert.deepEqual(chainOf(nodes, wires, 'c1', 3), ['c1', 'k1', 'ev1', 'ev2'])
  assert.deepEqual(chainOf(nodes, wires, 'c1', 1), ['c1', 'k1'])
  assert.deepEqual(chainOf(nodes, wires, 'c1', 0), ['c1'])
  assert.deepEqual(chainOf(nodes, wires, 'c1'), ['c1', 'k1', 'ev1', 'ev2'], 'the default depth is 3')
  assert.equal(chainOf(nodes, wires, 'ev2')[0], 'ev2', 'any node can head a chain; the walk is undirected')
})

test('8 · zero nodes yields an empty view without throwing', () => {
  assert.deepEqual(focusSet([], []), { seeds: [], kept: [], depthOf: {} })
  assert.deepEqual(focusView([], []), { mode: 'focus', shown: [], groups: [], summary: { shown: 0, total: 0, merged: 0, allClear: true, degraded: false } })
  assert.deepEqual(chainOf([], [], 'c1'), [])
  assert.deepEqual(focusView(undefined, undefined).summary, { shown: 0, total: 0, merged: 0, allClear: true, degraded: false })
  assert.deepEqual(focusSet(null, null).kept, [])
})

test('the seed predicate names every state short of Proven, and no other', () => {
  assert.equal(isSeed(claim('c', 'not_proven')), true)
  assert.equal(isSeed(claim('c', 'couldnt')), true)
  assert.equal(isSeed(claim('c', 'fog')), true)
  assert.equal(isSeed(claim('c', 'proven')), false)
  assert.equal(isSeed(promise('p', 'loose')), true)
  assert.equal(isSeed(promise('p', 'kept')), false)
  assert.equal(isSeed(finding('f', 'fail')), true)
  assert.equal(isSeed(finding('f', 'warn')), true)
  assert.equal(isSeed(finding('f', 'info')), false)
  assert.equal(isSeed(ask('a', 'not_proven')), true)
  assert.equal(isSeed(ask('a', 'loose')), true)
  assert.equal(isSeed(ask('a', 'broken')), true)
  assert.equal(isSeed(ask('a', 'kept')), false)
  assert.equal(isSeed(ev(1, 'exec', { risk: true })), false, 'a risk flag is a fact about an event, not a state short of Proven')
  assert.equal(isSeed(check('k')), false)
  assert.equal(isSeed(null), false)
})
