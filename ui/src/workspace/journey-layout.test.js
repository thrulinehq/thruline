import test from 'node:test'
import assert from 'node:assert/strict'
import { journeyLayout } from './journey-layout.js'
import { estH } from './logic.js'

const make = count => Array.from({ length: count }, (_, i) => ({ id: `ev${i}`, seq: i, w: 260, title: 'Recorded action', lines: i % 4 === 0 ? ['One detail', 'Another detail'] : [], kind: 'tool_call', tier: 'event' }))

test('ten thousand events keep their identity and a bounded reading height', () => {
  const nodes = make(10000), layout = journeyLayout(nodes, () => true)
  assert.equal(layout.pos.size, nodes.length); assert.equal(layout.frames.length, Math.ceil(nodes.length / 12))
  assert.equal(layout.frames[0].firstSeq, 0); assert.equal(layout.frames.at(-1).lastSeq, 9999)
  assert.ok(layout.frames.every(f => f.h < 1200 && f.n <= 12))
  assert.ok(layout.frames.every((f, i, a) => !i || f.x > a[i - 1].x + a[i - 1].w))
})

test('widened cards and variable details cannot overlap within a section', () => {
  const nodes = make(25); nodes[4].lines = Array(12).fill('A longer evidence detail')
  const layout = journeyLayout(nodes, () => true, n => n.id === 'ev3' ? 600 : 260)
  for (let i = 0; i < nodes.length; i++) for (let j = i + 1; j < nodes.length; j++) {
    const a = layout.pos.get(nodes[i].id), b = layout.pos.get(nodes[j].id)
    const ah = estH(nodes[i], a.w), bh = estH(nodes[j], b.w)
    assert.ok(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + ah <= b.y || b.y + bh <= a.y, `${i} overlaps ${j}`)
  }
})

test('expanding grouped sources preserves originals and does not invent joins', () => {
  const nodes = make(15); nodes[1].member = 'ev0'
  const before = JSON.stringify(nodes), folded = journeyLayout(nodes, n => !n.member), expanded = journeyLayout(nodes, () => true)
  assert.equal(folded.frames.reduce((s, f) => s + f.n, 0), 14)
  assert.equal(expanded.frames.reduce((s, f) => s + f.n, 0), 15)
  assert.equal(JSON.stringify(nodes), before)
})
